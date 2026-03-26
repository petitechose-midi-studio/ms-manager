<script lang="ts">
  import type { DeviceTarget } from "$lib/api/types";

  export let target: DeviceTarget;
  export let busy = false;
  export let onCreate: () => void;

  function unboundName(target: { product?: string | null; serial_number?: string | null }): string {
    return target.product?.trim() || `Controller ${target.serial_number ?? ""}`.trim();
  }
</script>

<div class="instanceHeader">
  <div>
    <div class="instanceTitle">{unboundName(target)}</div>
    <div class="instanceMeta">
      serial {target.serial_number}
      {#if target.port_name}
        · {target.port_name}
      {/if}
    </div>
  </div>
  <div class="statePill">Detected</div>
</div>

<div class="grid single">
  <div class="card">
    <div class="cardTitle">Controller</div>
    <div class="kv">
      <div class="k">Product</div>
      <div class="v">{target.product ?? "Controller"}</div>
      <div class="k">Manufacturer</div>
      <div class="v">{target.manufacturer ?? "-"}</div>
      <div class="k">Serial</div>
      <div class="v">{target.serial_number ?? "-"}</div>
      <div class="k">Port</div>
      <div class="v">{target.port_name ?? "-"}</div>
    </div>
    <div class="muted">
      Create a controller instance first. You will then be able to rename it, choose its
      source, choose its target, install releases, and flash firmware from this tab.
    </div>
  </div>

  <div class="card">
    <div class="cardTitle">Actions</div>
    <div class="actions">
      <button class="btn primary" type="button" disabled={busy} onclick={onCreate}>
        Create Instance
      </button>
    </div>
  </div>
</div>

<style>
  .instanceHeader {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    align-items: flex-start;
  }

  .instanceTitle {
    color: var(--fg);
    font-family: var(--font-sans);
    font-weight: 700;
    font-size: 18px;
    line-height: 22px;
  }

  .instanceMeta {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
    font-family: var(--font-sans);
    overflow-wrap: anywhere;
  }

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
    padding: 4px 10px;
    white-space: nowrap;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  .grid.single {
    grid-template-columns: 1fr;
  }

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

  .kv {
    display: grid;
    grid-template-columns: 120px 1fr;
    gap: 8px 10px;
    align-items: baseline;
  }

  .k {
    color: var(--muted);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 11px;
    line-height: 14px;
    font-family: var(--font-sans);
  }

  .k::after {
    content: ":";
    opacity: 0.6;
  }

  .v {
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

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  .btn {
    appearance: none;
    font: inherit;
    padding: 8px 10px;
    border-radius: 6px;
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

  @media (max-width: 980px) {
    .grid {
      grid-template-columns: 1fr;
    }
  }

  @media (max-width: 620px) {
    .instanceHeader {
      flex-direction: column;
      align-items: flex-start;
    }

    .kv {
      grid-template-columns: 1fr;
    }
  }
</style>
