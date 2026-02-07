import { get, writable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";

import type {
  ApiError,
  AppUpdateStatus,
  Channel,
  DeviceStatus,
  BridgeStatus,
  FlashEvent,
  InstallEvent,
  InstallState,
  LastFlashed,
  LatestManifestResponse,
  Platform,
  Status,
} from "$lib/api/types";
import {
  appUpdateCheck,
  appUpdateOpenLatest,
  deviceStatusGet,
  bridgeStatusGet,
  flashFirmware,
  installSelected,
  listChannelTags,
  payloadRootRelocate,
  resolveLatestManifest,
  resolveManifestForTag,
  settingsSetChannel,
  settingsSetPinnedTag,
  settingsSetProfile,
  statusGet,
} from "$lib/api/client";
import type { ActivityFilter, ActivityLevel, ActivityScope } from "$lib/state/activity";

export type FlashModalState = {
  open: boolean;
  targetProfile: string | null;
  ack: boolean;
};

export type ContextMenuState = {
  open: boolean;
  x: number;
  y: number;
  targetProfile: string | null;
};

export type DashboardState = {
  platform: Platform | null;
  payloadRoot: string | null;

  appUpdate: AppUpdateStatus | null;
  checkingAppUpdate: boolean;
  installingAppUpdate: boolean;

  channel: Channel;
  pinnedTag: string | null;
  tags: string[];
  loadingTags: boolean;

  profile: string;
  profileOptions: string[];

  installed: InstallState | null;
  hostInstalled: boolean;

  device: DeviceStatus;
  lastFlashed: LastFlashed | null;

  bridge: BridgeStatus;

  release: LatestManifestResponse | null;
  loadingRelease: boolean;
  savingSettings: boolean;
  installing: boolean;
  flashing: boolean;
  flashProgress: number | null;

  relocating: boolean;
  now: string | null;
  error: ApiError | null;

  activityOpen: boolean;
  activityFilter: ActivityFilter;

  flashModal: FlashModalState;
  relocateModal: {
    open: boolean;
    nextRoot: string;
    ack: boolean;
  };
  ctxMenu: ContextMenuState;
};

type Activity = {
  add: (level: ActivityLevel, scope: ActivityScope, message: string, details?: unknown) => void;
};

const INSTALL_EVENT = "ms-manager://install";
const FLASH_EVENT = "ms-manager://flash";

export function createDashboard(activity: Activity) {
  const state = writable<DashboardState>({
    platform: null,
    payloadRoot: null,

    appUpdate: null,
    checkingAppUpdate: false,
    installingAppUpdate: false,

    channel: "stable",
    pinnedTag: null,
    tags: [],
    loadingTags: false,

    profile: "default",
    profileOptions: ["default", "bitwig"],

    installed: null,
    hostInstalled: false,

    device: { connected: false, count: 0, targets: [] },
    lastFlashed: null,

    bridge: { installed: false, running: false, paused: false, serial_open: false },

    release: null,
    loadingRelease: false,
    savingSettings: false,
    installing: false,
    flashing: false,
    flashProgress: null,

    relocating: false,
    now: null,
    error: null,

    activityOpen: false,
    activityFilter: "all",

    flashModal: { open: false, targetProfile: null, ack: false },
    relocateModal: { open: false, nextRoot: "", ack: false },
    ctxMenu: { open: false, x: 0, y: 0, targetProfile: null },
  });

  function setError(e: unknown) {
    const err = e as ApiError;
    state.update((s) => ({ ...s, error: err }));
    activity.add("error", "ui", err.message, err.details);
  }

  function clearError() {
    state.update((s) => ({ ...s, error: null }));
  }

  function nowFromInstall(e: InstallEvent): string {
    if (e.type === "begin") return `Installing ${e.tag} (${e.profile})…`;
    if (e.type === "downloading") return `Downloading ${e.index}/${e.total}: ${e.filename}`;
    if (e.type === "applying") return `Applying: ${e.step}`;
    if (e.type === "done") return `Installed ${e.tag} (${e.profile})`;
    return "";
  }

  function parseJsonLine(line: string): unknown | null {
    try {
      return JSON.parse(line);
    } catch {
      return null;
    }
  }

  function extractPercent(line: string): number | null {
    const v = parseJsonLine(line);
    if (!v || typeof v !== "object") return null;

    const obj = v as Record<string, unknown>;

    // midi-studio-loader flash emits block events: { event: "block", i, n }
    if (obj.event === "block") {
      const i = typeof obj.i === "number" ? obj.i : null;
      const n = typeof obj.n === "number" ? obj.n : null;
      if (i != null && n != null && n > 0) {
        const pct = ((i + 1) / n) * 100;
        if (Number.isFinite(pct)) return pct;
      }
    }
    const keys = ["percent", "percent_complete", "progress_percent", "pct"];
    for (const k of keys) {
      const n = obj[k];
      if (typeof n === "number" && Number.isFinite(n)) {
        if (n >= 0 && n <= 100) return n;
      }
    }

    const progress = obj.progress;
    if (progress && typeof progress === "object") {
      const p = progress as Record<string, unknown>;
      const n = p.percent;
      if (typeof n === "number" && Number.isFinite(n)) {
        if (n >= 0 && n <= 100) return n;
      }
    }
    return null;
  }

  function nowFromFlash(e: FlashEvent): string {
    if (e.type === "begin") return `Flashing firmware: ${e.profile}…`;
    if (e.type === "output") {
      const line = e.line.trim();
      const pct = extractPercent(line);
      if (pct != null && pct > 0) {
        const p = Math.max(1, Math.min(100, Math.round(pct)));
        return `Flashing… ${p}%`;
      }

      const v = parseJsonLine(line);
      if (v && typeof v === "object") {
        const obj = v as Record<string, unknown>;
        const ev = typeof obj.event === "string" ? obj.event : null;
        if (ev === "discover_start") return "Waiting for controller…";
        if (ev === "discover_done") {
          const count = typeof obj.count === "number" ? obj.count : null;
          if (count === 0) return "Waiting for controller…";
        }
        if (ev === "hex_loaded") return "Firmware loaded…";
      }

      // Keep it short; full output goes to Activity.
      if (line.length > 80) return `${line.slice(0, 80)}…`;
      return line;
    }
    if (e.type === "done") return e.ok ? "Flash done" : "Flash failed";
    return "";
  }

  function applyStatus(st: Status) {
    state.update((s) => ({
      ...s,
      channel: st.settings.channel,
      pinnedTag: st.settings.pinned_tag ?? null,
      profile: st.settings.profile,
      platform: st.platform,
      payloadRoot: st.payload_root,
      installed: st.installed,
      hostInstalled: st.host_installed,
      device: st.device,
      lastFlashed: st.last_flashed,
      bridge: st.bridge,
    }));
  }

  async function refreshStatus() {
    activity.add("info", "ui", "status refresh");
    const st: Status = await statusGet();
    applyStatus(st);
  }

  async function refreshTags() {
    state.update((s) => ({ ...s, loadingTags: true }));
    try {
      const snap = get(state);
      activity.add("info", "net", `list tags channel=${snap.channel}`);
      const tags = await listChannelTags(snap.channel);

      const pinned = snap.pinnedTag;
      const out = pinned && !tags.includes(pinned) ? [pinned, ...tags] : tags;
      state.update((s) => ({ ...s, tags: out }));
      activity.add("ok", "net", `tags count=${out.length}`);
    } catch (e) {
      activity.add("warn", "net", "list tags failed", e);
    } finally {
      state.update((s) => ({ ...s, loadingTags: false }));
    }
  }

  async function setPinnedTag(next: string | null) {
    state.update((s) => ({ ...s, savingSettings: true }));
    clearError();
    try {
      activity.add("info", "ui", next ? `pin tag=${next}` : "pin tag=latest");
      const s = await settingsSetPinnedTag(next);
      state.update((st) => ({ ...st, pinnedTag: s.pinned_tag ?? null }));
      await refreshRelease();
    } catch (e) {
      setError(e);
    } finally {
      state.update((s) => ({ ...s, savingSettings: false }));
    }
  }

  async function refreshRelease() {
    state.update((s) => ({ ...s, loadingRelease: true }));
    clearError();
    try {
      const snap = get(state);
      const channel = snap.channel;
      const platform = snap.platform;
      const profile = snap.profile;
      const pinnedTag = snap.pinnedTag;

      activity.add(
        "info",
        "net",
        pinnedTag ? `resolve release channel=${channel} tag=${pinnedTag}` : `resolve release channel=${channel} tag=latest`
      );

      const out = pinnedTag
        ? await resolveManifestForTag(channel, pinnedTag)
        : await resolveLatestManifest(channel);

      state.update((s) => ({ ...s, release: out }));

      if (!out.available) {
        activity.add("warn", "net", out.message ?? "no release");
      } else {
        activity.add("ok", "net", `release tag=${out.tag ?? "?"}`);
      }

      const m = out.manifest;
      const p = platform;
      if (m && p) {
        const ids = m.install_sets
          .filter((set) => set.os === p.os && set.arch === p.arch)
          .map((set) => set.id);
        const uniq = Array.from(new Set(ids));
        const options = uniq.length ? uniq : ["default"];
        state.update((s) => ({ ...s, profileOptions: options }));

        if (!options.includes(profile!)) {
          const next = options[0] ?? "default";
          await settingsSetProfile(next);
          state.update((s) => ({ ...s, profile: next }));
        }
      }
    } catch (e) {
      setError(e);
    } finally {
      state.update((s) => ({ ...s, loadingRelease: false }));
    }
  }

  async function checkAppUpdate() {
    state.update((s) => ({ ...s, checkingAppUpdate: true }));
    try {
      activity.add("info", "net", "check app update");
      const out = await appUpdateCheck();
      state.update((s) => ({ ...s, appUpdate: out }));

      if (out.error) {
        activity.add("warn", "net", `app update check failed: ${out.error}`);
      } else if (out.available && out.update) {
        activity.add("ok", "net", `app update available: ${out.update.version}`);
      } else {
        activity.add("ok", "net", "app is up to date");
      }
    } catch (e) {
      activity.add("warn", "net", "app update check failed", e);
    } finally {
      state.update((s) => ({ ...s, checkingAppUpdate: false }));
    }
  }

  async function installAppUpdate() {
    const snap = get(state);
    if (!snap.appUpdate?.available) return;
    if (snap.installingAppUpdate) return;
    if (snap.installing || snap.flashing || snap.relocating || snap.savingSettings) return;

    state.update((s) => ({ ...s, installingAppUpdate: true }));
    clearError();
    activity.add("info", "ui", "opening ms-manager latest release page");
    try {
      await appUpdateOpenLatest();
    } catch (e) {
      setError(e);
    } finally {
      state.update((s) => ({ ...s, installingAppUpdate: false }));
    }
  }

  async function setChannel(next: Channel) {
    state.update((s) => ({ ...s, channel: next }));
    state.update((s) => ({ ...s, savingSettings: true }));
    clearError();
    try {
      activity.add("info", "ui", `set channel=${next}`);
      const s = await settingsSetChannel(next);
      state.update((st) => ({
        ...st,
        channel: s.channel,
        pinnedTag: s.pinned_tag ?? null,
        profile: s.profile,
      }));
      await refreshTags();
      await refreshRelease();
    } catch (e) {
      setError(e);
    } finally {
      state.update((s) => ({ ...s, savingSettings: false }));
    }
  }

  async function setProfile(next: string) {
    state.update((s) => ({ ...s, savingSettings: true }));
    clearError();
    try {
      activity.add("info", "ui", `set profile=${next}`);
      const s = await settingsSetProfile(next);
      state.update((st) => ({ ...st, profile: s.profile }));
    } catch (e) {
      setError(e);
    } finally {
      state.update((s) => ({ ...s, savingSettings: false }));
    }
  }

  function openContextMenu(profile: string, x: number, y: number) {
    state.update((s) => ({
      ...s,
      ctxMenu: { open: true, x, y, targetProfile: profile },
    }));
  }

  function closeContextMenu() {
    state.update((s) => ({ ...s, ctxMenu: { open: false, x: 0, y: 0, targetProfile: null } }));
  }

  function openFlashModal(profile: string) {
    state.update((s) => ({
      ...s,
      flashModal: { open: true, targetProfile: profile, ack: false },
    }));
  }

  function cancelFlashModal() {
    state.update((s) => ({
      ...s,
      flashModal: { open: false, targetProfile: null, ack: false },
    }));
  }

  function setFlashAck(ack: boolean) {
    state.update((s) => ({ ...s, flashModal: { ...s.flashModal, ack } }));
  }

  function openRelocateModal() {
    const snap = get(state);
    state.update((s) => ({
      ...s,
      relocateModal: { open: true, nextRoot: snap.payloadRoot ?? "", ack: false },
    }));
  }

  function cancelRelocateModal() {
    state.update((s) => ({ ...s, relocateModal: { open: false, nextRoot: "", ack: false } }));
  }

  function setRelocateRoot(nextRoot: string) {
    state.update((s) => ({ ...s, relocateModal: { ...s.relocateModal, nextRoot } }));
  }

  function setRelocateAck(ack: boolean) {
    state.update((s) => ({ ...s, relocateModal: { ...s.relocateModal, ack } }));
  }

  async function browseRelocateRoot() {
    const snap = get(state);
    const base = snap.payloadRoot ?? undefined;
    try {
      const mod = await import("@tauri-apps/plugin-dialog");
      const selected = await mod.open({
        title: "Select installation folder",
        directory: true,
        multiple: false,
        defaultPath: base,
      });
      if (typeof selected === "string" && selected.trim()) {
        state.update((s) => ({
          ...s,
          relocateModal: { ...s.relocateModal, nextRoot: selected },
        }));
      }
    } catch (e) {
      activity.add("warn", "ui", "folder picker failed", e);
    }
  }

  async function confirmRelocateModal() {
    const snap = get(state);
    const nextRoot = snap.relocateModal.nextRoot.trim();
    if (!snap.relocateModal.ack || !nextRoot) {
      return;
    }

    state.update((s) => ({ ...s, relocating: true }));
    clearError();
    activity.add("info", "fs", `relocate payload root -> ${nextRoot}`);
    try {
      const out = await payloadRootRelocate(nextRoot);
      applyStatus(out);
      activity.add("ok", "fs", `payload root: ${out.payload_root}`);
      cancelRelocateModal();
      await refreshTags();
      await refreshRelease();
    } catch (e) {
      setError(e);
    } finally {
      state.update((s) => ({ ...s, relocating: false }));
    }
  }

  async function confirmFlashModal() {
    const snap = get(state);
    const target = snap.flashModal.targetProfile;
    const ack = snap.flashModal.ack;

    if (!target || !ack) {
      return;
    }

    state.update((s) => ({ ...s, flashing: true, flashProgress: null }));
    clearError();
    activity.add("info", "flash", `flash start profile=${target}`);
    try {
      const out = await flashFirmware(target);
      state.update((s) => ({ ...s, lastFlashed: out }));
      activity.add("ok", "flash", `flash done profile=${out.profile}`);
      cancelFlashModal();
      await refreshStatus();
    } catch (e) {
      setError(e);
    } finally {
      state.update((s) => ({ ...s, flashing: false, flashProgress: null }));
    }
  }

  async function install() {
    state.update((s) => ({ ...s, installing: true }));
    clearError();
    try {
      await installSelected();
      await refreshStatus();
      await refreshRelease();
    } catch (e) {
      setError(e);
    } finally {
      state.update((s) => ({ ...s, installing: false }));
    }
  }

  function toggleActivity() {
    state.update((s) => ({ ...s, activityOpen: !s.activityOpen }));
  }

  function setActivityFilter(filter: ActivityFilter) {
    state.update((s) => ({ ...s, activityFilter: filter }));
  }

  async function start() {
    activity.add("info", "ui", "boot");
    try {
      await refreshStatus();
      await refreshTags();
      await refreshRelease();
      void checkAppUpdate();
    } catch (e) {
      setError(e);
    }

    const unlistenInstall = await listen<InstallEvent>(INSTALL_EVENT, (e) => {
      const now = nowFromInstall(e.payload);
      state.update((s) => ({ ...s, now }));

      if (e.payload.type === "begin") {
        activity.add("info", "install", `begin ${e.payload.tag} (${e.payload.profile})`);
      } else if (e.payload.type === "downloading") {
        activity.add(
          "info",
          "install",
          `download ${e.payload.index}/${e.payload.total} ${e.payload.filename}`
        );
      } else if (e.payload.type === "applying") {
        activity.add("info", "install", `apply ${e.payload.step}`);
      } else if (e.payload.type === "done") {
        activity.add("ok", "install", `done ${e.payload.tag} (${e.payload.profile})`);
      }
    });

    const unlistenFlash = await listen<FlashEvent>(FLASH_EVENT, (e) => {
      const now = nowFromFlash(e.payload);
      state.update((s) => ({ ...s, now }));

      if (e.payload.type === "begin") {
        state.update((s) => ({ ...s, flashProgress: null }));
        activity.add("info", "flash", `begin ${e.payload.tag} (${e.payload.profile})`);
      } else if (e.payload.type === "output") {
        const pct = extractPercent(e.payload.line);
        if (pct != null && pct > 0) {
          const p = Math.max(1, Math.min(100, Math.round(pct)));
          state.update((s) => ({ ...s, flashProgress: p }));
        }
        activity.add("info", "flash", e.payload.line);
      } else if (e.payload.type === "done") {
        state.update((s) => ({ ...s, flashProgress: null }));
        activity.add(e.payload.ok ? "ok" : "error", "flash", e.payload.ok ? "done" : "failed");
      }
    });

    let lastDeviceSig: string | null = null;
    let polling = false;
    const poll = setInterval(async () => {
      if (polling) return;
      const snap = get(state);
      if (snap.relocating) return;

      polling = true;
      try {
        const d = await deviceStatusGet();
        const sig = `${d.connected}:${d.count}`;
        if (sig !== lastDeviceSig) {
          lastDeviceSig = sig;
          activity.add(
            "info",
            "device",
            d.connected ? `controller detected (${d.count})` : "controller not detected"
          );
        }
        state.update((s) => ({ ...s, device: d }));
      } catch {
        // ignore
      } finally {
        polling = false;
      }
    }, 4000);

    let bridgePolling = false;
    const pollBridge = setInterval(async () => {
      if (bridgePolling) return;
      const snap = get(state);
      if (snap.relocating) return;

      bridgePolling = true;
      try {
        const b = await bridgeStatusGet();
        state.update((s) => ({ ...s, bridge: b }));
      } catch {
        // ignore
      } finally {
        bridgePolling = false;
      }
    }, 2000);

    return () => {
      unlistenInstall();
      unlistenFlash();
      clearInterval(poll);
      clearInterval(pollBridge);
    };
  }

  return {
    state,
    start,
    checkAppUpdate,
    installAppUpdate,
    refreshRelease,
    setChannel,
    setPinnedTag,
    setProfile,
    install,
    openFlashModal,
    cancelFlashModal,
    confirmFlashModal,
    setFlashAck,
    openRelocateModal,
    cancelRelocateModal,
    setRelocateRoot,
    setRelocateAck,
    browseRelocateRoot,
    confirmRelocateModal,
    openContextMenu,
    closeContextMenu,
    toggleActivity,
    setActivityFilter,
  };
}
