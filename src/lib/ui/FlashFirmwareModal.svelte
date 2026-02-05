<script lang="ts">
  import type { InstallState } from "$lib/api/types";

  export let open: boolean;
  export let targetProfile: string | null;
  export let hostInstalled: boolean;
  export let installed: InstallState | null;
  export let controllerConnected: boolean;
  export let flashing: boolean;
  export let progress: number | null;
  export let ack: boolean;

  export let onAck: (v: boolean) => void;
  export let onCancel: () => void;
  export let onConfirm: () => void;

  $: firmwareSource = installed ? `${installed.channel} / ${installed.tag}` : "(not installed)";
  $: installedProfile = installed ? installed.profile : null;
  $: pct = progress == null ? null : Math.max(0, Math.min(100, Math.round(progress)));
  $: canConfirm = hostInstalled && !!targetProfile && ack && !flashing;
</script>

{#if open}
  <div class="backdrop" role="presentation" onclick={onCancel}></div>

  <div class="modal" role="dialog" aria-modal="true" aria-label="Flash firmware">
    <div class="head">
      <div class="title">FLASH FIRMWARE</div>
      <button class="x" type="button" onclick={onCancel} aria-label="Close" disabled={flashing}>
        X
      </button>
    </div>

    <div class="body">
      <div class="kv">
        <div class="k">Target profile</div>
        <div class="v">{targetProfile ?? "-"}</div>
        <div class="k">Firmware source</div>
        <div class="v">
          {firmwareSource}
          {#if installedProfile}
            <span class="muted">(installed: {installedProfile})</span>
          {/if}
        </div>
        <div class="k">Controller</div>
        <div class="v">{controllerConnected ? "connected" : "not detected (will wait up to 60s)"}</div>
      </div>

      <label class="ack">
        <input type="checkbox" bind:checked={ack} onchange={() => onAck(ack)} />
        <span>I understand this overwrites controller firmware</span>
      </label>

      {#if !hostInstalled}
        <div class="note">Install the host bundle first (so firmware + loader are available).</div>
      {:else if installed && targetProfile && installed.profile !== targetProfile}
        <div class="note">
          The target profile firmware may not be installed yet. If flashing fails, run
          <span class="mono">Install / Update</span> for the target profile first.
        </div>
      {/if}
    </div>

    <div class="foot">
      <button class="btn" type="button" onclick={onCancel} disabled={flashing}>Cancel</button>
      <button
        class="btn primary"
        type="button"
        disabled={!canConfirm}
        onclick={onConfirm}
        data-flashing={flashing}
      >
        <span
          class="fill"
          class:indeterminate={flashing && pct == null}
          style={pct == null ? undefined : `width:${pct}%`}
        ></span>
        <span class="label">
          {flashing ? (pct == null ? "Flashing…" : `Flashing ${pct}%`) : "Flash firmware"}
        </span>
      </button>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    z-index: 2000;
  }

  .modal {
    position: fixed;
    z-index: 2001;
    left: 50%;
    top: 18%;
    transform: translateX(-50%);
    width: min(560px, calc(100vw - 24px));
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--panel);
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--border);
  }

  .title {
    font-weight: 900;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .x {
    appearance: none;
    font: inherit;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    border-radius: 6px;
    width: 34px;
    height: 34px;
    display: grid;
    place-items: center;
    padding: 0;
    cursor: pointer;
  }

  .body {
    padding: 12px;
    display: grid;
    gap: 12px;
  }

  .kv {
    display: grid;
    grid-template-columns: 150px 1fr;
    gap: 8px 10px;
    align-items: baseline;
  }

  .k {
    color: var(--muted);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-size: 11px;
    font-family: var(--font-mono);
  }

  .k::after {
    content: ":";
    opacity: 0.6;
  }

  .v {
    overflow-wrap: anywhere;
    font-family: var(--font-mono);
    color: var(--fg);
    opacity: 0.86;
    font-weight: 600;
  }

  .ack {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--value);
  }

  .note {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
    border: 1px dashed var(--border);
    border-radius: 6px;
    padding: 10px 12px;
  }

  .mono {
    font-family: var(--font-mono);
  }

  .foot {
    padding: 10px 12px;
    border-top: 1px solid var(--border);
    display: flex;
    gap: 10px;
    justify-content: flex-end;
  }

  .btn {
    appearance: none;
    font: inherit;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    border-radius: 6px;
    padding: 8px 10px;
    cursor: pointer;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 12px;
  }

  .btn.primary {
    position: relative;
    overflow: hidden;
    background: var(--value);
    color: var(--bg);
    border-color: var(--value);
  }

  .btn.primary .fill {
    position: absolute;
    left: 0;
    top: 0;
    height: 100%;
    width: 0;
    background: var(--ok);
    opacity: 0.62;
    transition: width 120ms linear;
    pointer-events: none;
  }

  .btn.primary .fill.indeterminate {
    width: 100%;
    animation: flash-pulse 1.2s ease-in-out infinite;
  }

  @keyframes flash-pulse {
    0%,
    100% {
      opacity: 0.10;
    }
    50% {
      opacity: 0.28;
    }
  }

  .btn.primary .label {
    position: relative;
    z-index: 1;
  }

  .btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .btn.primary[data-flashing="true"] {
    opacity: 1;
    cursor: progress;
  }

  @media (max-width: 560px) {
    .modal {
      top: 10%;
    }
    .kv {
      grid-template-columns: 1fr;
    }
  }
</style>
