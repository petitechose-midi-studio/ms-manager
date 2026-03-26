<script lang="ts">
  import type { DeviceStatus, Platform } from "$lib/api/types";
  import ControllerStatus from "$lib/ui/ControllerStatus.svelte";
  export let device: DeviceStatus;
  export let platform: Platform | null;

  export let appUpdateAvailable: boolean;
  export let appUpdateLabel: string | null;
</script>

<header class="bar">
  <div class="title">
    <span class="app">MIDI Studio Manager</span>
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
    gap: 12px;
    padding: 6px 2px;
    font-family: var(--font-sans);
  }

  .app {
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--value);
  }

  .meta {
    display: flex;
    gap: 10px;
    align-items: center;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .badge {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 5px 8px;
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
