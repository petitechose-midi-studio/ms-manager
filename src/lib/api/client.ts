import { invokeApi } from "$lib/api/tauri";
import type {
  Channel,
  InstallPlan,
  InstallState,
  LatestManifestResponse,
  Settings,
  Status,
} from "$lib/api/types";

export function statusGet(): Promise<Status> {
  return invokeApi<Status>("status_get");
}

export function settingsGet(): Promise<Settings> {
  return invokeApi<Settings>("settings_get");
}

export function settingsSetChannel(channel: Channel): Promise<Settings> {
  return invokeApi<Settings>("settings_set_channel", { channel });
}

export function settingsSetProfile(profile: string): Promise<Settings> {
  return invokeApi<Settings>("settings_set_profile", { profile });
}

export function resolveLatestManifest(channel: Channel): Promise<LatestManifestResponse> {
  return invokeApi<LatestManifestResponse>("resolve_latest_manifest", { channel });
}

export function planLatestInstall(channel: Channel, profile: string): Promise<InstallPlan> {
  return invokeApi<InstallPlan>("plan_latest_install", { channel, profile });
}

export function installLatest(channel: Channel, profile: string): Promise<InstallState> {
  return invokeApi<InstallState>("install_latest", { channel, profile });
}
