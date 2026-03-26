<script lang="ts">
  export let selectedFirmware = "-";
  export let needsDownload = false;
  export let canFlash = false;
  export let busy = false;
  export let flashing = false;
  export let errorMessage: string | null = null;
  export let onFlash: () => void;
</script>

<div class="card">
  <div class="cardTitle">Actions</div>
  <div class="field">
    <div class="label">Selected Firmware</div>
    <div class="value">{selectedFirmware}</div>
  </div>
  {#if needsDownload}
    <div class="muted">Download the selected installed release before flashing.</div>
  {/if}
  <div class="actions">
    <button class="btn primary" type="button" disabled={busy || !canFlash} onclick={onFlash}>
      {flashing ? "Flashing..." : "Flash"}
    </button>
  </div>
  {#if errorMessage}
    <div class="err">{errorMessage}</div>
  {/if}
</div>

<style>
  .card {
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 12px;
    display: grid;
    gap: 12px;
    background: rgba(0, 0, 0, 0.03);
  }

  :global(:root[data-theme="light"]) .card {
    background: rgba(0, 0, 0, 0.015);
  }

  .cardTitle {
    color: var(--muted);
    font-family: var(--font-sans);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 12px;
    line-height: 16px;
  }

  .field {
    display: grid;
    gap: 6px;
  }

  .label {
    color: var(--muted);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 11px;
    line-height: 14px;
    font-family: var(--font-sans);
  }

  .value {
    overflow-wrap: anywhere;
    color: var(--fg);
    opacity: 0.64;
    font-weight: 400;
    font-family: var(--font-sans);
    font-size: 13px;
    line-height: 18px;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }

  .btn {
    appearance: none;
    font: inherit;
    padding: 8px 10px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 12px;
    line-height: 14px;
  }

  .btn.primary {
    background: var(--value);
    color: var(--bg);
    border-color: var(--value);
  }

  .btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .muted {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
  }

  .err {
    color: var(--err);
    font-size: 12px;
    line-height: 16px;
    border: 1px solid var(--err);
    border-radius: 6px;
    padding: 10px 12px;
  }
</style>
