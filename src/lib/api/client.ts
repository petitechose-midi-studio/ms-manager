import { invokeApi } from "$lib/api/tauri";
import type {
  Channel,
  BridgeStatus,
  DeviceStatus,
  InstallPlan,
  InstallState,
  LastFlashed,
  LatestManifestResponse,
  Settings,
  Status,
} from "$lib/api/types";

export function statusGet(): Promise<Status> {
  return invokeApi<Status>("status_get");
}

export function deviceStatusGet(): Promise<DeviceStatus> {
  return invokeApi<DeviceStatus>("device_status_get");
}

export function bridgeStatusGet(): Promise<BridgeStatus> {
  return invokeApi<BridgeStatus>("bridge_status_get");
}

export function bridgeLogOpen(): Promise<void> {
  return invokeApi<void>("bridge_log_open");
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

export function settingsSetPinnedTag(pinnedTag: string | null): Promise<Settings> {
  return invokeApi<Settings>("settings_set_pinned_tag", { pinnedTag });
}

export function resolveLatestManifest(channel: Channel): Promise<LatestManifestResponse> {
  return invokeApi<LatestManifestResponse>("resolve_latest_manifest", { channel });
}

export function resolveManifestForTag(channel: Channel, tag: string): Promise<LatestManifestResponse> {
  return invokeApi<LatestManifestResponse>("resolve_manifest_for_tag", { channel, tag });
}

export function listChannelTags(channel: Channel): Promise<string[]> {
  return invokeApi<string[]>("list_channel_tags", { channel });
}

export function planLatestInstall(channel: Channel, profile: string): Promise<InstallPlan> {
  return invokeApi<InstallPlan>("plan_latest_install", { channel, profile });
}

export function installLatest(channel: Channel, profile: string): Promise<InstallState> {
  return invokeApi<InstallState>("install_latest", { channel, profile });
}

export function installSelected(): Promise<InstallState> {
  return invokeApi<InstallState>("install_selected");
}

export function flashFirmware(profile: string): Promise<LastFlashed> {
  return invokeApi<LastFlashed>("flash_firmware", { profile });
}

export function payloadRootRelocate(newRoot: string): Promise<Status> {
  return invokeApi<Status>("payload_root_relocate", { newRoot });
}

export function payloadRootOpen(): Promise<void> {
  return invokeApi<void>("payload_root_open");
}
