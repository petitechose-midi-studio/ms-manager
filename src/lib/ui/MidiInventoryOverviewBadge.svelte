<script lang="ts">
  import { tick } from "svelte";

  import type { MidiInventoryStatus, MidiPortInfo } from "$lib/api/types";
  import { portal } from "$lib/ui/actions/portal";

  export let inventory: MidiInventoryStatus | null = null;
  export let loading = false;
  export let linkLabelsBySerial: Record<string, string> = {};
  export let onRefresh: () => void = () => {};

  let anchorEl: HTMLButtonElement | null = null;
  let overlayEl: HTMLDivElement | null = null;
  let open = false;
  let anchorHover = false;
  let overlayHover = false;
  let closeTimer: ReturnType<typeof setTimeout> | null = null;
  let x = 0;
  let y = 0;

  function directionLabel(direction: MidiPortInfo["direction"]): string {
    if (direction === "input") return "IN";
    if (direction === "output") return "OUT";
    return "I/O";
  }

  function linkLabel(port: MidiPortInfo): string | null {
    const serial = port.match_info.controller_serial?.trim();
    const linkedName = serial ? linkLabelsBySerial[serial] : null;

    if (port.match_info.confidence === "strong") {
      return linkedName ? `Linked: ${linkedName}` : "Linked";
    }
    if (port.match_info.confidence === "weak") {
      return linkedName ? `Possible: ${linkedName}` : "Possible";
    }
    return null;
  }

  $: ports = inventory?.ports ?? [];
  $: waiting = loading && !inventory;
  $: summary = waiting
    ? "Waiting MIDI"
    : loading
      ? "Scanning MIDI Ports"
      : `${ports.length} MIDI Port${ports.length === 1 ? "" : "s"}`;

  function scheduleClose() {
    if (closeTimer) clearTimeout(closeTimer);
    closeTimer = setTimeout(() => {
      if (!anchorHover && !overlayHover) {
        open = false;
      }
    }, 90);
  }

  function updatePosition() {
    if (!anchorEl || !overlayEl) return;
    const a = anchorEl.getBoundingClientRect();
    const o = overlayEl.getBoundingClientRect();
    const margin = 12;
    x = Math.round(Math.max(margin, Math.min(a.left, window.innerWidth - o.width - margin)));
    y = Math.round(Math.min(a.bottom + 10, window.innerHeight - o.height - margin));
  }

  async function openOverlay() {
    if (!ports.length && !inventory?.notes.length) return;
    if (closeTimer) {
      clearTimeout(closeTimer);
      closeTimer = null;
    }
    open = true;
    await tick();
    updatePosition();
  }
</script>

<div class="wrap">
  <button
    type="button"
    class="badge"
    bind:this={anchorEl}
    aria-label="System MIDI ports"
    onmouseenter={() => {
      anchorHover = true;
      void openOverlay();
    }}
    onmouseleave={() => {
      anchorHover = false;
      scheduleClose();
    }}
    onfocus={openOverlay}
    onblur={() => {
      anchorHover = false;
      scheduleClose();
    }}
  >
    <span class="dot" aria-hidden="true"></span>
    <span class="text">{summary}</span>
  </button>

  <button
    type="button"
    class="refresh"
    aria-label="Refresh MIDI inventory"
    disabled={loading}
    onclick={onRefresh}
  >
    ↻
  </button>
</div>

{#if open}
  <div
    class="overlay"
    use:portal
    bind:this={overlayEl}
    role="tooltip"
    aria-label="System MIDI inventory"
    style={`left:${x}px; top:${y}px;`}
    onmouseenter={() => {
      overlayHover = true;
      if (closeTimer) clearTimeout(closeTimer);
    }}
    onmouseleave={() => {
      overlayHover = false;
      scheduleClose();
    }}
  >
    <div class="overlayHead">System MIDI</div>
    <div class="overlayBody">
      {#if waiting}
        <div class="muted">MIDI inventory is still loading.</div>
      {:else if ports.length}
        {#each ports as port (port.id)}
          <div class="row">
            <span class="dir" data-direction={port.direction}>{directionLabel(port.direction)}</span>
            <span class="name">{port.name}</span>
            {#if linkLabel(port)}
              <span class="state" class:strong={port.match_info.confidence === "strong"} class:weak={port.match_info.confidence === "weak"}>
                {linkLabel(port)}
              </span>
            {/if}
          </div>
        {/each}
      {:else}
        <div class="muted">No system MIDI port reported.</div>
      {/if}

      {#if inventory?.notes?.length}
        {#each inventory.notes as note}
          <div class="muted">{note}</div>
        {/each}
      {/if}
    </div>
  </div>
{/if}

<style>
  .wrap {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .badge,
  .refresh {
    appearance: none;
    font: inherit;
    font-family: var(--font-sans);
    border: 1px solid var(--border);
    background: rgba(0, 0, 0, 0.06);
    color: var(--muted);
    border-radius: 999px;
    line-height: 16px;
    user-select: none;
  }

  .badge {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 5px 8px;
    cursor: default;
    font-size: 12px;
    font-weight: 500;
  }

  .refresh {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    cursor: pointer;
    font-size: 14px;
  }

  .refresh:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  :global(:root[data-theme="light"]) .badge,
  :global(:root[data-theme="light"]) .refresh {
    background: rgba(0, 0, 0, 0.03);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ok);
  }

  .overlay {
    position: fixed;
    width: min(380px, calc(100vw - 24px));
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: color-mix(in srgb, var(--panel) 96%, black 4%);
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.35);
    padding: 10px 12px;
    z-index: 6000;
  }

  .overlayHead {
    color: var(--muted);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 11px;
    line-height: 14px;
    padding-bottom: 8px;
    border-bottom: 1px solid var(--border);
  }

  .overlayBody {
    padding-top: 10px;
    display: grid;
    gap: 8px;
    max-height: 42vh;
    overflow: auto;
  }

  .row {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 8px;
    align-items: center;
  }

  .dir,
  .state {
    border-radius: 999px;
    padding: 2px 6px;
    font-size: 10px;
    line-height: 12px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    white-space: nowrap;
  }

  .dir {
    color: var(--muted);
    background: color-mix(in srgb, var(--muted) 10%, transparent);
  }

  .dir[data-direction="input"] {
    color: var(--value);
    background: color-mix(in srgb, var(--value) 12%, transparent);
  }

  .dir[data-direction="output"] {
    color: var(--ok);
    background: color-mix(in srgb, var(--ok) 14%, transparent);
  }

  .state.strong {
    color: var(--value);
    background: color-mix(in srgb, var(--value) 12%, transparent);
  }

  .state.weak {
    color: var(--warn);
    background: color-mix(in srgb, var(--warn) 12%, transparent);
  }

  .name,
  .muted {
    font-size: 12px;
    line-height: 16px;
    overflow-wrap: anywhere;
  }

  .name {
    color: var(--fg);
  }

  .muted {
    color: var(--muted);
  }
</style>
