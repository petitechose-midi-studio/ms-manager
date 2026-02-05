<script lang="ts" context="module">
  export type ProfileOption = {
    id: string;
    label: string;
    icons?: ("controller" | "bitwig")[];
  };
</script>

<script lang="ts">
  import BitwigIcon from "$lib/ui/icons/BitwigIcon.svelte";
  import ControllerIcon from "$lib/ui/icons/ControllerIcon.svelte";

  export let value: string;
  export let options: ProfileOption[];
  export let disabled = false;
  export let onSelect: (profileId: string) => void;
  export let onContext: (profileId: string, x: number, y: number) => void;

  function handleContext(e: MouseEvent, id: string) {
    e.preventDefault();
    onContext(id, e.clientX, e.clientY);
  }
</script>

<div class="wrap" role="radiogroup" aria-label="Profile">
  <span class="label">Profile</span>
  <div class="buttons">
    {#each options as o}
      <button
        type="button"
        class:selected={value === o.id}
        disabled={disabled}
        onclick={() => onSelect(o.id)}
        oncontextmenu={(e) => handleContext(e, o.id)}
      >
        <span class="icons" aria-hidden="true">
          {#if o.icons && o.icons.length}
            {#each o.icons as ico}
              <span class="icon">
                {#if ico === "controller"}
                  <ControllerIcon size={14} />
                {:else if ico === "bitwig"}
                  <BitwigIcon size={14} />
                {:else}
                  <span class="fallback"></span>
                {/if}
              </span>
            {/each}
          {:else}
            <span class="icon"><span class="fallback"></span></span>
          {/if}
        </span>
        <span class="text">{o.label}</span>
      </button>
    {/each}
  </div>
</div>

<style>
  .wrap {
    display: grid;
    gap: 8px;
  }

  .label {
    color: var(--muted);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 11px;
    line-height: 14px;
  }

  .buttons {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }

  button {
    appearance: none;
    font: inherit;
    padding: 6px 9px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    user-select: none;
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  button.selected {
    color: var(--value);
    border-color: var(--border-strong);
  }

  button:hover:not(:disabled) {
    border-color: var(--border-strong);
    color: var(--value);
  }

  button:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .icons {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .icon {
    width: 16px;
    height: 16px;
    display: grid;
    place-items: center;
  }

  .fallback {
    width: 12px;
    height: 12px;
    border-radius: 2px;
    border: 1px solid var(--border);
  }

  .text {
    font-family: var(--font-sans);
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    font-size: 12px;
  }
</style>
