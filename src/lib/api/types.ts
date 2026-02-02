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
  pinned_tag?: string | null;
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
  filename: string;
  sha256: string;
  size: number;
  url: string;
};

export type InstallPlan = {
  channel: Channel;
  tag: string;
  platform: Platform;
  assets: AssetPlan[];
};
