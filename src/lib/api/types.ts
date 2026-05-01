export type Channel = "stable" | "beta";
export type ArtifactSource = "installed" | "workspace";
export type FirmwareTarget = "standalone" | "bitwig";

export type Os = "windows" | "macos" | "linux";
export type Arch = "x86_64" | "arm64";

export type ApiError = {
  code: string;
  message: string;
  details?: unknown;
};

export type InstallState = {
  schema: number;
  channel: Channel;
  profile: string;
  tag: string;
};

export type LastFlashed = {
  channel: Channel;
  tag: string;
  profile: string;
  flashed_at_ms: number;
};

export type DeviceStatus = {
  connected: boolean;
  count: number;
  targets: DeviceTarget[];
};

export type MidiInventoryProvider =
  | "windows_midi_services"
  | "winmm"
  | "alsa"
  | "core_midi"
  | "none";

export type MidiPortDirection = "input" | "output" | "bidirectional";
export type MidiMatchConfidence = "strong" | "weak" | "none";

export type MidiPortMatch = {
  controller_serial?: string | null;
  confidence: MidiMatchConfidence;
  reason?: string | null;
};

export type MidiPortInfo = {
  id: string;
  provider: MidiInventoryProvider;
  name: string;
  direction: MidiPortDirection;
  manufacturer?: string | null;
  serial_number?: string | null;
  vid?: number | null;
  pid?: number | null;
  system_device_id?: string | null;
  match_info: MidiPortMatch;
};

export type MidiInventoryStatus = {
  provider: MidiInventoryProvider;
  available: boolean;
  ports: MidiPortInfo[];
  notes: string[];
};

export type BridgeStatus = {
  installed: boolean;
  running: boolean;
  paused: boolean;
  serial_open: boolean;
  version?: string | null;
  message?: string | null;
  instances: BridgeInstanceStatus[];
};

export type BridgeInstanceStatus = {
  instance_id: string;
  display_name?: string | null;
  configured_serial: string;
  target: FirmwareTarget;
  artifact_source: ArtifactSource;
  installed_channel?: Channel | null;
  installed_pinned_tag?: string | null;
  artifacts_ready: boolean;
  artifact_message?: string | null;
  enabled: boolean;
  running: boolean;
  paused: boolean;
  serial_open: boolean;
  version?: string | null;
  resolved_serial_port?: string | null;
  connected_serial?: string | null;
  message?: string | null;
  last_flashed?: LastFlashed | null;
  artifact_location_path?: string | null;
  host_udp_port: number;
  control_port: number;
  log_broadcast_port: number;
};

export type BridgeApp = "bitwig";
export type BridgeMode = "hardware" | "native_sim" | "wasm_sim";

export type BridgeInstanceBinding = {
  instance_id: string;
  display_name?: string | null;
  app: BridgeApp;
  mode: BridgeMode;
  controller_serial: string;
  controller_vid: number;
  controller_pid: number;
  target: FirmwareTarget;
  artifact_source: ArtifactSource;
  installed_channel?: Channel | null;
  installed_pinned_tag?: string | null;
  host_udp_port: number;
  control_port: number;
  log_broadcast_port: number;
  enabled: boolean;
};

export type BridgeInstancesState = {
  schema: number;
  instances: BridgeInstanceBinding[];
};

export type BridgeInstancesResponse = {
  state: BridgeInstancesState;
};

export type BridgeInstanceBindRequest = {
  app: BridgeApp;
  mode: BridgeMode;
  controller_serial: string;
  controller_vid: number;
  controller_pid: number;
  target: FirmwareTarget;
  artifact_source: ArtifactSource;
  installed_channel?: Channel | null;
};

export type BridgeInstanceBindingResponse = {
  binding: BridgeInstanceBinding;
};

export type BridgeInstanceTargetSetRequest = {
  instance_id: string;
  target: FirmwareTarget;
};

export type BridgeInstanceArtifactSourceSetRequest = {
  instance_id: string;
  artifact_source: ArtifactSource;
};

export type BridgeInstanceInstalledReleaseSetRequest = {
  instance_id: string;
  channel: Channel;
  pinned_tag?: string | null;
};

export type BridgeInstanceNameSetRequest = {
  instance_id: string;
  display_name?: string | null;
};

export type DeviceTargetKind = "serial" | "halfkay";

export type DeviceTarget = {
  index: number;
  target_id: string;
  kind: DeviceTargetKind;
  port_name?: string | null;
  path?: string | null;
  serial_number?: string | null;
  manufacturer?: string | null;
  product?: string | null;
  vid: number;
  pid: number;
};

export type Platform = {
  os: Os;
  arch: Arch;
};

export type ManifestRepo = {
  id: string;
  url: string;
  sha: string;
};

export type ManifestAsset = {
  id: string;
  kind: string;
  os?: string | null;
  arch?: string | null;
  filename: string;
  size: number;
  sha256: string;
  url?: string | null;
};

export type ManifestInstallSet = {
  id: string;
  os?: string | null;
  arch?: string | null;
  assets: string[];
};

export type ManifestPages = {
  demo_url?: string | null;
};

export type Manifest = {
  schema: number;
  channel: Channel;
  tag: string;
  published_at: string;
  repos: ManifestRepo[];
  assets: ManifestAsset[];
  install_sets: ManifestInstallSet[];
  pages?: ManifestPages | null;
};

export type Status = {
  installed: InstallState | null;
  host_installed: boolean;
  artifact_source: ArtifactSource;
  artifact_config_path: string | null;
  artifact_message: string | null;
  tab_order: string[];
  platform: Platform;
  payload_root: string;
  device: DeviceStatus;
  bridge: BridgeStatus;
};

export type TabOrderSetRequest = {
  instance_ids: string[];
};

export type TabOrderResponse = {
  tab_order: string[];
};

export type AppUpdateInfo = {
  version: string;
  pub_date?: string | null;
  notes?: string | null;
  url: string;
};

export type AppUpdateStatus = {
  current_version: string;
  available: boolean;
  update: AppUpdateInfo | null;
  error?: string | null;
};

export type InstallEvent =
  | {
      type: "begin";
      channel: Channel;
      tag: string;
      profile: string;
      assets_total: number;
    }
  | {
      type: "downloading";
      index: number;
      total: number;
      asset_id: string;
      filename: string;
    }
  | {
      type: "applying";
      step: string;
    }
  | {
      type: "done";
      tag: string;
      profile: string;
    };

export type FlashEvent =
  | {
      type: "begin";
      channel: Channel;
      tag: string;
      profile: string;
    }
  | {
      type: "message";
      level: "info" | "warn";
      message: string;
    }
  | {
      type: "output";
      line: string;
    }
  | {
      type: "done";
      ok: boolean;
    };

export type BridgeLogEvent = {
  instance_id?: string | null;
  port: number;
  timestamp: string;
  kind: "system" | "debug" | "protocol_in" | "protocol_out";
  level?: "debug" | "info" | "warn" | "error" | null;
  message: string;
};

export type UxRecordingSessionInfo = {
  instance_id: string;
  path: string;
  started_at: string;
  event_count: number;
  raw_event_count: number;
};

export type UxEventPresentation = {
  kind: string;
  action: string;
  control?: string | null;
  value?: string | null;
  target?: string | null;
  effect?: string | null;
  state?: string | null;
  detail?: string | null;
};

export type UxRecorderEvent =
  | {
      type: "session_started";
      instance_id: string;
      path: string;
      trigger: string;
    }
  | {
      type: "event_recorded";
      instance_id: string;
      path: string;
      event_count: number;
      summary: string;
      presentation: UxEventPresentation;
    }
  | {
      type: "session_ended";
      instance_id: string;
      path: string;
      reason: string;
      event_count: number;
      raw_event_count: number;
    }
  | {
      type: "error";
      instance_id?: string | null;
      message: string;
    };
