import { get } from "svelte/store";

import type { ActivityFilter } from "$lib/state/activity";
import {
  bridgeInstanceArtifactSourceSet,
  bridgeInstanceBind,
  bridgeInstanceEnableSet,
  bridgeInstanceInstalledReleaseSet,
  bridgeInstanceNameSet,
  bridgeInstanceRemove,
  bridgeInstanceTargetSet,
  flashBridgeInstance,
  installBridgeInstance,
  payloadRootRelocate,
} from "$lib/api/client";
import type { Channel } from "$lib/api/types";
import {
  applyStatus,
  clearApiError,
  setApiError,
  type DashboardArtifactSource,
  type DashboardBindTarget,
  type DashboardBridgeMode,
  type DashboardFirmwareTarget,
  type DashboardMutationDeps,
} from "$lib/state/dashboard_shared";

export function createDashboardMutationController({
  state,
  activity,
  refreshStatus,
}: DashboardMutationDeps) {
  function openRelocateModal() {
    const snapshot = get(state);
    state.update((current) => ({
      ...current,
      relocateModal: { open: true, nextRoot: snapshot.payloadRoot ?? "", ack: false },
    }));
  }

  function cancelRelocateModal() {
    state.update((current) => ({
      ...current,
      relocateModal: { open: false, nextRoot: "", ack: false },
    }));
  }

  function setRelocateRoot(nextRoot: string) {
    state.update((current) => ({
      ...current,
      relocateModal: { ...current.relocateModal, nextRoot },
    }));
  }

  function setRelocateAck(ack: boolean) {
    state.update((current) => ({
      ...current,
      relocateModal: { ...current.relocateModal, ack },
    }));
  }

  async function browseRelocateRoot() {
    const base = get(state).payloadRoot ?? undefined;
    try {
      const dialog = await import("@tauri-apps/plugin-dialog");
      const selected = await dialog.open({
        title: "Select installation folder",
        directory: true,
        multiple: false,
        defaultPath: base,
      });
      if (typeof selected === "string" && selected.trim()) {
        state.update((current) => ({
          ...current,
          relocateModal: { ...current.relocateModal, nextRoot: selected },
        }));
      }
    } catch (error) {
      activity.add("warn", "ui", "folder picker failed", error);
    }
  }

  async function confirmRelocateModal() {
    const snapshot = get(state);
    const nextRoot = snapshot.relocateModal.nextRoot.trim();
    if (!snapshot.relocateModal.ack || !nextRoot) return;

    state.update((current) => ({ ...current, relocating: true }));
    clearApiError(state);
    activity.add("info", "fs", `relocate payload root -> ${nextRoot}`);
    try {
      const status = await payloadRootRelocate(nextRoot);
      applyStatus(state, status);
      activity.add("ok", "fs", `payload root: ${status.payload_root}`);
      cancelRelocateModal();
    } catch (error) {
      setApiError(state, activity, error);
    } finally {
      state.update((current) => ({ ...current, relocating: false }));
    }
  }

  async function flashInstance(instanceId: string) {
    state.update((current) => ({ ...current, flashing: true, flashingInstanceId: instanceId }));
    clearApiError(state);
    activity.add("info", "flash", `flash start instance=${instanceId}`);
    try {
      const result = await flashBridgeInstance(instanceId);
      activity.add("ok", "flash", `flash done profile=${result.profile}`);
      await refreshStatus();
    } catch (error) {
      setApiError(state, activity, error);
    } finally {
      state.update((current) => ({ ...current, flashing: false, flashingInstanceId: null }));
    }
  }

  async function bindHardwareBridge(target: DashboardBindTarget, mode: DashboardBridgeMode = "hardware") {
    const controllerSerial = target.serial_number?.trim();
    if (!controllerSerial) return null;

    state.update((current) => ({ ...current, bridgeMutating: true }));
    clearApiError(state);
    activity.add("info", "bridge", `bind bridge serial=${controllerSerial}`);
    try {
      const result = await bridgeInstanceBind({
        app: "bitwig",
        mode,
        controller_serial: controllerSerial,
        controller_vid: target.vid,
        controller_pid: target.pid,
      });
      await refreshStatus();
      activity.add("ok", "bridge", `bridge bound serial=${controllerSerial}`);
      return result.binding;
    } catch (error) {
      setApiError(state, activity, error);
    } finally {
      state.update((current) => ({ ...current, bridgeMutating: false }));
    }
    return null;
  }

  async function removeBridge(instanceId: string) {
    state.update((current) => ({ ...current, bridgeMutating: true }));
    clearApiError(state);
    activity.add("info", "bridge", `remove bridge instance=${instanceId}`);
    try {
      await bridgeInstanceRemove(instanceId);
      await refreshStatus();
    } catch (error) {
      setApiError(state, activity, error);
    } finally {
      state.update((current) => ({ ...current, bridgeMutating: false }));
    }
  }

  async function setBridgeEnabled(instanceId: string, enabled: boolean) {
    state.update((current) => ({ ...current, bridgeMutating: true }));
    clearApiError(state);
    try {
      await bridgeInstanceEnableSet(instanceId, enabled);
      await refreshStatus();
    } catch (error) {
      setApiError(state, activity, error);
    } finally {
      state.update((current) => ({ ...current, bridgeMutating: false }));
    }
  }

  async function installForBridgeInstance(instanceId: string) {
    state.update((current) => ({ ...current, installing: true }));
    clearApiError(state);
    try {
      await installBridgeInstance(instanceId);
      await refreshStatus();
    } catch (error) {
      setApiError(state, activity, error);
    } finally {
      state.update((current) => ({ ...current, installing: false }));
    }
  }

  async function setBridgeTarget(instanceId: string, target: DashboardFirmwareTarget) {
    state.update((current) => ({ ...current, bridgeMutating: true }));
    clearApiError(state);
    try {
      await bridgeInstanceTargetSet({ instance_id: instanceId, target });
      await refreshStatus();
    } catch (error) {
      setApiError(state, activity, error);
    } finally {
      state.update((current) => ({ ...current, bridgeMutating: false }));
    }
  }

  async function setBridgeArtifactSource(instanceId: string, artifactSource: DashboardArtifactSource) {
    state.update((current) => ({ ...current, bridgeMutating: true }));
    clearApiError(state);
    try {
      await bridgeInstanceArtifactSourceSet({
        instance_id: instanceId,
        artifact_source: artifactSource,
      });
      await refreshStatus();
    } catch (error) {
      setApiError(state, activity, error);
    } finally {
      state.update((current) => ({ ...current, bridgeMutating: false }));
    }
  }

  async function setBridgeInstalledRelease(
    instanceId: string,
    channel: Channel,
    pinnedTag: string | null,
  ) {
    state.update((current) => ({ ...current, bridgeMutating: true }));
    clearApiError(state);
    try {
      await bridgeInstanceInstalledReleaseSet({
        instance_id: instanceId,
        channel,
        pinned_tag: pinnedTag,
      });
      await refreshStatus();
    } catch (error) {
      setApiError(state, activity, error);
    } finally {
      state.update((current) => ({ ...current, bridgeMutating: false }));
    }
  }

  async function setBridgeDisplayName(instanceId: string, displayName: string | null) {
    state.update((current) => ({ ...current, bridgeMutating: true }));
    clearApiError(state);
    try {
      await bridgeInstanceNameSet({
        instance_id: instanceId,
        display_name: displayName,
      });
      await refreshStatus();
    } catch (error) {
      setApiError(state, activity, error);
    } finally {
      state.update((current) => ({ ...current, bridgeMutating: false }));
    }
  }

  function setActiveBridgeInstance(instanceId: string | null) {
    activity.retain((entry) => {
      if (entry.scope !== "bridge") return true;
      if (!instanceId) return false;
      const details = entry.details as { instance_id?: string | null } | undefined;
      return details?.instance_id === instanceId;
    });

    state.update((current) => {
      if (current.activeBridgeInstanceId === instanceId) {
        return current;
      }
      return { ...current, activeBridgeInstanceId: instanceId };
    });
  }

  function toggleActivity() {
    state.update((current) => ({ ...current, activityOpen: !current.activityOpen }));
  }

  function setActivityFilter(filter: ActivityFilter) {
    state.update((current) => ({ ...current, activityFilter: filter }));
  }

  return {
    openRelocateModal,
    cancelRelocateModal,
    setRelocateRoot,
    setRelocateAck,
    browseRelocateRoot,
    confirmRelocateModal,
    flashInstance,
    bindHardwareBridge,
    removeBridge,
    setBridgeEnabled,
    installForBridgeInstance,
    setBridgeTarget,
    setBridgeArtifactSource,
    setBridgeInstalledRelease,
    setBridgeDisplayName,
    setActiveBridgeInstance,
    toggleActivity,
    setActivityFilter,
  };
}
