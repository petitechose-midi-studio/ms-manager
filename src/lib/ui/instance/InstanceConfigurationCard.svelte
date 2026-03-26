<script lang="ts">
  import type { BridgeInstanceStatus, Channel } from "$lib/api/types";
  import ChoiceDropdown from "$lib/ui/ChoiceDropdown.svelte";
  import ChannelDropdown from "$lib/ui/ChannelDropdown.svelte";
  import TagDropdown from "$lib/ui/TagDropdown.svelte";
  import FolderIcon from "$lib/ui/icons/FolderIcon.svelte";

  export let instance: BridgeInstanceStatus;
  export let artifactConfigPath: string | null = null;
  export let disabled = false;
  export let loadingTags = false;
  export let activeTagValue = "";
  export let activeTagOptions: { value: string; label: string }[] = [];
  export let needsDownload = false;
  export let onSourceChange: (source: "installed" | "workspace") => void;
  export let onTargetChange: (target: "standalone" | "bitwig") => void;
  export let onOpenFolder: () => void;
  export let onChannelChange: (channel: Channel) => void;
  export let onTagChange: (tag: string | null) => void;
  export let onDownload: () => void;

  function fmtLastFlashed(ms: number): string {
    const age = Math.max(0, Date.now() - ms);
    const m = Math.floor(age / 60000);
    if (m < 1) return "just now";
    if (m < 60) return `${m}m ago`;
    const h = Math.floor(m / 60);
    if (h < 24) return `${h}h ago`;
    const d = Math.floor(h / 24);
    return `${d}d ago`;
  }
</script>

<div class="card">
  <div class="cardTitle">Configuration</div>

  <div class="field">
    <div class="row">
      <ChoiceDropdown
        label="Source"
        value={instance.artifact_source}
        options={[
          { value: "installed", label: "Installed" },
          { value: "workspace", label: "Workspace" },
        ]}
        {disabled}
        onChange={(value) => onSourceChange(value as "installed" | "workspace")}
      />
    </div>
  </div>

  <div class="field">
    <div class="row">
      <ChoiceDropdown
        label="Target"
        value={instance.target}
        options={[
          { value: "standalone", label: "Standalone", icon: "controller" },
          { value: "bitwig", label: "Bitwig", icon: "bitwig" },
        ]}
        {disabled}
        onChange={(value) => onTargetChange(value as "standalone" | "bitwig")}
      />
      <button
        class="mini folderAction"
        type="button"
        disabled={!instance.artifact_location_path}
        onclick={onOpenFolder}
      >
        <span class="btnIcon" aria-hidden="true"><FolderIcon size={13} /></span>
        <span>Open Folder</span>
      </button>
    </div>
    {#if instance.artifact_source === "workspace"}
      <div class="subtlePath">{instance.artifact_location_path ?? artifactConfigPath ?? "-"}</div>
    {/if}
  </div>

  {#if instance.artifact_source === "installed"}
    <div class="field">
      <div class="label">Release</div>
      <div class="row">
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
        <button class="mini" type="button" disabled={disabled || !needsDownload} onclick={onDownload}>
          Download
        </button>
      </div>
    </div>
  {/if}

  {#if instance.artifact_message}
    <div class="muted">{instance.artifact_message}</div>
  {/if}

  <div class="field">
    <div class="label">Last Flash</div>
    <div class="value">
      {#if instance.last_flashed}
        {instance.last_flashed.channel}:{instance.last_flashed.tag}:{instance.last_flashed.profile}
        <span class="muted">({fmtLastFlashed(instance.last_flashed.flashed_at_ms)})</span>
      {:else}
        <span class="muted">unknown</span>
      {/if}
    </div>
  </div>

  {#if instance.message && !instance.running}
    <div class="muted">{instance.message}</div>
  {/if}
</div>

<style>
  .card {
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    display: grid;
    gap: 12px;
    background: rgba(0, 0, 0, 0.03);
  }

  :global(:root[data-theme="light"]) .card {
    background: rgba(0, 0, 0, 0.015);
  }

  .cardTitle {
    color: var(--muted);
    font-family: var(--font-sans);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 12px;
    line-height: 16px;
  }

  .field {
    display: grid;
    gap: 6px;
  }

  .label {
    color: var(--muted);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 11px;
    line-height: 14px;
    font-family: var(--font-sans);
  }

  .row {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    align-items: center;
  }

  .btnIcon {
    width: 14px;
    height: 14px;
    display: inline-grid;
    place-items: center;
    flex: 0 0 auto;
  }

  .folderAction {
    min-height: 40px;
    align-self: end;
  }

  .subtlePath {
    overflow-wrap: anywhere;
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
    font-family: var(--font-sans);
  }

  .value {
    overflow-wrap: anywhere;
    color: var(--fg);
    opacity: 0.64;
    font-weight: 400;
    font-family: var(--font-sans);
    font-size: 13px;
    line-height: 18px;
  }

  .muted {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
  }

  .mini {
    appearance: none;
    font: inherit;
    padding: 7px 9px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 11px;
    line-height: 14px;
  }

  .mini:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
