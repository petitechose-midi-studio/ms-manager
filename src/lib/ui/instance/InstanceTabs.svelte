<script lang="ts" context="module">
  import type { BridgeInstanceStatus, DeviceTarget } from "$lib/api/types";

  export type ControllerTabItem =
    | {
        key: string;
        kind: "instance";
        serial: string;
        label: string;
        instance: BridgeInstanceStatus;
      }
    | {
        key: string;
        kind: "unbound";
        serial: string;
        label: string;
        subtitle: string;
        target: DeviceTarget;
      };
</script>

<script lang="ts">
  import BitwigIcon from "$lib/ui/icons/BitwigIcon.svelte";
  import ControllerIcon from "$lib/ui/icons/ControllerIcon.svelte";

  export let tabs: ControllerTabItem[] = [];
  export let activeKey: string | null = null;
  export let onSelect: (key: string) => void;

  function firmwareIconKind(profile?: string | null, fallbackTarget?: string | null): "bitwig" | "controller" {
    if (profile === "bitwig") return "bitwig";
    if (profile === "default") return "controller";
    return fallbackTarget === "bitwig" ? "bitwig" : "controller";
  }

  function instanceDotKind(instance: {
    enabled: boolean;
    serial_open: boolean;
  }): "ok" | "warn" | "muted" {
    if (!instance.enabled) return "warn";
    if (instance.serial_open) return "ok";
    return "muted";
  }
</script>

<div class="tabs">
  {#if !tabs.length}
    <div class="emptyTabs">No controller detected.</div>
  {:else}
    {#each tabs as tab (tab.key)}
      <button
        class="tab"
        class:active={tab.key === activeKey}
        type="button"
        onclick={() => onSelect(tab.key)}
      >
        <span class="tabLabel">{tab.label}</span>
        {#if tab.kind === "instance"}
          <span class="tabMetaRow">
            <span class="tabDot" data-kind={instanceDotKind(tab.instance)} aria-hidden="true"></span>
            <span class="tabMetaIcon" aria-hidden="true">
              {#if firmwareIconKind(tab.instance.last_flashed?.profile, tab.instance.target) === "bitwig"}
                <BitwigIcon size={13} />
              {:else}
                <ControllerIcon size={13} />
              {/if}
            </span>
            <span class="tabMeta">Port {tab.instance.host_udp_port}</span>
          </span>
        {:else}
          <span class="tabMetaRow">
            <span class="tabDot" data-kind="muted" aria-hidden="true"></span>
            <span class="tabMeta">{tab.subtitle}</span>
          </span>
        {/if}
      </button>
    {/each}
  {/if}
</div>

<style>
  .tabs {
    display: flex;
    align-items: stretch;
    gap: 8px;
    padding: 10px 12px;
    min-height: 78px;
    border-bottom: 1px solid var(--border);
    overflow-x: auto;
    overflow-y: hidden;
    background: rgba(0, 0, 0, 0.06);
  }

  :global(:root[data-theme="light"]) .tabs {
    background: rgba(0, 0, 0, 0.03);
  }

  .tab {
    appearance: none;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    border-radius: 8px;
    padding: 10px 12px;
    min-height: 56px;
    min-width: 180px;
    flex: 0 0 180px;
    display: grid;
    gap: 4px;
    text-align: left;
    cursor: pointer;
    font: inherit;
  }

  .tab.active {
    border-color: var(--value);
    color: var(--fg);
    background: rgba(255, 255, 255, 0.04);
  }

  .tabLabel {
    font-family: var(--font-sans);
    font-weight: 700;
    font-size: 13px;
    line-height: 16px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tabMeta {
    font-family: var(--font-sans);
    font-weight: 500;
    font-size: 11px;
    line-height: 14px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .tabMetaRow {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .tabDot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: var(--border-strong);
    flex: 0 0 auto;
  }

  .tabDot[data-kind="ok"] {
    background: var(--ok);
  }

  .tabDot[data-kind="warn"] {
    background: var(--warn);
  }

  .tabMetaIcon {
    width: 14px;
    height: 14px;
    display: inline-grid;
    place-items: center;
    color: var(--muted);
    flex: 0 0 auto;
  }

  .emptyTabs {
    color: var(--muted);
    font-size: 12px;
  }
</style>
