<script lang="ts">
  import type { BridgeInstanceStatus, Channel } from "$lib/api/types";
  import ChoiceDropdown from "$lib/ui/ChoiceDropdown.svelte";
  import ChannelDropdown from "$lib/ui/ChannelDropdown.svelte";
  import TagDropdown from "$lib/ui/TagDropdown.svelte";
  import FolderIcon from "$lib/ui/icons/FolderIcon.svelte";
  import { formatEnvironmentLabel } from "$lib/ui/instance/firmwarePresentation";

  export let instance: BridgeInstanceStatus;
  export let artifactConfigPath: string | null = null;
  export let disabled = false;
  export let loadingTags = false;
  export let activeTagValue = "";
  export let activeTagOptions: { value: string; label: string }[] = [];
  export let needsDownload = false;
  export let canFlash = false;
  export let flashing = false;
  export let selectedFirmware = "-";
  export let errorMessage: string | null = null;
  export let errorActions: string[] = [];
  export let flashNotice: { instanceId: string | null; level: "warn"; message: string } | null = null;
  export let onEnvironmentChange: (source: "installed" | "workspace") => void;
  export let onTargetChange: (target: "standalone" | "bitwig") => void;
  export let onOpenFolder: () => void;
  export let onChannelChange: (channel: Channel) => void;
  export let onTagChange: (tag: string | null) => void;
  export let onDownload: () => void;
  export let onFlash: () => void;

  $: step1Valid = instance.artifact_source === "installed" || instance.artifact_source === "workspace";
  $: step2Valid =
    (instance.target === "standalone" || instance.target === "bitwig") &&
    !!(instance.artifact_location_path ?? artifactConfigPath ?? "").trim();
  $: step3Valid = canFlash;
</script>

<div class="card">
  <section class="step">
    <div class="stepHeader">
      <div class="stepIndex" class:valid={step1Valid}>1</div>
      <div class="stepTitleWrap">
        <div class="stepTitle">Select Environment</div>
        <div class="stepDetail">Choose the firmware environment.</div>
      </div>
    </div>
    <div class="stepBody">
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
    </div>
  </section>

  <section class="step">
    <div class="stepHeader">
      <div class="stepIndex" class:valid={step2Valid}>2</div>
      <div class="stepTitleWrap">
        <div class="stepTitle">Select Firmware</div>
        <div class="stepDetail">
          {#if instance.artifact_source === "installed"}
            Choose the target and distribution release, then download it if needed.
          {:else}
            Choose the target and confirm the development firmware artifact.
          {/if}
        </div>
      </div>
    </div>
    <div class="stepBody">
      <div class="row">
        <ChoiceDropdown
          value={instance.target}
          placeholder="Select"
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

      {#if instance.artifact_source === "installed"}
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
            {needsDownload ? "Download" : "Downloaded"}
          </button>
        </div>
      {/if}

      <div class="subtlePath">
        {formatEnvironmentLabel(instance.artifact_source)} path: {instance.artifact_location_path ?? artifactConfigPath ?? "-"}
      </div>

      {#if instance.artifact_message}
        <div class="muted">{instance.artifact_message}</div>
      {/if}
    </div>
  </section>

  <section class="step">
    <div class="stepHeader">
      <div class="stepIndex" class:valid={step3Valid}>3</div>
      <div class="stepTitleWrap">
        <div class="stepTitle">Flash Controller</div>
        <div class="stepDetail">
          {#if needsDownload}
            Download the selected release before flashing.
          {:else if canFlash}
            Controller is ready for firmware update.
          {:else}
            Finish the firmware selection step before flashing.
          {/if}
        </div>
      </div>
    </div>
    <div class="stepBody">
      <div class="field">
        <div class="label">Selected Firmware</div>
        <div class="value">{selectedFirmware}</div>
      </div>

      <div class="actions">
        <button class="btn primary" type="button" disabled={disabled || !canFlash} onclick={onFlash}>
          {flashing ? "Flashing..." : "Flash Firmware"}
        </button>
      </div>

      {#if errorMessage}
        <div class="err">
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
        <div class="warn">
          {flashNotice.message}
        </div>
      {/if}
    </div>
  </section>

  {#if instance.message && !instance.running}
    <div class="muted">{instance.message}</div>
  {/if}
</div>

<style>
  .card {
    display: grid;
    gap: var(--space-4);
  }

  .field {
    display: grid;
    gap: var(--space-1);
  }

  .step {
    display: grid;
    gap: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    padding: var(--space-3) var(--space-4);
    background: color-mix(in srgb, var(--panel) 70%, transparent);
  }

  .stepHeader {
    display: flex;
    gap: var(--space-3);
    align-items: center;
  }

  .stepIndex {
    width: 24px;
    height: 24px;
    border-radius: 999px;
    border: 1px solid var(--border-strong);
    color: var(--muted);
    display: grid;
    place-items: center;
    font-size: 11px;
    line-height: 14px;
    font-weight: 800;
    font-family: var(--font-sans);
    flex: 0 0 auto;
  }

  .stepIndex.valid {
    border-color: var(--ok);
    color: var(--ok);
  }

  .stepTitleWrap {
    display: grid;
    gap: 2px;
  }

  .stepTitle {
    color: var(--fg);
    font-size: 13px;
    line-height: 16px;
    font-weight: 700;
    font-family: var(--font-sans);
  }

  .stepDetail {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
    font-family: var(--font-sans);
  }

  .stepBody {
    display: grid;
    gap: var(--space-3);
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
    gap: var(--space-3);
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
    min-height: var(--control-height);
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
    padding: 7px var(--control-padding-x);
    min-height: var(--control-height);
    border-radius: var(--control-radius);
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

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
  }

  .btn {
    appearance: none;
    font: inherit;
    padding: 8px var(--control-padding-x);
    min-height: var(--control-height);
    border-radius: var(--control-radius);
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 12px;
    line-height: 14px;
  }

  .btn.primary {
    background: var(--value);
    color: var(--bg);
    border-color: var(--value);
  }

  .btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .err {
    color: var(--err);
    font-size: 12px;
    line-height: 16px;
    border: 1px solid var(--err);
    border-radius: var(--control-radius);
    padding: var(--space-3) var(--space-4);
  }

  .hintTitle {
    margin-top: 8px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 11px;
    line-height: 14px;
  }

  .hintList {
    margin: 8px 0 0;
    padding-left: 18px;
    display: grid;
    gap: 4px;
  }

  .warn {
    color: var(--warn);
    font-size: 12px;
    line-height: 16px;
    border: 1px solid var(--warn);
    border-radius: var(--control-radius);
    padding: var(--space-3) var(--space-4);
  }
</style>
