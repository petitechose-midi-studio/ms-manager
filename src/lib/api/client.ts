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
  ControllerFsBridgeRequest,
  ControllerFsCapabilities,
  ControllerFsDeleteRequest,
  ControllerFsListEntry,
  ControllerFsPathRequest,
  ControllerFsPullFileRequest,
  ControllerFsPushFileRequest,
  ControllerFsRenameRequest,
  ControllerFsTransferResponse,
  DeviceStatus,
  InstallState,
  LastFlashed,
  LocalFsDeleteRequest,
  LocalFsListRequest,
  LocalFsListResponse,
  LocalFsPathRequest,
  LocalFsRenameRequest,
  MidiInventoryStatus,
  ProjectMigrationInspectRequest,
  ProjectMigrationMigrateRequest,
  ProjectMigrationReport,
  RemoteStepPresetIdentityRequest,
  RemoteStepPresetInspectRequest,
  RemoteStepPresetRenameRequest,
  Status,
  StepPresetInspectRequest,
  StepPresetIdentityRequest,
  StepPresetRenameRequest,
  StepPresetReport,
  TabOrderResponse,
  TabOrderSetRequest,
  UxRecordingSessionInfo,
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

export function controllerFsCapabilitiesGet(
  request: ControllerFsBridgeRequest,
): Promise<ControllerFsCapabilities> {
  return invokeApi<ControllerFsCapabilities>("controller_fs_capabilities_get", { request });
}

export function controllerFsList(request: ControllerFsPathRequest): Promise<ControllerFsListEntry[]> {
  return invokeApi<ControllerFsListEntry[]>("controller_fs_list", { request });
}

export function controllerFsMkdir(request: ControllerFsPathRequest): Promise<void> {
  return invokeApi<void>("controller_fs_mkdir", { request });
}

export function controllerFsDelete(request: ControllerFsDeleteRequest): Promise<void> {
  return invokeApi<void>("controller_fs_delete", { request });
}

export function controllerFsRename(request: ControllerFsRenameRequest): Promise<void> {
  return invokeApi<void>("controller_fs_rename", { request });
}

export function controllerFsPullFile(
  request: ControllerFsPullFileRequest,
): Promise<ControllerFsTransferResponse> {
  return invokeApi<ControllerFsTransferResponse>("controller_fs_pull_file", { request });
}

export function controllerFsPushFile(
  request: ControllerFsPushFileRequest,
): Promise<ControllerFsTransferResponse> {
  return invokeApi<ControllerFsTransferResponse>("controller_fs_push_file", { request });
}

export function projectMigrationInspect(
  request: ProjectMigrationInspectRequest,
): Promise<ProjectMigrationReport> {
  return invokeApi<ProjectMigrationReport>("project_migration_inspect", { request });
}

export function projectMigrationMigrate(
  request: ProjectMigrationMigrateRequest,
): Promise<ProjectMigrationReport> {
  return invokeApi<ProjectMigrationReport>("project_migration_migrate", { request });
}

export function stepPresetInspect(request: StepPresetInspectRequest): Promise<StepPresetReport> {
  return invokeApi<StepPresetReport>("step_preset_inspect", { request });
}

export function stepPresetValidate(request: StepPresetInspectRequest): Promise<StepPresetReport> {
  return invokeApi<StepPresetReport>("step_preset_validate", { request });
}

export function stepPresetRename(request: StepPresetRenameRequest): Promise<StepPresetReport> {
  return invokeApi<StepPresetReport>("step_preset_rename", { request });
}

export function stepPresetDelete(request: StepPresetIdentityRequest): Promise<StepPresetReport> {
  return invokeApi<StepPresetReport>("step_preset_delete", { request });
}

export function remoteStepPresetInspect(
  request: RemoteStepPresetInspectRequest,
): Promise<StepPresetReport> {
  return invokeApi<StepPresetReport>("remote_step_preset_inspect", { request });
}

export function remoteStepPresetValidate(
  request: RemoteStepPresetInspectRequest,
): Promise<StepPresetReport> {
  return invokeApi<StepPresetReport>("remote_step_preset_validate", { request });
}

export function remoteStepPresetRename(
  request: RemoteStepPresetRenameRequest,
): Promise<StepPresetReport> {
  return invokeApi<StepPresetReport>("remote_step_preset_rename", { request });
}

export function remoteStepPresetDelete(
  request: RemoteStepPresetIdentityRequest,
): Promise<StepPresetReport> {
  return invokeApi<StepPresetReport>("remote_step_preset_delete", { request });
}

export function midiInventoryGet(): Promise<MidiInventoryStatus> {
  return invokeApi<MidiInventoryStatus>("midi_inventory_get");
}

export function localFsList(request: LocalFsListRequest = {}): Promise<LocalFsListResponse> {
  return invokeApi<LocalFsListResponse>("local_fs_list", { request });
}

export function localFsMkdir(request: LocalFsPathRequest): Promise<void> {
  return invokeApi<void>("local_fs_mkdir", { request });
}

export function localFsDelete(request: LocalFsDeleteRequest): Promise<void> {
  return invokeApi<void>("local_fs_delete", { request });
}

export function localFsRename(request: LocalFsRenameRequest): Promise<void> {
  return invokeApi<void>("local_fs_rename", { request });
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

export function urlOpen(url: string): Promise<void> {
  return invokeApi<void>("url_open", { url });
}

export function tabOrderSet(request: TabOrderSetRequest): Promise<TabOrderResponse> {
  return invokeApi<TabOrderResponse>("tab_order_set", { request });
}

export function appUpdateCheck(): Promise<AppUpdateStatus> {
  return invokeApi<AppUpdateStatus>("app_update_check");
}

export function appUpdateOpenLatest(): Promise<void> {
  return invokeApi<void>("app_update_open_latest");
}

export function uxRecordingsOpen(): Promise<void> {
  return invokeApi<void>("ux_recordings_open");
}

export function uxRecordingSessionRotate(instanceId: string): Promise<UxRecordingSessionInfo> {
  return invokeApi<UxRecordingSessionInfo>("ux_recording_session_rotate", { instanceId });
}
