<script lang="ts">
  import { onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { LogicalSize } from "@tauri-apps/api/dpi";
  import { bridgeLogOpen, pathOpen } from "$lib/api/client";

  import type { ActivityEntry, ActivityFilter } from "$lib/state/activity";
  import { createActivityLog, matchesActivityFilter } from "$lib/state/activity";
  import { createDashboard } from "$lib/state/dashboard";

  import HeaderBar from "$lib/ui/HeaderBar.svelte";
  import RelocatePayloadModal from "$lib/ui/RelocatePayloadModal.svelte";
  import ActivityDrawer from "$lib/ui/ActivityDrawer.svelte";
  import InstanceTabs from "$lib/ui/instance/InstanceTabs.svelte";
  import type { ControllerTabItem } from "$lib/ui/instance/InstanceTabs.svelte";
  import InstanceHeader from "$lib/ui/instance/InstanceHeader.svelte";
  import InstanceConfigurationCard from "$lib/ui/instance/InstanceConfigurationCard.svelte";
  import InstanceActionsCard from "$lib/ui/instance/InstanceActionsCard.svelte";
  import UnboundControllerView from "$lib/ui/instance/UnboundControllerView.svelte";

  const activity = createActivityLog(1000);
  const dash = createDashboard(activity);
  const dashState = dash.state;
  const activityEntries = activity.entries;

  let cleanup: (() => void) | null = null;
  let activeTabKey: string | null = null;
  let lastTagsChannel: "stable" | "beta" | "nightly" | null = null;
  let nameDraft = "";
  let nameDraftKey: string | null = null;
  let renamingInstanceId: string | null = null;

  onMount(() => {
    void (async () => {
      try {
        const win = getCurrentWindow();
        await win.setMinSize(new LogicalSize(720, 520));
      } catch {
        // ignore
      }
    })();

    void (async () => {
      cleanup = await dash.start();
    })();

    return () => {
      cleanup?.();
    };
  });

  function fmtTargetLabel(target: string): string {
    if (target === "standalone") return "Standalone";
    if (target === "bitwig") return "Bitwig";
    return target;
  }

  function fallbackInstanceName(instance: {
    configured_serial: string;
    target: string;
  }): string {
    return `${fmtTargetLabel(instance.target)} ${instance.configured_serial}`;
  }

  function unboundName(target: { product?: string | null; serial_number?: string | null }): string {
    return target.product?.trim() || `Controller ${target.serial_number ?? ""}`.trim();
  }

  $: serialTargets = ($dashState.device.targets ?? []).filter(
    (target) => target.kind === "serial" && !!target.serial_number
  );
  $: boundSerials = new Set($dashState.bridge.instances.map((instance) => instance.configured_serial));
  $: unboundSerialTargets = serialTargets.filter(
    (target) => !boundSerials.has(target.serial_number ?? "")
  );
  $: controllerTabs = [
    ...$dashState.bridge.instances.map(
      (instance) =>
        ({
          key: `instance:${instance.instance_id}`,
          kind: "instance",
          serial: instance.configured_serial,
          label: instance.display_name?.trim() || fallbackInstanceName(instance),
          instance,
        }) satisfies ControllerTabItem
    ),
    ...unboundSerialTargets.map(
      (target) =>
        ({
          key: `unbound:${target.serial_number}`,
          kind: "unbound",
          serial: target.serial_number ?? "",
          label: unboundName(target),
          subtitle: target.serial_number ?? "",
          target,
        }) satisfies ControllerTabItem
    ),
  ];
  $: if (controllerTabs.length > 0 && !controllerTabs.some((tab) => tab.key === activeTabKey)) {
    activeTabKey = controllerTabs[0].key;
  }
  $: if (controllerTabs.length === 0) {
    activeTabKey = null;
  }
  $: activeTab = controllerTabs.find((tab) => tab.key === activeTabKey) ?? null;
  $: activeInstance = activeTab?.kind === "instance" ? activeTab.instance : null;
  $: activeUnboundTarget = activeTab?.kind === "unbound" ? activeTab.target : null;
  $: dash.setActiveBridgeInstance(activeInstance?.instance_id ?? null);
  $: activeInstalledChannel =
    activeInstance?.artifact_source === "installed"
      ? (activeInstance.installed_channel ?? "stable")
      : null;
  $: if (activeInstalledChannel && activeInstalledChannel !== lastTagsChannel) {
    lastTagsChannel = activeInstalledChannel;
    void dash.loadTagsForChannel(activeInstalledChannel);
  }
  $: if (!activeInstalledChannel) {
    lastTagsChannel = null;
  }
  $: activeTagValue =
    activeInstance?.artifact_source === "installed"
      ? (activeInstance.installed_pinned_tag ?? "")
      : "";
  $: activeTagOptions = [
    { value: "", label: "latest" },
    ...$dashState.tags.map((tag) => ({ value: tag, label: tag })),
  ];
  $: if (activeInstance) {
    const nextDraft = activeInstance.display_name?.trim() || "";
    if (nameDraftKey !== activeInstance.instance_id) {
      nameDraftKey = activeInstance.instance_id;
      nameDraft = nextDraft;
    }
  } else {
    nameDraftKey = null;
    nameDraft = "";
  }

  $: appUpdateAvailable = $dashState.appUpdate?.available ?? false;
  $: appUpdateLabel = $dashState.appUpdate?.update
    ? `ms-manager ${$dashState.appUpdate.update.version}`
    : null;
  $: activeBusy = $dashState.bridgeMutating || $dashState.installing || $dashState.flashing;

  async function copyActivity(entries: ActivityEntry[], filter: ActivityFilter) {
    const visible = entries.filter((e) => matchesActivityFilter(e, filter));
    const text = activity.toText(visible);
    try {
      await navigator.clipboard.writeText(text);
    } catch (e) {
      activity.add("warn", "ui", "copy failed", e);
    }
  }

  async function openSourcePath(path?: string | null) {
    if (!path) return;
    try {
      await pathOpen(path);
    } catch (e) {
      const err = e as { code?: string; message?: string };
      const msg = typeof err?.message === "string" ? err.message : String(e);
      activity.add("warn", "ui", `open folder failed: ${msg}`, e);
    }
  }

  async function openBridgeLogs() {
    try {
      await bridgeLogOpen();
    } catch (e) {
      const err = e as { code?: string; message?: string };
      const msg = typeof err?.message === "string" ? err.message : String(e);
      activity.add("warn", "bridge", `open bridge logs failed: ${msg}`, e);
    }
  }

  async function createInstanceForActiveTarget() {
    if (!activeUnboundTarget) return;
    const binding = await dash.bindHardwareBridge(activeUnboundTarget);
    if (binding) {
      activeTabKey = `instance:${binding.instance_id}`;
    }
  }

  async function saveDisplayName() {
    if (!activeInstance) return;
    const nextValue = nameDraft.trim();
    const currentValue = activeInstance.display_name?.trim() || "";
    if (nextValue !== currentValue) {
      await dash.setBridgeDisplayName(activeInstance.instance_id, nextValue || null);
    }
    renamingInstanceId = null;
  }

  function beginRename() {
    if (!activeInstance) return;
    renamingInstanceId = activeInstance.instance_id;
    nameDraft = activeInstance.display_name?.trim() || "";
  }

  async function setActiveInstanceTarget(target: "standalone" | "bitwig") {
    if (!activeInstance || target === activeInstance.target) return;
    await dash.setBridgeTarget(activeInstance.instance_id, target);
  }

  async function setActiveInstanceSource(source: "installed" | "workspace") {
    if (!activeInstance || source === activeInstance.artifact_source) return;
    await dash.setBridgeArtifactSource(activeInstance.instance_id, source);
  }

  async function setActiveInstanceChannel(channel: "stable" | "beta" | "nightly") {
    if (!activeInstance) return;
    await dash.setBridgeInstalledRelease(activeInstance.instance_id, channel, null);
  }

  async function setActiveInstanceTag(tag: string | null) {
    if (!activeInstance || activeInstance.artifact_source !== "installed") return;
    const channel = activeInstance.installed_channel ?? "stable";
    await dash.setBridgeInstalledRelease(activeInstance.instance_id, channel, tag);
  }

  async function onTitleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter") {
      event.preventDefault();
      await saveDisplayName();
      return;
    }
    if (event.key === "Escape") {
      event.preventDefault();
      renamingInstanceId = null;
      nameDraft = activeInstance?.display_name?.trim() || "";
    }
  }

  $: activeTargetProfile = activeInstance?.target === "standalone" ? "default" : "bitwig";
  $: installedSelectionReady =
    !!activeInstance &&
    activeInstance.artifact_source === "installed" &&
    (
      activeInstance.installed_pinned_tag
        ? activeInstance.artifacts_ready
        : !!$dashState.installed &&
          $dashState.installed.channel === (activeInstance.installed_channel ?? "stable") &&
          $dashState.installed.profile === activeTargetProfile &&
          activeInstance.artifacts_ready
    );
  $: canFlashActiveInstance =
    !!activeInstance &&
    (
      activeInstance.artifact_source === "workspace"
        ? activeInstance.artifacts_ready
        : installedSelectionReady
    );
  $: needsDownloadActiveInstance =
    !!activeInstance &&
    activeInstance.artifact_source === "installed" &&
    !installedSelectionReady;
  $: activeVersionLabel =
    !activeInstance
      ? "-"
      : activeInstance.artifact_source === "workspace"
        ? "workspace"
        : activeInstance.installed_pinned_tag?.trim()
          ? activeInstance.installed_pinned_tag
          : installedSelectionReady && $dashState.installed
            ? $dashState.installed.tag
            : `latest (${activeInstance.installed_channel ?? "stable"})`;
  $: activeFlashLabel = activeInstance
    ? `${fmtTargetLabel(activeInstance.target)} / ${activeVersionLabel}`
    : "-";
  $: activeErrorMessage = $dashState.error?.message ?? null;
</script>

<div class="page">
  <HeaderBar
    device={$dashState.device}
    platform={$dashState.platform}
    appUpdateAvailable={appUpdateAvailable}
    appUpdateLabel={appUpdateLabel}
  />

  <section class="panel mainPanel">
    <InstanceTabs tabs={controllerTabs} activeKey={activeTabKey} onSelect={(key) => (activeTabKey = key)} />

    <div class="panelBody">
      {#if $dashState.booting}
        <div class="bootState">
          <div class="bootTitle">Starting ms-manager…</div>
          <div class="bootGrid">
            <div class="bootCard"></div>
            <div class="bootCard"></div>
          </div>
        </div>
      {:else if activeInstance}
        <InstanceHeader
          instance={activeInstance}
          fallbackName={fallbackInstanceName(activeInstance)}
          renaming={renamingInstanceId === activeInstance.instance_id}
          {nameDraft}
          busy={activeBusy}
          onNameInput={(value) => (nameDraft = value)}
          {onTitleKeydown}
          onSaveName={saveDisplayName}
          onBeginRename={beginRename}
          onOpenLogs={openBridgeLogs}
          onToggleEnabled={() => dash.setBridgeEnabled(activeInstance.instance_id, !activeInstance.enabled)}
          onRemove={() => dash.removeBridge(activeInstance.instance_id)}
        />

        <div class="stack">
          <InstanceConfigurationCard
            instance={activeInstance}
            artifactConfigPath={$dashState.artifactConfigPath}
            disabled={activeBusy}
            loadingTags={$dashState.loadingTags}
            {activeTagValue}
            {activeTagOptions}
            needsDownload={needsDownloadActiveInstance}
            onSourceChange={setActiveInstanceSource}
            onTargetChange={setActiveInstanceTarget}
            onOpenFolder={() => openSourcePath(activeInstance.artifact_location_path)}
            onChannelChange={setActiveInstanceChannel}
            onTagChange={setActiveInstanceTag}
            onDownload={() => dash.installForBridgeInstance(activeInstance.instance_id)}
          />

          <InstanceActionsCard
            selectedFirmware={activeFlashLabel}
            needsDownload={needsDownloadActiveInstance}
            canFlash={canFlashActiveInstance}
            busy={activeBusy}
            flashing={$dashState.flashing && $dashState.flashingInstanceId === activeInstance.instance_id}
            errorMessage={activeErrorMessage}
            onFlash={() => dash.flashInstance(activeInstance.instance_id)}
          />
        </div>
      {:else if activeUnboundTarget}
        <UnboundControllerView
          target={activeUnboundTarget}
          busy={activeBusy}
          onCreate={createInstanceForActiveTarget}
        />
      {:else}
        <div class="emptyState">
          <div class="emptyTitle">No controller tab available</div>
          <div class="muted">Connect a controller to start configuring an instance.</div>
        </div>
      {/if}

      {#if $dashState.now}
        <div class="muted">{$dashState.now}</div>
      {/if}
    </div>
  </section>

  <ActivityDrawer
    open={$dashState.activityOpen}
    filter={$dashState.activityFilter}
    entries={$activityEntries}
    onToggle={() => dash.toggleActivity()}
    onFilter={(f) => dash.setActivityFilter(f)}
    onCopy={() => copyActivity($activityEntries, $dashState.activityFilter)}
    onClear={() => activity.clear()}
  />

  <RelocatePayloadModal
    open={$dashState.relocateModal.open}
    currentRoot={$dashState.payloadRoot}
    nextRoot={$dashState.relocateModal.nextRoot}
    relocating={$dashState.relocating}
    ack={$dashState.relocateModal.ack}
    onRoot={(v) => dash.setRelocateRoot(v)}
    onBrowse={() => dash.browseRelocateRoot()}
    onAck={(v) => dash.setRelocateAck(v)}
    onCancel={() => dash.cancelRelocateModal()}
    onConfirm={() => dash.confirmRelocateModal()}
  />
</div>

<style>
  .page {
    height: 100vh;
    padding: 14px;
    display: grid;
    grid-template-rows: auto 1fr auto;
    gap: 12px;
  }

  .panel {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }

  .mainPanel {
    min-height: 0;
  }

  .panelBody {
    padding: 14px;
    overflow: auto;
    display: grid;
    gap: 14px;
    min-height: 0;
  }

  .stack {
    display: grid;
    gap: 12px;
  }

  .muted {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
  }

  .emptyState {
    display: grid;
    gap: 6px;
    padding: 10px 0;
  }

  .emptyTitle {
    color: var(--fg);
    font-family: var(--font-sans);
    font-weight: 700;
    font-size: 18px;
    line-height: 22px;
  }

  .bootState {
    display: grid;
    gap: 14px;
  }

  .bootTitle {
    color: var(--muted);
    font-family: var(--font-sans);
    font-weight: 600;
    font-size: 13px;
    line-height: 18px;
  }

  .bootGrid {
    display: grid;
    gap: 12px;
  }

  .bootCard {
    min-height: 136px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background:
      linear-gradient(
        90deg,
        rgba(255, 255, 255, 0.02) 0%,
        rgba(255, 255, 255, 0.06) 50%,
        rgba(255, 255, 255, 0.02) 100%
      );
    background-size: 220% 100%;
    animation: bootShimmer 1.3s linear infinite;
  }

  @keyframes bootShimmer {
    from {
      background-position: 200% 0;
    }
    to {
      background-position: -20% 0;
    }
  }

  @media (max-width: 980px) {
    .page {
      padding: 12px;
      gap: 10px;
    }
  }
</style>
