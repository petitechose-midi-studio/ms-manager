import { invokeApi } from "$lib/api/tauri";
import type { Channel, InstallPlan, LatestManifestResponse, Settings } from "$lib/api/types";

export function settingsGet(): Promise<Settings> {
  return invokeApi<Settings>("settings_get");
}

export function settingsSetChannel(channel: Channel): Promise<Settings> {
  return invokeApi<Settings>("settings_set_channel", { channel });
}

export function resolveLatestManifest(channel: Channel): Promise<LatestManifestResponse> {
  return invokeApi<LatestManifestResponse>("resolve_latest_manifest", { channel });
}

export function planLatestInstall(channel: Channel): Promise<InstallPlan> {
  return invokeApi<InstallPlan>("plan_latest_install", { channel });
}
