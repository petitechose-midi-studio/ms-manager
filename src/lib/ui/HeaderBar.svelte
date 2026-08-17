<script lang="ts">
  import type { DeviceStatus, MidiInventoryStatus, Platform } from "$lib/api/types";
  import ControllerStatus from "$lib/ui/ControllerStatus.svelte";
  import MidiInventoryOverviewBadge from "$lib/ui/MidiInventoryOverviewBadge.svelte";
  export let device: DeviceStatus;
  export let midiInventory: MidiInventoryStatus | null = null;
  export let loadingMidiInventory = false;
  export let midiLinkLabelsBySerial: Record<string, string> = {};
  export let platform: Platform | null;

  export let appUpdateAvailable: boolean;
  export let appUpdateLabel: string | null;
  export let onRefreshMidiInventory: () => void = () => {};
</script>

<header class="bar">
  <div class="title">
    <span class="brand">
      <svg class="appMark" viewBox="0 0 24 24" aria-hidden="true">
        <path
          d="M5 21V8.5A5.5 5.5 0 0 1 10.5 3H14a5.5 5.5 0 0 1 5.5 5.5v.75A5.75 5.75 0 0 1 13.75 15c-1.5 0-2.95.25-4.25.75"
        ></path>
        <circle cx="12.3" cy="8.6" r="2.05"></circle>
      </svg>
      <span class="brandCopy">
        <span class="app">MIDI Studio Manager</span>
        <span class="maker">petitechose.audio</span>
      </span>
    </span>
    <MidiInventoryOverviewBadge
      inventory={midiInventory}
      loading={loadingMidiInventory}
      linkLabelsBySerial={midiLinkLabelsBySerial}
      onRefresh={onRefreshMidiInventory}
    />
  </div>

  <div class="meta">
    {#if appUpdateAvailable}
      <div class="badge" data-kind="warn">
        <span class="dot" aria-hidden="true"></span>
        <span class="text">{appUpdateLabel ?? "update available"}</span>
      </div>
    {/if}

    <ControllerStatus
      device={device}
      variant="badge"
      textOverride={`${device.count} device${device.count === 1 ? "" : "s"}`}
      align="center"
    />

    {#if platform}
      <div class="badge">
        <span class="text">{platform.os}/{platform.arch}</span>
      </div>
    {/if}
  </div>
</header>

<style>
  .bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-4);
    padding: var(--space-1) 2px;
    font-family: var(--font-sans);
  }

  .app {
    display: block;
    font-weight: 700;
    font-size: 15px;
    line-height: 17px;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: var(--fg);
  }

  .maker {
    display: block;
    color: var(--muted);
    font-size: 10px;
    line-height: 12px;
    letter-spacing: 0.02em;
  }

  .brand {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    white-space: nowrap;
  }

  .appMark {
    width: 30px;
    height: 30px;
    display: block;
    flex: 0 0 auto;
    color: var(--fg);
    fill: none;
    stroke: currentColor;
    stroke-width: 3.2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .appMark circle {
    fill: var(--brand);
    stroke: none;
  }

  .brandCopy {
    display: block;
  }

  .title {
    display: inline-flex;
    align-items: center;
    gap: var(--space-4);
    flex-wrap: wrap;
  }

  .meta {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .badge {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--pill-padding-y) var(--pill-padding-x);
    border-radius: 999px;
    border: 1px solid var(--border);
    background: rgba(0, 0, 0, 0.06);
    color: var(--muted);
    line-height: 16px;
    font-size: 12px;
    user-select: none;
    font-family: var(--font-sans);
    font-weight: 500;
  }

  :global(:root[data-theme="light"]) .badge {
    background: rgba(0, 0, 0, 0.03);
  }

  .badge[data-kind="warn"] {
    color: var(--value);
    border-color: var(--border-strong);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--border-strong);
  }

  .badge[data-kind="warn"] .dot {
    background: var(--warn);
  }
</style>
