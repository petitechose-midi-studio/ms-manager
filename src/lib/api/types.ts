export type Channel = "stable" | "beta" | "nightly";

export type Os = "windows" | "macos" | "linux";
export type Arch = "x86_64" | "arm64";

export type ApiError = {
  code: string;
  message: string;
  details?: unknown;
};

export type Settings = {
  schema: number;
  channel: Channel;
  profile: string;
  pinned_tag?: string | null;
  payload_root_override?: string | null;
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

export type BridgeStatus = {
  installed: boolean;
  running: boolean;
  paused: boolean;
  serial_open: boolean;
  version?: string | null;
  message?: string | null;
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

export type LatestManifestResponse = {
  channel: Channel;
  available: boolean;
  tag: string | null;
  manifest: Manifest | null;
  message: string | null;
};

export type AssetPlan = {
  id: string;
  kind: string;
  filename: string;
  sha256: string;
  size: number;
  url: string;
};

export type InstallPlan = {
  channel: Channel;
  tag: string;
  profile: string;
  platform: Platform;
  assets: AssetPlan[];
};

export type Status = {
  settings: Settings;
  installed: InstallState | null;
  host_installed: boolean;
  platform: Platform;
  payload_root: string;
  device: DeviceStatus;
  last_flashed: LastFlashed | null;
  bridge: BridgeStatus;
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
      type: "output";
      line: string;
    }
  | {
      type: "done";
      ok: boolean;
    };
