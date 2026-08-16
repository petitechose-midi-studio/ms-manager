<script lang="ts">
  import type { BridgeInstanceStatus, Channel } from "$lib/api/types";
  import ChannelDropdown from "$lib/ui/ChannelDropdown.svelte";
  import ChoiceDropdown from "$lib/ui/ChoiceDropdown.svelte";
  import TagDropdown from "$lib/ui/TagDropdown.svelte";
  import CopyIcon from "$lib/ui/icons/CopyIcon.svelte";
  import FolderIcon from "$lib/ui/icons/FolderIcon.svelte";
  import InfoIcon from "$lib/ui/icons/InfoIcon.svelte";
  import RepositoryIcon from "$lib/ui/icons/RepositoryIcon.svelte";
  import {
    formatExactTimestamp,
    formatRelativeAge,
  } from "$lib/ui/instance/firmwarePresentation";

  export let instance: BridgeInstanceStatus;
  export let artifactConfigPath: string | null = null;
  export let disabled = false;
  export let loadingTags = false;
  export let activeTagValue = "";
  export let activeTagOptions: { value: string; label: string }[] = [];
  export let loadingBuildProfiles = false;
  export let buildProfileOptions: { value: string; label: string }[] = [];
  export let selectedBuildProfile = "";
  export let developmentSourcePath: string | null = null;
  export let developmentArtifactPath: string | null = null;
  export let artifactReady = false;
  export let artifactBuiltAtMs: number | null = null;
  export let sourceDirty = false;
  export let canCopyArtifact = false;
  export let building = false;
  export let profileError: string | null = null;
  export let needsDownload = false;
  export let canFlash = false;
  export let flashing = false;
  export let selectedFirmware = "-";
  export let errorMessage: string | null = null;
  export let errorActions: string[] = [];
  export let flashNotice: { instanceId: string | null; level: "warn"; message: string } | null = null;
  export let onEnvironmentChange: (source: "installed" | "workspace") => void;
  export let onTargetChange: (target: "standalone" | "bitwig") => void;
  export let onBuildProfileChange: (profile: string) => void;
  export let onBuild: () => void;
  export let onOpenSourceFolder: () => void;
  export let onOpenArtifactFolder: () => void;
  export let onCopyArtifact: () => void;
  export let onChannelChange: (channel: Channel) => void;
  export let onTagChange: (tag: string | null) => void;
  export let onDownload: () => void;
  export let onFlash: () => void;

  function compactPath(path: string | null): string {
    if (!path) return "No artifact";
    const separator = path.includes("\\") ? "\\" : "/";
    const parts = path.split(/[\\/]/).filter(Boolean);
    if (parts.length <= 4) return path;
    return `…${separator}${parts.slice(-4).join(separator)}`;
  }

  $: firmwareValid =
    (instance.target === "standalone" || instance.target === "bitwig") &&
    (instance.artifact_source === "workspace"
      ? !!selectedBuildProfile
      : !!(instance.artifact_location_path ?? artifactConfigPath ?? "").trim());
  $: artifactPath =
    instance.artifact_source === "workspace"
      ? developmentArtifactPath
      : (instance.artifact_location_path ?? artifactConfigPath);
  $: artifactPathLabel = compactPath(artifactPath);
  $: artifactBuildLabel = artifactBuiltAtMs
    ? `Built ${formatRelativeAge(artifactBuiltAtMs)}`
    : artifactReady
      ? "Build time unknown"
      : "Not built";
  $: artifactBuildTitle = artifactBuiltAtMs ? formatExactTimestamp(artifactBuiltAtMs) : artifactBuildLabel;
  $: artifactFolderReady = instance.artifact_source === "workspace" ? artifactReady : !!artifactPath;
  $: flashStatusLabel = needsDownload
    ? "Download required"
    : canFlash
      ? instance.artifact_source === "workspace" && !artifactReady
        ? "Build required"
        : "Ready to flash"
      : "Firmware selection incomplete";
</script>

<div class="workflow">
  <section class="environmentStep">
    <div class="stepHeading">
      <div class="stepIndex valid">1</div>
      <div>
        <div class="stepTitle">Environment</div>
        <div class="stepDetail">Firmware source</div>
      </div>
    </div>
    <ChoiceDropdown
      value={instance.artifact_source}
      placeholder="Select"
      options={[
        { value: "workspace", label: "Development" },
        { value: "installed", label: "Distribution" },
      ]}
      {disabled}
      onChange={(value) => onEnvironmentChange(value as "installed" | "workspace")}
    />
  </section>

  <section class="firmwareStep">
    <div class="stepHeading">
      <div class="stepIndex" class:valid={firmwareValid}>2</div>
      <div>
        <div class="stepTitle">Firmware</div>
        <div class="stepDetail">
          {instance.artifact_source === "workspace" ? "Build profile and artifact" : "Target and release"}
        </div>
      </div>
    </div>

    <div class="controls">
      <ChoiceDropdown
        label="Target"
        value={instance.target}
        placeholder="Select"
        options={[
          { value: "standalone", label: "Standalone", icon: "controller" },
          { value: "bitwig", label: "Bitwig", icon: "bitwig" },
        ]}
        {disabled}
        onChange={(value) => onTargetChange(value as "standalone" | "bitwig")}
      />

      {#if instance.artifact_source === "workspace"}
        <ChoiceDropdown
          label="Profile"
          value={selectedBuildProfile}
          placeholder={loadingBuildProfiles ? "Loading profiles..." : "Select profile"}
          options={buildProfileOptions}
          disabled={disabled || loadingBuildProfiles || !buildProfileOptions.length}
          onChange={onBuildProfileChange}
        />
        <button
          class="textButton controlBottom"
          type="button"
          disabled={disabled || building || !selectedBuildProfile}
          onclick={onBuild}
        >
          {building ? "Building..." : "Build"}
        </button>
        <div class="sourceActions controlBottom">
          <button
            class="iconButton"
            type="button"
            title="Open development source repository"
            aria-label="Open development source repository"
            disabled={!developmentSourcePath}
            onclick={onOpenSourceFolder}
          >
            <RepositoryIcon size={16} />
          </button>
          {#if sourceDirty}
            <span class="dirtyBadge" title="The development repository contains uncommitted changes">
              <InfoIcon size={13} />
              <span>Uncommitted changes</span>
            </span>
          {/if}
        </div>
      {:else}
        <ChannelDropdown
          value={instance.installed_channel ?? "stable"}
          {disabled}
          onChange={onChannelChange}
        />
        <TagDropdown
          value={activeTagValue}
          options={activeTagOptions}
          disabled={loadingTags || disabled}
          onChange={(value) => onTagChange(value === "" ? null : value)}
        />
        <button class="textButton controlBottom" type="button" disabled={disabled || !needsDownload} onclick={onDownload}>
          {needsDownload ? "Download" : "Downloaded"}
        </button>
      {/if}
    </div>

    <div class="artifactRow" data-ready={artifactFolderReady}>
      <button
        class="iconButton"
        type="button"
        title="Open artifact folder"
        aria-label="Open artifact folder"
        disabled={!artifactFolderReady}
        onclick={onOpenArtifactFolder}
      >
        <FolderIcon size={16} />
      </button>
      <span class="artifactPath" title={artifactPath ?? "No artifact"}>{artifactPathLabel}</span>
      {#if instance.artifact_source === "workspace" && canCopyArtifact}
        <button
          class="inlineIconButton"
          type="button"
          title="Copy firmware file"
          aria-label="Copy firmware file"
          disabled={!artifactReady || !developmentArtifactPath}
          onclick={onCopyArtifact}
        >
          <CopyIcon size={14} />
        </button>
      {/if}
      {#if instance.artifact_source === "workspace"}
        <span class="artifactAge" title={artifactBuildTitle}>{artifactBuildLabel}</span>
      {/if}
    </div>

    {#if profileError}
      <div class="message error">{profileError}</div>
    {:else if instance.artifact_message && instance.artifact_source === "installed"}
      <div class="message neutral">{instance.artifact_message}</div>
    {/if}

    <div class="flashRow">
      <div class="flashSummary" data-ready={canFlash && !needsDownload}>
        <span class="statusDot" aria-hidden="true"></span>
        <div>
          <div class="flashStatus">{flashStatusLabel}</div>
          <div class="selectedFirmware">{selectedFirmware}</div>
        </div>
      </div>
      <button class="primaryButton" type="button" disabled={disabled || !canFlash} onclick={onFlash}>
        {#if flashing}
          {instance.artifact_source === "workspace" && !artifactReady ? "Building / Flashing..." : "Flashing..."}
        {:else if instance.artifact_source === "workspace" && !artifactReady}
          Build &amp; Flash
        {:else}
          Flash Firmware
        {/if}
      </button>
    </div>

    {#if errorMessage}
      <div class="message error">
        <div>{errorMessage}</div>
        {#if errorActions.length}
          <div class="hintTitle">Try this</div>
          <ul class="hintList">
            {#each errorActions as action}
              <li>{action}</li>
            {/each}
          </ul>
        {/if}
      </div>
    {/if}

    {#if flashNotice}
      <div class="message warning">{flashNotice.message}</div>
    {/if}
  </section>

  {#if instance.message && !instance.running}
    <div class="message neutral">{instance.message}</div>
  {/if}
</div>

<style>
  .workflow {
    display: grid;
    gap: var(--space-4);
  }

  .environmentStep,
  .firmwareStep {
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: color-mix(in srgb, var(--panel) 72%, transparent);
  }

  .environmentStep {
    min-height: 64px;
    padding: var(--space-3) var(--space-4);
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .firmwareStep {
    padding: var(--space-4);
    display: grid;
    gap: var(--space-4);
  }

  .stepHeading {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    min-width: 0;
  }

  .stepIndex {
    width: 24px;
    height: 24px;
    border-radius: 999px;
    border: 1px solid var(--border-strong);
    color: var(--muted);
    display: grid;
    place-items: center;
    font: 800 11px/14px var(--font-sans);
    flex: 0 0 auto;
  }

  .stepIndex.valid {
    border-color: var(--ok);
    color: var(--ok);
  }

  .stepTitle {
    color: var(--fg);
    font: 700 14px/18px var(--font-sans);
  }

  .stepDetail {
    color: var(--muted);
    font: 400 11px/15px var(--font-sans);
  }

  .controls {
    display: flex;
    align-items: flex-end;
    gap: var(--space-3);
    flex-wrap: wrap;
  }

  .controlBottom {
    align-self: flex-end;
  }

  .sourceActions {
    min-height: var(--control-height);
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }

  .textButton,
  .primaryButton {
    appearance: none;
    min-height: var(--control-height);
    padding: 7px var(--control-padding-x);
    border-radius: var(--control-radius);
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    font: 800 11px/14px var(--font-sans);
    text-transform: uppercase;
    letter-spacing: 0.06em;
    transition: 120ms ease;
    transition-property: color, border-color, background-color, transform;
  }

  .textButton:hover:not(:disabled) {
    color: var(--fg);
    border-color: var(--border-strong);
    background: color-mix(in srgb, var(--fg) 5%, transparent);
  }

  .primaryButton {
    background: var(--value);
    color: var(--bg);
    border-color: var(--value);
    white-space: nowrap;
  }

  .primaryButton:hover:not(:disabled) {
    background: color-mix(in srgb, var(--value) 88%, white);
  }

  .textButton:active:not(:disabled),
  .primaryButton:active:not(:disabled),
  .iconButton:active:not(:disabled),
  .inlineIconButton:active:not(:disabled) {
    transform: translateY(1px);
  }

  .textButton:focus-visible,
  .primaryButton:focus-visible,
  .iconButton:focus-visible,
  .inlineIconButton:focus-visible {
    outline: 2px solid var(--value);
    outline-offset: 2px;
  }

  .textButton:disabled,
  .primaryButton:disabled,
  .iconButton:disabled,
  .inlineIconButton:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .iconButton,
  .inlineIconButton {
    appearance: none;
    padding: 0;
    border-radius: var(--control-radius);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    display: grid;
    place-items: center;
    transition: 120ms ease;
    transition-property: color, border-color, background-color, transform;
  }

  .iconButton {
    width: var(--control-height);
    height: var(--control-height);
    border: 1px solid var(--border);
  }

  .inlineIconButton {
    width: 28px;
    height: 28px;
    border: 1px solid transparent;
    flex: 0 0 auto;
  }

  .iconButton:hover:not(:disabled),
  .inlineIconButton:hover:not(:disabled) {
    color: var(--fg);
    border-color: var(--border-strong);
    background: color-mix(in srgb, var(--fg) 5%, transparent);
  }

  .dirtyBadge {
    min-height: 28px;
    padding: 4px 8px;
    border: 1px solid color-mix(in srgb, var(--warn) 46%, var(--border));
    border-radius: 999px;
    color: var(--warn);
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font: 700 10px/13px var(--font-sans);
    letter-spacing: 0.03em;
    white-space: nowrap;
  }

  .artifactRow {
    min-height: 48px;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: var(--control-radius);
    background: color-mix(in srgb, var(--bg) 38%, transparent);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .artifactRow[data-ready="true"] {
    border-color: color-mix(in srgb, var(--ok) 28%, var(--border));
  }

  .artifactPath {
    min-width: 120px;
    max-width: 520px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: color-mix(in srgb, var(--fg) 82%, var(--muted));
    font: 400 12px/16px var(--font-mono);
  }

  .artifactAge {
    color: var(--muted);
    font: 500 11px/15px var(--font-sans);
    white-space: nowrap;
  }

  .flashRow {
    display: flex;
    align-items: center;
    gap: var(--space-4);
    flex-wrap: wrap;
  }

  .flashSummary {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .statusDot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--warn);
    flex: 0 0 auto;
  }

  .flashSummary[data-ready="true"] .statusDot {
    background: var(--ok);
  }

  .flashStatus {
    color: var(--fg);
    font: 700 12px/16px var(--font-sans);
  }

  .selectedFirmware {
    max-width: 440px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--muted);
    font: 400 11px/15px var(--font-mono);
  }

  .message {
    border-radius: var(--control-radius);
    padding: var(--space-3) var(--space-4);
    font: 400 12px/16px var(--font-sans);
  }

  .message.error {
    color: var(--err);
    border: 1px solid var(--err);
  }

  .message.warning {
    color: var(--warn);
    border: 1px solid var(--warn);
  }

  .message.neutral {
    color: var(--muted);
    border: 1px solid var(--border);
  }

  .hintTitle {
    margin-top: var(--space-2);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 11px;
    line-height: 14px;
  }

  .hintList {
    margin: var(--space-2) 0 0;
    padding-left: 18px;
    display: grid;
    gap: 4px;
  }

  @media (max-width: 760px) {
    .environmentStep {
      align-items: stretch;
      flex-direction: column;
    }

    .controls :global(.wrap) {
      width: 100%;
    }

    .sourceActions {
      flex-wrap: wrap;
    }

    .artifactPath {
      flex: 1 1 140px;
    }

    .flashRow {
      align-items: stretch;
      flex-direction: column;
    }

    .primaryButton {
      width: fit-content;
    }
  }
</style>
