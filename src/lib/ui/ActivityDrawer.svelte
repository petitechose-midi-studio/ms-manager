<script lang="ts">
  import type { ActivityEntry, ActivityFilter, ActivityScope } from "$lib/state/activity";

  export let open: boolean;
  export let filter: ActivityFilter;
  export let entries: ActivityEntry[];
  export let onToggle: () => void;
  export let onFilter: (f: ActivityFilter) => void;
  export let onCopy: () => void;
  export let onClear: () => void;

  const filters: { id: ActivityFilter; label: string }[] = [
    { id: "all", label: "all" },
    { id: "install", label: "install" },
    { id: "flash", label: "flash" },
    { id: "device", label: "device" },
    { id: "net", label: "net" },
    { id: "fs", label: "fs" },
    { id: "ui", label: "ui" },
  ];

  function isVisible(e: ActivityEntry): boolean {
    if (filter === "all") return true;
    return e.scope === (filter as ActivityScope);
  }

  $: visible = entries.filter(isVisible);
</script>

<section class="wrap" class:open aria-label="Activity">
  <button class="toggle" type="button" onclick={onToggle}>
    Activity {open ? "v" : ">"}
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

    <div class="log" role="log" aria-live="polite">
      {#if visible.length === 0}
        <div class="empty">(no entries)</div>
      {:else}
        {#each visible as e (e.ts + e.scope + e.message)}
          <div class="line" data-level={e.level}>
            <span class="msg">{e.message}</span>
          </div>
        {/each}
      {/if}
    </div>
  {/if}
</section>

<style>
  .wrap {
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--panel);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .wrap.open {
    height: 220px;
    min-height: 160px;
    max-height: 60vh;
    resize: vertical;
    overflow: hidden;
  }

  .toggle {
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
    display: grid;
    gap: 6px;
    scrollbar-gutter: stable;
  }

  .line {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
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
  }
</style>
