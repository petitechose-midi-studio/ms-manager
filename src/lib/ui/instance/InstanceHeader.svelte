<script lang="ts">
  import type { BridgeInstanceStatus } from "$lib/api/types";
  import {
    formatEnvironmentLabel,
    formatLastFlashLabel,
    formatTargetLabel,
  } from "$lib/ui/instance/firmwarePresentation";

  export let instance: BridgeInstanceStatus;
  export let fallbackName: string;
  export let renaming = false;
  export let nameDraft = "";
  export let busy = false;
  export let onNameInput: (value: string) => void;
  export let onTitleKeydown: (event: KeyboardEvent) => void;
  export let onSaveName: () => void;
  export let onBeginRename: () => void;
  export let onOpenLogs: () => void;
  export let onToggleEnabled: () => void;
  export let onRemove: () => void;

  function fmtInstanceState(instance: {
    enabled: boolean;
    running: boolean;
    paused: boolean;
    serial_open: boolean;
  }): string {
    if (!instance.enabled) return "Disabled";
    if (!instance.running) return "Down";
    if (instance.paused) return "Paused";
    return instance.serial_open ? "Serial Open" : "Waiting";
  }

  function fmtPort(port?: string | null): string {
    return port?.trim() || "-";
  }
</script>

<div class="instanceHeader">
  <div class="instanceHeaderMain">
    <div class="titleRow">
      {#if renaming}
        <input
          class="titleInput"
          type="text"
          value={nameDraft}
          placeholder={fallbackName}
          disabled={busy}
          oninput={(event) => onNameInput((event.currentTarget as HTMLInputElement).value)}
          onkeydown={onTitleKeydown}
          onblur={onSaveName}
        />
      {:else}
        <button
          class="instanceTitleButton"
          type="button"
          title="Double-click to rename instance"
          disabled={busy}
          ondblclick={onBeginRename}
        >
          {instance.display_name?.trim() || fallbackName}
        </button>
      {/if}
    </div>
    <div class="instanceMeta">
      Serial ID: {instance.configured_serial}
      · Port: {fmtPort(instance.resolved_serial_port)}
      · Bridge Port: {instance.host_udp_port}
    </div>
    <div class="instanceSubMeta">{formatLastFlashLabel(instance.last_flashed)}</div>
  </div>
  <div class="instanceHeaderSide">
    <div class="pillRow">
      <div class="configPill">
        {formatEnvironmentLabel(instance.artifact_source)} / {formatTargetLabel(instance.target)}
      </div>
      <div class="statePill" data-running={instance.running}>
        {fmtInstanceState(instance)}
      </div>
    </div>
    <div class="pillRow">
      <button class="mini" type="button" onclick={onOpenLogs}>Logs</button>
      <button class="mini warn" type="button" disabled={busy} onclick={onToggleEnabled}>
        {instance.enabled ? "Disable" : "Enable"}
      </button>
      <button class="mini danger" type="button" disabled={busy} onclick={onRemove}>Remove</button>
    </div>
  </div>
</div>

<style>
  .instanceHeader {
    display: flex;
    justify-content: space-between;
    gap: var(--space-4);
    align-items: flex-start;
  }

  .instanceHeaderMain {
    display: grid;
    gap: var(--space-2);
    min-width: 0;
  }

  .instanceHeaderSide {
    display: grid;
    gap: var(--space-2);
    justify-items: end;
  }

  .instanceTitleButton {
    appearance: none;
    border: 0;
    background: transparent;
    color: var(--fg);
    font-family: var(--font-sans);
    font-size: 18px;
    font-weight: 700;
    line-height: 22px;
    padding: 0;
    margin: 0;
    cursor: default;
    text-align: left;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .instanceTitleButton:hover {
    color: var(--value);
  }

  .instanceTitleButton:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .titleRow {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }

  .titleInput {
    appearance: none;
    width: 100%;
    min-width: 0;
    max-width: 360px;
    border: 0;
    background: transparent;
    color: var(--fg);
    border-radius: 0;
    padding: 0;
    font: inherit;
    font-family: var(--font-sans);
    font-weight: 700;
    font-size: 18px;
    line-height: 22px;
    outline: none;
    box-shadow: none;
    caret-color: var(--fg);
  }

  .titleInput:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .instanceMeta {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
    font-family: var(--font-sans);
    overflow-wrap: anywhere;
  }

  .instanceSubMeta {
    color: color-mix(in srgb, var(--muted) 88%, transparent);
    font-size: 11px;
    line-height: 15px;
    font-family: var(--font-sans);
    overflow-wrap: anywhere;
  }

  .pillRow {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .configPill,
  .statePill {
    color: var(--muted);
    font-size: 11px;
    line-height: 14px;
    font-family: var(--font-sans);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: var(--pill-padding-y) var(--pill-padding-x);
    white-space: nowrap;
  }

  .statePill[data-running="true"] {
    color: var(--ok);
    border-color: var(--ok);
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

  .mini.warn {
    color: var(--warn);
    border-color: var(--warn);
  }

  .mini.danger {
    color: var(--err);
    border-color: var(--err);
  }

  @media (max-width: 620px) {
    .instanceHeader {
      flex-direction: column;
      align-items: flex-start;
    }

    .instanceHeaderSide {
      justify-items: start;
    }
  }
</style>
