<script lang="ts" context="module">
  export type MenuItem = {
    id: string;
    label: string;
    disabled?: boolean;
  };
</script>

<script lang="ts">
  export let open: boolean;
  export let x: number;
  export let y: number;
  export let items: MenuItem[];
  export let onSelect: (id: string) => void;
  export let onClose: () => void;

  function clickItem(item: MenuItem) {
    if (item.disabled) return;
    onSelect(item.id);
  }
</script>

{#if open}
  <div class="backdrop" onclick={onClose} aria-hidden="true"></div>

  <div class="menu" style={`left:${x}px;top:${y}px;`} role="menu">
    {#each items as item}
      <button
        type="button"
        role="menuitem"
        class:disabled={!!item.disabled}
        disabled={!!item.disabled}
        onclick={() => clickItem(item)}
      >
        {item.label}
      </button>
    {/each}
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: transparent;
    z-index: 1000;
  }

  .menu {
    position: fixed;
    z-index: 1001;
    min-width: 180px;
    border: 1px solid var(--border-strong);
    background: var(--panel);
    border-radius: 6px;
    padding: 6px;
    box-shadow: 0 10px 24px rgba(0, 0, 0, 0.35);
  }

  button {
    width: 100%;
    text-align: left;
    appearance: none;
    font: inherit;
    padding: 8px 10px;
    border-radius: 6px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--fg);
    cursor: pointer;
  }

  button:hover:not(:disabled) {
    border-color: var(--border);
  }

  button:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
