<script lang="ts">
  export let open: boolean;
  export let currentRoot: string | null;
  export let nextRoot: string;
  export let relocating: boolean;
  export let ack: boolean;

  export let onRoot: (v: string) => void;
  export let onBrowse: () => void;
  export let onAck: (v: boolean) => void;
  export let onCancel: () => void;
  export let onConfirm: () => void;

  $: canConfirm = !!nextRoot.trim() && ack && !relocating;
</script>

{#if open}
  <div class="backdrop" role="presentation" onclick={onCancel}></div>

  <div class="modal" role="dialog" aria-modal="true" aria-label="Relocate installation folder">
    <div class="head">
      <div class="title">INSTALLATION FOLDER</div>
      <button class="x" type="button" onclick={onCancel} aria-label="Close" disabled={relocating}>X</button>
    </div>

    <div class="body">
      <div class="kv">
        <div class="k">Current</div>
        <div class="v">{currentRoot ?? "-"}</div>
        <div class="k">New location</div>
        <div class="v">
          <div class="inputRow">
            <input
              class="input"
              type="text"
              value={nextRoot}
              oninput={(e) => onRoot((e.currentTarget as HTMLInputElement).value)}
              placeholder="Absolute path (e.g. D:\\MIDI Studio)"
              disabled={relocating}
              spellcheck="false"
            />
            <button class="pick" type="button" onclick={onBrowse} disabled={relocating} aria-label="Browse">
              <svg viewBox="0 0 24 24" width="16" height="16" aria-hidden="true">
                <path
                  fill="currentColor"
                  d="M10 4h4l2 2h4c1.1 0 2 .9 2 2v10c0 1.1-.9 2-2 2H4c-1.1 0-2-.9-2-2V6c0-1.1.9-2 2-2h6zm10 14V8H4v10h16z"
                />
              </svg>
            </button>
          </div>
        </div>
      </div>

      <div class="note">
        This will stop/uninstall background services (e.g. Open Control Bridge), move the installation folder,
        then reinstall services pointing at the new location.
      </div>

      <label class="ack">
        <input type="checkbox" bind:checked={ack} onchange={() => onAck(ack)} disabled={relocating} />
        <span>I understand this may require admin privileges and closing running hosts (Bitwig, etc.)</span>
      </label>
    </div>

    <div class="foot">
      <button class="btn" type="button" onclick={onCancel} disabled={relocating}>Cancel</button>
      <button class="btn primary" type="button" onclick={onConfirm} disabled={!canConfirm} data-busy={relocating}>
        {relocating ? "Moving…" : "Move folder"}
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
    top: 14%;
    transform: translateX(-50%);
    width: min(640px, calc(100vw - 24px));
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
    grid-template-columns: 140px 1fr;
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

  .input {
    width: 100%;
    font: inherit;
    font-family: var(--font-mono);
    font-weight: 600;
    color: var(--fg);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 7px 10px;
    outline: none;
  }

  .inputRow {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .pick {
    flex: 0 0 auto;
    width: 38px;
    height: 38px;
    display: grid;
    place-items: center;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }

  .pick:hover:not(:disabled) {
    border-color: var(--border-strong);
    color: var(--value);
  }

  .pick:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .input:focus {
    border-color: var(--border-strong);
  }

  .note {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
    border: 1px dashed var(--border);
    border-radius: 6px;
    padding: 10px 12px;
  }

  .ack {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--value);
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
    background: var(--value);
    color: var(--bg);
    border-color: var(--value);
  }

  .btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .btn.primary[data-busy="true"] {
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
