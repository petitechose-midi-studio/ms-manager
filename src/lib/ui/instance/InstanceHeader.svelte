<script lang="ts">
  import type { BridgeInstanceStatus } from "$lib/api/types";
  import LogIcon from "$lib/ui/icons/LogIcon.svelte";
  import PowerIcon from "$lib/ui/icons/PowerIcon.svelte";
  import TrashIcon from "$lib/ui/icons/TrashIcon.svelte";
  import {
    formatEnvironmentLabel,
    formatLastFlashValue,
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
    if (!instance.running) return "Bridge down";
    if (instance.paused) return "Paused";
    return instance.serial_open ? "Connected" : "Waiting for controller";
  }

  function fmtStateKind(instance: {
    enabled: boolean;
    running: boolean;
    paused: boolean;
    serial_open: boolean;
  }): "ok" | "warn" | "err" | "idle" {
    if (!instance.enabled) return "idle";
    if (!instance.running) return "err";
    if (instance.paused || !instance.serial_open) return "warn";
    return "ok";
  }

  function fmtPort(port?: string | null): string {
    return port?.trim() || "-";
  }

  $: instanceState = fmtInstanceState(instance);
  $: stateKind = fmtStateKind(instance);
</script>

<div class="instanceHeader">
  <div class="instanceHeaderMain">
    <div class="titleRow">
      <span class="connectionDot" data-state={stateKind} title={instanceState} aria-label={instanceState}></span>
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
      <span class="configPill">
        {formatEnvironmentLabel(instance.artifact_source)} / {formatTargetLabel(instance.target)}
      </span>
      {#if stateKind !== "ok"}
        <span class="stateLabel" data-state={stateKind}>{instanceState}</span>
      {/if}
    </div>

    <div class="instanceMeta">
      <div class="metaItem">
        <span class="metaLabel">Serial ID</span>
        <span class="metaValue">{instance.configured_serial}</span>
      </div>
      <div class="metaItem">
        <span class="metaLabel">Serial port</span>
        <span class="metaValue">{fmtPort(instance.resolved_serial_port)}</span>
      </div>
      <div class="metaItem">
        <span class="metaLabel">Bridge UDP</span>
        <span class="metaValue">{instance.host_udp_port}</span>
      </div>
      <div class="metaItem lastFlash">
        <span class="metaLabel">Last flash</span>
        <span class="metaValue">{formatLastFlashValue(instance.last_flashed)}</span>
      </div>
    </div>
  </div>

  <div class="headerActions" aria-label="Controller actions">
    <button class="iconButton" type="button" title="Open bridge logs" aria-label="Open bridge logs" onclick={onOpenLogs}>
      <LogIcon size={16} />
    </button>
    <button
      class="iconButton power"
      type="button"
      title={instance.enabled ? "Disable controller" : "Enable controller"}
      aria-label={instance.enabled ? "Disable controller" : "Enable controller"}
      disabled={busy}
      onclick={onToggleEnabled}
    >
      <PowerIcon size={16} />
    </button>
    <button
      class="iconButton danger"
      type="button"
      title="Remove controller"
      aria-label="Remove controller"
      disabled={busy}
      onclick={onRemove}
    >
      <TrashIcon size={16} />
    </button>
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
    gap: var(--space-3);
    min-width: 0;
    flex: 1 1 auto;
  }

  .titleRow {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    min-height: 28px;
  }

  .connectionDot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--muted);
    flex: 0 0 auto;
  }

  .connectionDot[data-state="ok"] {
    background: var(--ok);
  }

  .connectionDot[data-state="warn"] {
    background: var(--warn);
  }

  .connectionDot[data-state="err"] {
    background: var(--err);
  }

  .instanceTitleButton {
    appearance: none;
    border: 0;
    background: transparent;
    color: var(--fg);
    font-family: var(--font-sans);
    font-size: 20px;
    font-weight: 700;
    line-height: 24px;
    padding: 0;
    margin: 0;
    cursor: default;
    text-align: left;
    max-width: min(360px, 40vw);
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

  .titleInput {
    appearance: none;
    width: min(360px, 40vw);
    min-width: 0;
    border: 0;
    border-bottom: 1px solid var(--value);
    background: transparent;
    color: var(--fg);
    padding: 0 0 2px;
    font-family: var(--font-sans);
    font-weight: 700;
    font-size: 20px;
    line-height: 24px;
    outline: none;
    caret-color: var(--fg);
  }

  .titleInput:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .configPill,
  .stateLabel {
    color: var(--muted);
    font-size: 10px;
    line-height: 14px;
    font-family: var(--font-sans);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 3px 8px;
    white-space: nowrap;
  }

  .stateLabel[data-state="warn"] {
    color: var(--warn);
    border-color: color-mix(in srgb, var(--warn) 58%, var(--border));
  }

  .stateLabel[data-state="err"] {
    color: var(--err);
    border-color: color-mix(in srgb, var(--err) 58%, var(--border));
  }

  .instanceMeta {
    display: flex;
    align-items: flex-start;
    gap: var(--space-4);
    flex-wrap: wrap;
  }

  .metaItem {
    display: grid;
    gap: 2px;
    min-width: 84px;
  }

  .metaItem.lastFlash {
    min-width: min(280px, 100%);
  }

  .metaLabel {
    color: var(--muted);
    font: 700 10px/13px var(--font-sans);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .metaValue {
    color: color-mix(in srgb, var(--fg) 84%, var(--muted));
    font: 400 12px/16px var(--font-mono);
    overflow-wrap: anywhere;
  }

  .headerActions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex: 0 0 auto;
  }

  .iconButton {
    appearance: none;
    width: 34px;
    height: 34px;
    padding: 0;
    border-radius: var(--control-radius);
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    display: grid;
    place-items: center;
    transition: 120ms ease;
    transition-property: color, border-color, background-color, transform;
  }

  .iconButton:hover:not(:disabled) {
    color: var(--fg);
    border-color: var(--border-strong);
    background: color-mix(in srgb, var(--fg) 5%, transparent);
  }

  .iconButton:active:not(:disabled) {
    transform: translateY(1px);
  }

  .iconButton:focus-visible {
    outline: 2px solid var(--value);
    outline-offset: 2px;
  }

  .iconButton.power {
    color: var(--warn);
  }

  .iconButton.danger {
    color: var(--err);
  }

  .iconButton:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  @media (max-width: 760px) {
    .instanceHeader {
      gap: var(--space-3);
    }

    .configPill {
      display: none;
    }

    .instanceTitleButton,
    .titleInput {
      max-width: calc(100vw - 250px);
      width: auto;
    }
  }
</style>
