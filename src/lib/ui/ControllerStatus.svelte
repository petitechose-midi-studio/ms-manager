<script lang="ts">
  import { tick } from "svelte";

  import type { DeviceStatus, DeviceTarget } from "$lib/api/types";
  import ControllerIcon from "$lib/ui/icons/ControllerIcon.svelte";
  import { portal } from "$lib/ui/actions/portal";

  export let device: DeviceStatus;
  export let variant: "badge" | "pill" = "pill";
  export let label: string = "";
  export let showCount = false;

  let anchorEl: HTMLButtonElement | null = null;
  let overlayEl: HTMLDivElement | null = null;

  let open = false;
  let anchorHover = false;
  let overlayHover = false;
  let closeTimer: ReturnType<typeof setTimeout> | null = null;
  let x = 0;
  let y = 0;

  function statusText(d: DeviceStatus): string {
    const base = d.connected ? "connected" : "not detected";
    return label ? `${label}: ${base}` : base;
  }

  function portFor(t: DeviceTarget): string {
    if (t.kind === "serial") return t.port_name ?? t.target_id;
    return t.path ?? t.target_id;
  }

  function detailsFor(t: DeviceTarget): string | null {
    const parts = [t.product, t.manufacturer, t.serial_number].filter(Boolean) as string[];
    if (!parts.length) return null;
    return parts.join(" / ");
  }

  $: targets = device.targets ?? [];
  $: showOverlay = device.connected && targets.length > 0;

  $: if (!showOverlay) {
    open = false;
  }

  function scheduleClose() {
    if (closeTimer) {
      clearTimeout(closeTimer);
      closeTimer = null;
    }

    closeTimer = setTimeout(() => {
      if (!anchorHover && !overlayHover) {
        open = false;
      }
    }, 90);
  }

  function setOpen(v: boolean) {
    open = v;
  }

  function updatePosition() {
    if (!anchorEl || !overlayEl) return;

    const a = anchorEl.getBoundingClientRect();
    const o = overlayEl.getBoundingClientRect();
    const margin = 12;

    let nextX = a.right - o.width;
    nextX = Math.max(margin, Math.min(nextX, window.innerWidth - o.width - margin));

    let nextY = a.bottom + 10;
    if (nextY + o.height + margin > window.innerHeight) {
      nextY = a.top - 10 - o.height;
    }
    nextY = Math.max(margin, Math.min(nextY, window.innerHeight - o.height - margin));

    x = Math.round(nextX);
    y = Math.round(nextY);
  }

  async function openOverlay() {
    if (!showOverlay) return;

    if (closeTimer) {
      clearTimeout(closeTimer);
      closeTimer = null;
    }

    setOpen(true);
    await tick();
    updatePosition();
  }

  function onAnchorEnter() {
    anchorHover = true;
    void openOverlay();
  }

  function onAnchorLeave() {
    anchorHover = false;
    scheduleClose();
  }

  function onOverlayEnter() {
    overlayHover = true;
  }

  function onOverlayLeave() {
    overlayHover = false;
    scheduleClose();
  }
</script>

<button
  type="button"
  class="wrap"
  bind:this={anchorEl}
  data-kind={device.connected ? "ok" : "muted"}
  data-variant={variant}
  aria-label={showOverlay ? "Detected controllers" : undefined}
  onmouseenter={onAnchorEnter}
  onmouseleave={onAnchorLeave}
  onfocus={openOverlay}
  onblur={onAnchorLeave}
>
  <span class="dot" aria-hidden="true"></span>
  <span class="ico" aria-hidden="true"><ControllerIcon size={14} /></span>
  <span class="text">{statusText(device)}</span>
  {#if showCount && device.connected}
    <span class="count">{device.count}</span>
  {/if}

</button>

{#if showOverlay && open}
  <div
    class="overlay"
    use:portal
    bind:this={overlayEl}
    role="tooltip"
    aria-label="Detected controllers"
    style={`left:${x}px; top:${y}px;`}
    onmouseenter={onOverlayEnter}
    onmouseleave={onOverlayLeave}
  >
    <div class="overlayHead">Controllers ({targets.length})</div>
    <div class="overlayBody">
      {#each targets as t (t.target_id)}
        <div class="t">
          <div class="tTop">
            <span class="kind">{t.kind}</span>
            <span class="port">{portFor(t)}</span>
          </div>
          {#if detailsFor(t)}
            <div class="tMeta">{detailsFor(t)}</div>
          {/if}
        </div>
      {/each}
    </div>
  </div>
{/if}

<style>
  .wrap {
    appearance: none;
    font: inherit;
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 5px 8px;
    border-radius: 999px;
    border: 1px solid var(--border);
    color: var(--muted);
    line-height: 16px;
    font-size: 12px;
    user-select: none;
    cursor: default;
  }

  .wrap:focus-visible {
    outline: 2px solid var(--border-strong);
    outline-offset: 2px;
  }

  .wrap[data-variant="badge"] {
    background: rgba(0, 0, 0, 0.06);
  }

  :global(:root[data-theme="light"]) .wrap[data-variant="badge"] {
    background: rgba(0, 0, 0, 0.03);
  }

  .wrap[data-variant="pill"] {
    background: var(--bg);
  }

  .wrap[data-kind="ok"] {
    color: var(--value);
    border-color: var(--border-strong);
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--border-strong);
  }

  .wrap[data-kind="ok"] .dot {
    background: var(--ok);
  }

  .ico {
    width: 16px;
    height: 16px;
    display: grid;
    place-items: center;
    opacity: 0.9;
  }

  .count {
    margin-left: 2px;
    padding: 2px 6px;
    border-radius: 6px;
    border: 1px solid var(--border);
    font-family: var(--font-mono);
    font-weight: 800;
    font-size: 11px;
    line-height: 14px;
    color: inherit;
  }

  .overlay {
    position: fixed;
    width: min(420px, calc(100vw - 24px));
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--panel);
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.35);
    padding: 10px 12px;
    opacity: 1;
    pointer-events: auto;
    z-index: 6000;
  }

  .overlayHead {
    color: var(--muted);
    font-weight: 800;
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
    gap: 10px;
    max-height: 42vh;
    overflow: auto;
    scrollbar-gutter: stable;
  }

  .t {
    display: grid;
    gap: 4px;
  }

  .tTop {
    display: flex;
    gap: 10px;
    align-items: baseline;
    justify-content: space-between;
  }

  .kind {
    font-family: var(--font-mono);
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 11px;
    line-height: 14px;
    color: var(--muted);
  }

  .port {
    font-family: var(--font-mono);
    font-weight: 700;
    font-size: 12px;
    line-height: 16px;
    color: var(--value);
    overflow-wrap: anywhere;
    text-align: right;
  }

  .tMeta {
    font-size: 12px;
    line-height: 16px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
</style>
