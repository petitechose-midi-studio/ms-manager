import { get, writable } from "svelte/store";
import { listen } from "@tauri-apps/api/event";

import type { FlashEvent, InstallEvent } from "$lib/api/types";
import { bridgeStatusGet, deviceStatusGet } from "$lib/api/client";
import type { ActivityFilter, ActivityLevel, ActivityScope } from "$lib/state/activity";
import { createDashboardMutationController } from "$lib/state/dashboard_mutations";
import {
  createInitialDashboardState,
  nowFromFlash,
  nowFromInstall,
  type DashboardBridgeLogEvent,
  type DashboardState,
  type Activity,
} from "$lib/state/dashboard_shared";
import { createDashboardStatusController } from "$lib/state/dashboard_status";

const INSTALL_EVENT = "ms-manager://install";
const FLASH_EVENT = "ms-manager://flash";
const BRIDGE_LOG_EVENT = "ms-manager://bridge-log";

export type { DashboardState } from "$lib/state/dashboard_shared";

export function createDashboard(activity: Activity) {
  const state = writable<DashboardState>(createInitialDashboardState());
  const status = createDashboardStatusController({ state, activity });
  const mutations = createDashboardMutationController({
    state,
    activity,
    refreshStatus: status.refreshStatus,
  });

  let bridgeLogQueue: {
    level: ActivityLevel;
    scope: ActivityScope;
    message: string;
    details?: unknown;
  }[] = [];
  let bridgeLogFlushHandle: number | null = null;

  function flushBridgeLogs() {
    if (!bridgeLogQueue.length) return;
    activity.addMany(bridgeLogQueue);
    bridgeLogQueue = [];
  }

  function queueBridgeLog(
    level: ActivityLevel,
    scope: ActivityScope,
    message: string,
    details?: unknown,
  ) {
    bridgeLogQueue.push({ level, scope, message, details });
    if (bridgeLogFlushHandle != null) return;
    bridgeLogFlushHandle = requestAnimationFrame(() => {
      bridgeLogFlushHandle = null;
      flushBridgeLogs();
    });
  }

  async function copyBridgeEventsToActivity(payload: DashboardBridgeLogEvent) {
    const snapshot = get(state);
    if (
      payload.instance_id &&
      snapshot.activeBridgeInstanceId &&
      payload.instance_id !== snapshot.activeBridgeInstanceId
    ) {
      return;
    }

    const matchedInstance =
      payload.instance_id == null
        ? null
        : snapshot.bridge.instances.find((instance) => instance.instance_id === payload.instance_id);
    const displayName =
      matchedInstance?.display_name?.trim() || matchedInstance?.configured_serial || payload.instance_id;
    const prefix = displayName ? `[${displayName}] ` : "";
    const level =
      payload.level === "error"
        ? "error"
        : payload.level === "warn"
          ? "warn"
          : "info";
    queueBridgeLog(level, "bridge", `${prefix}${payload.message}`, payload);
  }

  async function start() {
    function deviceSignatureOf(device: { targets?: { target_id: string; serial_number?: string | null }[] }) {
      return JSON.stringify(
        (device.targets ?? [])
          .map((target) => `${target.target_id}:${target.serial_number ?? ""}`)
          .sort(),
      );
    }

    let lastDeviceSignature = "";

    activity.add("info", "ui", "boot");
    try {
      await status.refreshStatus();
      lastDeviceSignature = deviceSignatureOf(get(state).device);
      void status.checkAppUpdate();
    } catch (error) {
      activity.add("error", "ui", "boot failed", error);
    }

    const unlistenInstall = await listen<InstallEvent>(INSTALL_EVENT, (event) => {
      state.update((current) => ({ ...current, now: nowFromInstall(event.payload) }));
      if (event.payload.type === "done") {
        activity.add("ok", "install", `done ${event.payload.tag} (${event.payload.profile})`);
      }
    });

    const unlistenFlash = await listen<FlashEvent>(FLASH_EVENT, (event) => {
      const payload = event.payload;
      if (payload.type === "message") {
        if (payload.level === "warn") {
          activity.add("warn", "flash", payload.message);
        }
        state.update((current) => ({
          ...current,
          now: nowFromFlash(payload),
          flashNotice:
            payload.level === "warn"
              ? {
                  instanceId: current.flashingInstanceId,
                  level: "warn",
                  message: payload.message,
                }
              : current.flashNotice,
        }));
        return;
      }

      state.update((current) => ({
        ...current,
        now: nowFromFlash(payload),
        flashNotice: payload.type === "begin" ? null : current.flashNotice,
      }));
    });

    const unlistenBridgeLog = await listen<DashboardBridgeLogEvent>(BRIDGE_LOG_EVENT, (event) => {
      void copyBridgeEventsToActivity(event.payload);
    });

    let devicePolling = false;
    const pollDevice = setInterval(async () => {
      if (devicePolling || get(state).relocating) return;
      devicePolling = true;
      try {
        const device = await deviceStatusGet();
        const nextSignature = deviceSignatureOf(device);
        const changed = nextSignature !== lastDeviceSignature;
        lastDeviceSignature = nextSignature;
        state.update((current) => ({ ...current, device }));
        if (changed) {
          void status.refreshMidiInventory({ log: false });
        }
      } finally {
        devicePolling = false;
      }
    }, 4000);

    let bridgePolling = false;
    const pollBridge = setInterval(async () => {
      if (bridgePolling || get(state).relocating) return;
      bridgePolling = true;
      try {
        const bridge = await bridgeStatusGet();
        state.update((current) => ({ ...current, bridge }));
      } finally {
        bridgePolling = false;
      }
    }, 2000);

    return () => {
      if (bridgeLogFlushHandle != null) {
        cancelAnimationFrame(bridgeLogFlushHandle);
        bridgeLogFlushHandle = null;
      }
      flushBridgeLogs();
      unlistenInstall();
      unlistenFlash();
      unlistenBridgeLog();
      clearInterval(pollDevice);
      clearInterval(pollBridge);
    };
  }

  return {
    state,
    start,
    ...status,
    ...mutations,
    setActivityFilter(filter: ActivityFilter) {
      mutations.setActivityFilter(filter);
    },
  };
}
