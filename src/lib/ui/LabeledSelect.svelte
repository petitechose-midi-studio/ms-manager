<script lang="ts" context="module">
  export type SelectOption = {
    value: string;
    label: string;
  };
</script>

<script lang="ts">
  export let label: string;
  export let value: string;
  export let disabled = false;
  export let options: SelectOption[];
  export let onChange: (next: string) => void;

  function handleChange(e: Event) {
    const el = e.currentTarget as HTMLSelectElement | null;
    onChange(el?.value ?? "");
  }
</script>

<label class="wrap">
  <span class="label">{label}</span>
  <select class="select" {disabled} value={value} onchange={handleChange} aria-label={label}>
    {#each options as o}
      <option value={o.value}>{o.label}</option>
    {/each}
  </select>
</label>

<style>
  .wrap {
    display: inline-flex;
    align-items: center;
    gap: 10px;
  }

  .label {
    color: var(--muted);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 11px;
    line-height: 14px;
  }

  .select {
    font: inherit;
    font-family: var(--font-mono);
    font-weight: 600;
    color: var(--fg);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 6px 10px;
    cursor: pointer;
  }

  .select option {
    background: var(--bg);
    color: var(--fg);
  }

  .select:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
