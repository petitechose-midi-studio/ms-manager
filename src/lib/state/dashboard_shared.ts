import type { Writable } from "svelte/store";

import { normalizeApiError } from "$lib/api/tauri";
import type {
  ApiError,
  AppUpdateStatus,
  ArtifactSource,
  BridgeLogEvent,
  BridgeMode,
  BridgeStatus,
  Channel,
  DeviceStatus,
  DeviceTarget,
  FirmwareTarget,
  FlashEvent,
  InstallEvent,
  InstallState,
  Platform,
  Status,
} from "$lib/api/types";
import type { ActivityFilter, ActivityLevel, ActivityScope } from "$lib/state/activity";

export type DashboardState = {
  booting: boolean;
  platform: Platform | null;
  payloadRoot: string | null;
  artifactConfigPath: string | null;
  artifactMessage: string | null;
  appUpdate: AppUpdateStatus | null;
  checkingAppUpdate: boolean;
  installingAppUpdate: boolean;
  tags: string[];
  loadingTags: boolean;
  installed: InstallState | null;
  hostInstalled: boolean;
  device: DeviceStatus;
  bridge: BridgeStatus;
  bridgeMutating: boolean;
  installing: boolean;
  flashing: boolean;
  flashingInstanceId: string | null;
  activeBridgeInstanceId: string | null;
  relocating: boolean;
  now: string | null;
  error: ApiError | null;
  activityOpen: boolean;
  activityFilter: ActivityFilter;
  relocateModal: {
    open: boolean;
    nextRoot: string;
    ack: boolean;
  };
};

export type Activity = {
  add: (level: ActivityLevel, scope: ActivityScope, message: string, details?: unknown) => void;
  addMany: (
    entries: {
      level: ActivityLevel;
      scope: ActivityScope;
      message: string;
      details?: unknown;
    }[],
  ) => void;
  retain: (predicate: (entry: { scope: ActivityScope; details?: unknown }) => boolean) => void;
};

export function createInitialDashboardState(): DashboardState {
  return {
    booting: true,
    platform: null,
    payloadRoot: null,
    artifactConfigPath: null,
    artifactMessage: null,
    appUpdate: null,
    checkingAppUpdate: false,
    installingAppUpdate: false,
    tags: [],
    loadingTags: false,
    installed: null,
    hostInstalled: false,
    device: { connected: false, count: 0, targets: [] },
    bridge: { installed: false, running: false, paused: false, serial_open: false, instances: [] },
    bridgeMutating: false,
    installing: false,
    flashing: false,
    flashingInstanceId: null,
    activeBridgeInstanceId: null,
    relocating: false,
    now: null,
    error: null,
    activityOpen: false,
    activityFilter: "all",
    relocateModal: { open: false, nextRoot: "", ack: false },
  };
}

export function setApiError(state: Writable<DashboardState>, activity: Activity, error: unknown) {
  const err = normalizeApiError(error);
  state.update((current) => ({ ...current, error: err }));
  activity.add("error", "ui", err.message, err.details);
}

export function clearApiError(state: Writable<DashboardState>) {
  state.update((current) => ({ ...current, error: null }));
}

export function applyStatus(state: Writable<DashboardState>, status: Status) {
  state.update((current) => ({
    ...current,
    booting: false,
    platform: status.platform,
    payloadRoot: status.payload_root,
    artifactConfigPath: status.artifact_config_path,
    artifactMessage: status.artifact_message,
    installed: status.installed,
    hostInstalled: status.host_installed,
    device: status.device,
    bridge: status.bridge,
  }));
}

function parseJsonLine(line: string): unknown | null {
  try {
    return JSON.parse(line);
  } catch {
    return null;
  }
}

export function extractPercent(line: string): number | null {
  const value = parseJsonLine(line);
  if (!value || typeof value !== "object") return null;

  const obj = value as Record<string, unknown>;
  if (obj.event === "block") {
    const i = typeof obj.i === "number" ? obj.i : null;
    const n = typeof obj.n === "number" ? obj.n : null;
    if (i != null && n != null && n > 0) {
      const pct = ((i + 1) / n) * 100;
      if (Number.isFinite(pct)) return pct;
    }
  }

  for (const key of ["percent", "percent_complete", "progress_percent", "pct"]) {
    const percent = obj[key];
    if (typeof percent === "number" && Number.isFinite(percent) && percent >= 0 && percent <= 100) {
      return percent;
    }
  }

  const progress = obj.progress;
  if (progress && typeof progress === "object") {
    const percent = (progress as Record<string, unknown>).percent;
    if (typeof percent === "number" && Number.isFinite(percent) && percent >= 0 && percent <= 100) {
      return percent;
    }
  }

  return null;
}

export function nowFromInstall(event: InstallEvent): string {
  if (event.type === "begin") return `Installing ${event.tag} (${event.profile})…`;
  if (event.type === "downloading") return `Downloading ${event.index}/${event.total}: ${event.filename}`;
  if (event.type === "applying") return `Applying: ${event.step}`;
  if (event.type === "done") return `Installed ${event.tag} (${event.profile})`;
  return "";
}

export function nowFromFlash(event: FlashEvent): string {
  if (event.type === "begin") return `Flashing firmware: ${event.profile}…`;
  if (event.type === "output") {
    const pct = extractPercent(event.line.trim());
    if (pct != null && pct > 0) {
      return `Flashing… ${Math.max(1, Math.min(100, Math.round(pct)))}%`;
    }
    const line = event.line.trim();
    if (line.length > 80) return `${line.slice(0, 80)}…`;
    return line;
  }
  if (event.type === "done") return event.ok ? "Flash done" : "Flash failed";
  return "";
}

export type DashboardMutationDeps = {
  state: Writable<DashboardState>;
  activity: Activity;
  refreshStatus: () => Promise<void>;
};

export type DashboardStatusDeps = {
  state: Writable<DashboardState>;
  activity: Activity;
};

export type DashboardBindTarget = Pick<DeviceTarget, "serial_number" | "vid" | "pid">;
export type DashboardBridgeMode = BridgeMode;
export type DashboardArtifactSource = ArtifactSource;
export type DashboardFirmwareTarget = FirmwareTarget;
export type DashboardBridgeLogEvent = BridgeLogEvent;
