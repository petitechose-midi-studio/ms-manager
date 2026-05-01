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
  MidiInventoryStatus,
  Platform,
  Status,
  UxRecorderEvent,
} from "$lib/api/types";
import type { ActivityFilter, ActivityLevel, ActivityScope } from "$lib/state/activity";

export type DashboardState = {
  booting: boolean;
  platform: Platform | null;
  payloadRoot: string | null;
  artifactConfigPath: string | null;
  artifactMessage: string | null;
  tabOrder: string[];
  appUpdate: AppUpdateStatus | null;
  checkingAppUpdate: boolean;
  installingAppUpdate: boolean;
  tags: string[];
  loadingTags: boolean;
  installed: InstallState | null;
  hostInstalled: boolean;
  device: DeviceStatus;
  midiInventory: MidiInventoryStatus | null;
  loadingMidiInventory: boolean;
  bridge: BridgeStatus;
  bridgeMutating: boolean;
  installing: boolean;
  flashing: boolean;
  flashingInstanceId: string | null;
  flashNotice: {
    instanceId: string | null;
    level: "warn";
    message: string;
  } | null;
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
    tabOrder: [],
    appUpdate: null,
    checkingAppUpdate: false,
    installingAppUpdate: false,
    tags: [],
    loadingTags: false,
    installed: null,
    hostInstalled: false,
    device: { connected: false, count: 0, targets: [] },
    midiInventory: null,
    loadingMidiInventory: false,
    bridge: {
      installed: false,
      running: false,
      paused: false,
      serial_open: false,
      instances: [],
    },
    bridgeMutating: false,
    installing: false,
    flashing: false,
    flashingInstanceId: null,
    flashNotice: null,
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function apiErrorSuggestedActions(error: ApiError | null): string[] {
  if (!error || !isRecord(error.details)) return [];
  const actions = error.details.suggested_actions;
  if (!Array.isArray(actions)) return [];
  return actions.filter(
    (value): value is string => typeof value === "string" && value.trim().length > 0,
  );
}

export function applyStatus(state: Writable<DashboardState>, status: Status) {
  state.update((current) => ({
    ...current,
    booting: false,
    platform: status.platform,
    payloadRoot: status.payload_root,
    artifactConfigPath: status.artifact_config_path,
    artifactMessage: status.artifact_message,
    tabOrder: status.tab_order ?? [],
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
  if (event.type === "downloading")
    return `Downloading ${event.index}/${event.total}: ${event.filename}`;
  if (event.type === "applying") return `Applying: ${event.step}`;
  if (event.type === "done") return `Installed ${event.tag} (${event.profile})`;
  return "";
}

export function nowFromFlash(event: FlashEvent): string {
  if (event.type === "begin") return `Flashing firmware: ${event.profile}…`;
  if (event.type === "message") return event.message;
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
export type DashboardUxRecorderEvent = UxRecorderEvent;
export type DashboardBindPreset = "standalone" | "bitwig";

export function bindPresetDefaults(preset: DashboardBindPreset): {
  target: DashboardFirmwareTarget;
  artifactSource: DashboardArtifactSource;
  installedChannel: Channel | null;
} {
  if (preset === "bitwig") {
    return {
      target: "bitwig",
      artifactSource: "installed",
      installedChannel: "stable",
    };
  }

  return {
    target: "standalone",
    artifactSource: "workspace",
    installedChannel: null,
  };
}

export function sortInstanceIdsByTabOrder(instanceIds: string[], tabOrder: string[]): string[] {
  const orderIndex = new Map(tabOrder.map((instanceId, index) => [instanceId, index]));

  return instanceIds
    .map((instanceId, index) => ({
      instanceId,
      index,
      order: orderIndex.get(instanceId) ?? Number.MAX_SAFE_INTEGER,
    }))
    .sort((a, b) => {
      if (a.order !== b.order) return a.order - b.order;
      return a.index - b.index;
    })
    .map((entry) => entry.instanceId);
}
