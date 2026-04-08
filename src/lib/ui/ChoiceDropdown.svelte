<script lang="ts" context="module">
  export type ChoiceOption = {
    value: string;
    label: string;
    icon?: "controller" | "bitwig";
  };
</script>

<script lang="ts">
  import BitwigIcon from "$lib/ui/icons/BitwigIcon.svelte";
  import ControllerIcon from "$lib/ui/icons/ControllerIcon.svelte";

  export let label = "";
  export let value: string | null | undefined;
  export let disabled = false;
  export let options: ChoiceOption[] = [];
  export let placeholder = "";
  export let onChange: (next: string) => void;

  let open = false;
  let rootEl: HTMLDivElement | null = null;
  let selectedOption: ChoiceOption | undefined;

  $: selectedOption = options.find((option) => option.value === value);

  function toggleOpen() {
    if (disabled) return;
    open = !open;
  }

  function close() {
    open = false;
  }

  function choose(next: string) {
    close();
    if (next !== value) {
      onChange(next);
    }
  }

  function onWindowClick(event: MouseEvent) {
    if (!open || !rootEl) return;
    const target = event.target;
    if (target instanceof Node && !rootEl.contains(target)) {
      close();
    }
  }

  function onWindowKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      close();
    }
  }
</script>

<svelte:window onclick={onWindowClick} onkeydown={onWindowKeydown} />

<div class="wrap" bind:this={rootEl}>
  {#if label}
    <span class="label">{label}</span>
  {/if}
  <div class="dropdown">
    <button
      type="button"
      class="trigger"
      disabled={disabled}
      aria-haspopup="listbox"
      aria-expanded={open}
      onclick={toggleOpen}
    >
      {#if selectedOption?.icon}
        <span class="currentIcon" aria-hidden="true">
          {#if selectedOption.icon === "bitwig"}
            <BitwigIcon size={14} />
          {:else}
            <ControllerIcon size={14} />
          {/if}
        </span>
      {/if}
      <span class:placeholder={!selectedOption && !!placeholder} class="currentLabel">
        {selectedOption?.label ?? placeholder}
      </span>
      <span class="chevron" aria-hidden="true">
        <svg viewBox="0 0 12 12" focusable="false">
          <path d="M2.25 4.25 6 8l3.75-3.75" />
        </svg>
      </span>
    </button>

    {#if open}
      <div class="menu" role="listbox" aria-label={label || "choice"}>
        {#each options as option}
          <button
            type="button"
            class="item"
            class:selected={option.value === value}
            onclick={() => choose(option.value)}
          >
            {#if option.icon}
              <span class="itemIcon" aria-hidden="true">
                {#if option.icon === "bitwig"}
                  <BitwigIcon size={14} />
                {:else}
                  <ControllerIcon size={14} />
                {/if}
              </span>
            {/if}
            <span class="itemLabel">{option.label}</span>
          </button>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .wrap {
    display: grid;
    gap: var(--space-2);
    min-width: 220px;
  }

  .label {
    color: var(--muted);
    font-family: var(--font-sans);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 11px;
    line-height: 14px;
  }

  .dropdown {
    position: relative;
  }

  .trigger,
  .item {
    appearance: none;
    width: 100%;
    background: var(--panel);
    color: var(--fg);
    border-radius: var(--control-radius);
    font: inherit;
    font-family: var(--font-sans);
    cursor: pointer;
  }

  .trigger {
    border: 1px solid var(--border);
  }

  .trigger {
    min-height: var(--control-height);
    padding: 8px var(--control-padding-x);
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .trigger:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .currentIcon,
  .itemIcon,
  .chevron {
    width: 14px;
    height: 14px;
    display: inline-grid;
    place-items: center;
    flex: 0 0 auto;
  }

  .currentLabel,
  .itemLabel {
    min-width: 0;
    font-weight: 600;
    letter-spacing: 0.02em;
    text-transform: uppercase;
    font-size: 12px;
    line-height: 14px;
  }

  .currentLabel {
    flex: 1 1 auto;
    text-align: left;
  }

  .currentLabel.placeholder {
    color: var(--muted);
  }

  .chevron {
    color: var(--muted);
  }

  .chevron svg {
    width: 12px;
    height: 12px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .menu {
    position: absolute;
    inset: calc(100% + 6px) 0 auto 0;
    z-index: 20;
    display: grid;
    gap: var(--space-1);
    padding: var(--space-1);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-card);
    background: var(--panel);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.24);
  }

  .item {
    border: 1px solid transparent;
    min-height: calc(var(--control-height) - 2px);
    padding: 8px var(--control-padding-x);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    text-align: left;
    color: color-mix(in srgb, var(--fg) 68%, transparent);
    transition:
      border-color 180ms ease,
      color 180ms ease,
      background-color 180ms ease,
      box-shadow 180ms ease;
  }

  .item.selected {
    border-color: color-mix(in srgb, var(--border-strong) 68%, transparent);
    background: color-mix(in srgb, var(--panel-elevated) 88%, var(--panel));
    color: color-mix(in srgb, var(--value) 86%, white);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--value) 12%, transparent);
    transition-duration: 90ms;
  }

  .item:hover {
    border-color: var(--border-strong);
    color: color-mix(in srgb, var(--value) 78%, white);
    transition-duration: 80ms;
  }
</style>
