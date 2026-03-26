import { invokeApi } from "$lib/api/tauri";
import type {
  AppUpdateStatus,
  ArtifactSource,
  BridgeInstanceArtifactSourceSetRequest,
  BridgeInstanceBindingResponse,
  BridgeInstanceBindRequest,
  BridgeInstanceInstalledReleaseSetRequest,
  BridgeInstanceNameSetRequest,
  BridgeInstanceTargetSetRequest,
  BridgeInstancesResponse,
  Channel,
  BridgeStatus,
  DeviceStatus,
  InstallState,
  LastFlashed,
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

export function bridgeInstanceBind(
  request: BridgeInstanceBindRequest,
): Promise<BridgeInstanceBindingResponse> {
  return invokeApi<BridgeInstanceBindingResponse>("bridge_instance_bind", { request });
}

export function bridgeInstanceRemove(instanceId: string): Promise<BridgeInstancesResponse> {
  return invokeApi<BridgeInstancesResponse>("bridge_instance_remove", { instanceId });
}

export function bridgeInstanceEnableSet(
  instanceId: string,
  enabled: boolean,
): Promise<BridgeInstancesResponse> {
  return invokeApi<BridgeInstancesResponse>("bridge_instance_enable_set", { instanceId, enabled });
}

export function bridgeInstanceTargetSet(
  request: BridgeInstanceTargetSetRequest,
): Promise<BridgeInstancesResponse> {
  return invokeApi<BridgeInstancesResponse>("bridge_instance_target_set", { request });
}

export function bridgeInstanceArtifactSourceSet(
  request: BridgeInstanceArtifactSourceSetRequest,
): Promise<BridgeInstancesResponse> {
  return invokeApi<BridgeInstancesResponse>("bridge_instance_artifact_source_set", { request });
}

export function bridgeInstanceInstalledReleaseSet(
  request: BridgeInstanceInstalledReleaseSetRequest,
): Promise<BridgeInstancesResponse> {
  return invokeApi<BridgeInstancesResponse>("bridge_instance_installed_release_set", { request });
}

export function bridgeInstanceNameSet(
  request: BridgeInstanceNameSetRequest,
): Promise<BridgeInstancesResponse> {
  return invokeApi<BridgeInstancesResponse>("bridge_instance_name_set", { request });
}

export function listChannelTags(channel: Channel): Promise<string[]> {
  return invokeApi<string[]>("list_channel_tags", { channel });
}

export function installBridgeInstance(instanceId: string): Promise<InstallState> {
  return invokeApi<InstallState>("install_bridge_instance", { instanceId });
}

export function flashBridgeInstance(instanceId: string): Promise<LastFlashed> {
  return invokeApi<LastFlashed>("flash_bridge_instance", { instanceId });
}

export function payloadRootRelocate(newRoot: string): Promise<Status> {
  return invokeApi<Status>("payload_root_relocate", { newRoot });
}

export function pathOpen(path: string): Promise<void> {
  return invokeApi<void>("path_open", { path });
}

export function appUpdateCheck(): Promise<AppUpdateStatus> {
  return invokeApi<AppUpdateStatus>("app_update_check");
}

export function appUpdateOpenLatest(): Promise<void> {
  return invokeApi<void>("app_update_open_latest");
}
