<script lang="ts">
  import { tick } from "svelte";
  import { matchesActivityFilter } from "$lib/state/activity";
  import type { ActivityEntry, ActivityFilter } from "$lib/state/activity";

  export let open: boolean;
  export let filter: ActivityFilter;
  export let entries: ActivityEntry[];
  export let onToggle: () => void;
  export let onFilter: (f: ActivityFilter) => void;
  export let onCopy: () => void;
  export let onClear: () => void;

  let logEl: HTMLDivElement | null = null;
  let height = 220;
  let followBottom = true;
  let lastVisibleTailId = 0;
  let startY = 0;
  let startHeight = 220;
  let viewportHeight = 0;
  let scrollTop = 0;

  const ROW_HEIGHT = 22;
  const OVERSCAN = 18;

  const filters: { id: ActivityFilter; label: string }[] = [
    { id: "all", label: "all" },
    { id: "manager", label: "manager" },
    { id: "flash", label: "flash" },
    { id: "bridge", label: "bridge" },
  ];

  function isVisible(e: ActivityEntry): boolean {
    return matchesActivityFilter(e, filter);
  }

  $: visible = entries.filter(isVisible);
  $: tailId = visible.length > 0 ? visible[visible.length - 1].id : 0;
  $: totalHeight = visible.length * ROW_HEIGHT;
  $: visibleCount = Math.max(1, Math.ceil(viewportHeight / ROW_HEIGHT));
  $: startIndex = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
  $: endIndex = Math.min(visible.length, startIndex + visibleCount + OVERSCAN * 2);
  $: topSpacer = startIndex * ROW_HEIGHT;
  $: bottomSpacer = Math.max(0, (visible.length - endIndex) * ROW_HEIGHT);
  $: rendered = visible.slice(startIndex, endIndex);
  $: if (open && tailId !== lastVisibleTailId) {
    const shouldStick = followBottom;
    lastVisibleTailId = tailId;
    if (shouldStick) {
      void tick().then(() => {
        if (logEl) {
          logEl.scrollTop = logEl.scrollHeight;
          scrollTop = logEl.scrollTop;
        }
      });
    }
  }

  function onLogScroll() {
    if (!logEl) return;
    scrollTop = logEl.scrollTop;
    followBottom = logEl.scrollTop + logEl.clientHeight >= logEl.scrollHeight - 8;
  }

  function onResizeStart(event: MouseEvent) {
    startY = event.clientY;
    startHeight = height;

    const onMove = (moveEvent: MouseEvent) => {
      const next = startHeight + (startY - moveEvent.clientY);
      height = Math.max(140, Math.min(next, Math.floor(window.innerHeight * 0.75)));
    };

    const onUp = () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };

    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    event.preventDefault();
  }
</script>

<section class="wrap" class:open aria-label="Activity" style:height={open ? `${height}px` : undefined}>
  {#if open}
    <button
      class="resizeHandle"
      type="button"
      aria-label="Resize activity panel"
      onmousedown={onResizeStart}
    ></button>
  {/if}

  <button class="toggle" type="button" onclick={onToggle}>
    <span>Activity</span>
    <span class="toggleIcon" aria-hidden="true">
      {#if open}
        <svg viewBox="0 0 12 12" focusable="false" aria-hidden="true">
          <path d="M2.25 4.25 6 8l3.75-3.75" />
        </svg>
      {:else}
        <svg viewBox="0 0 12 12" focusable="false" aria-hidden="true">
          <path d="M4.25 2.25 8 6l-3.75 3.75" />
        </svg>
      {/if}
    </span>
  </button>

  {#if open}
    <div class="bar">
      <div class="filters">
        {#each filters as f}
          <button type="button" class:selected={filter === f.id} onclick={() => onFilter(f.id)}>
            {f.label}
          </button>
        {/each}
      </div>

      <div class="tools">
        <button type="button" onclick={onCopy}>copy</button>
        <button type="button" onclick={onClear}>clear</button>
      </div>
    </div>

    <div
      class="log"
      bind:this={logEl}
      bind:clientHeight={viewportHeight}
      role="log"
      aria-live="polite"
      onscroll={onLogScroll}
    >
      {#if visible.length === 0}
        <div class="empty">(no entries)</div>
      {:else}
        {#if topSpacer > 0}
          <div class="spacer" aria-hidden="true" style:height={`${topSpacer}px`}></div>
        {/if}
        {#each rendered as e (e.id)}
          <div class="line" data-level={e.level}>
            <span class="msg">{e.message}</span>
          </div>
        {/each}
        {#if bottomSpacer > 0}
          <div class="spacer" aria-hidden="true" style:height={`${bottomSpacer}px`}></div>
        {/if}
      {/if}
    </div>
  {/if}
</section>

<style>
  .wrap {
    position: relative;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .wrap.open {
    min-height: 160px;
    max-height: 60vh;
    overflow: hidden;
  }

  .resizeHandle {
    position: absolute;
    inset: 0 0 auto 0;
    height: 10px;
    border: 0;
    background: transparent;
    cursor: ns-resize;
    z-index: 2;
  }

  .toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    appearance: none;
    font: inherit;
    padding: 10px 12px;
    border: 0;
    background: transparent;
    color: var(--muted);
    text-align: left;
    font-weight: 900;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    cursor: pointer;
  }

  .toggleIcon {
    width: 14px;
    height: 14px;
    display: inline-grid;
    place-items: center;
    color: inherit;
    opacity: 0.9;
  }

  .toggleIcon svg {
    width: 12px;
    height: 12px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .wrap.open .toggle {
    border-bottom: 1px solid var(--border);
  }

  .bar {
    padding: 10px 12px;
    display: flex;
    justify-content: space-between;
    gap: 12px;
    align-items: center;
    border-bottom: 1px solid var(--border);
    flex: 0 0 auto;
  }

  .filters {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .filters button,
  .tools button {
    appearance: none;
    font: inherit;
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 11px;
  }

  .filters button.selected {
    color: var(--fg);
    border-color: var(--border-strong);
  }

  .tools {
    display: inline-flex;
    gap: 8px;
  }

  .log {
    flex: 1 1 auto;
    min-height: 0;
    overflow: auto;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    justify-content: flex-start;
    gap: 6px;
    scrollbar-gutter: stable;
  }

  .line {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
    font-family: var(--font-mono);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    min-height: 16px;
  }

  .line[data-level="error"] {
    color: var(--err);
  }

  .line[data-level="warn"] {
    color: var(--warn);
  }

  .line[data-level="ok"] {
    color: var(--ok);
  }

  .empty {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
    font-family: var(--font-mono);
  }

  .spacer {
    flex: 0 0 auto;
  }
</style>
