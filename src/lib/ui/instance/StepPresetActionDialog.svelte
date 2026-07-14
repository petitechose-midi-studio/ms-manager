<script lang="ts">
  import { browser } from "$app/environment";
  import { tick } from "svelte";
  import type { StepPresetReport } from "$lib/api/types";
  import RenameIcon from "$lib/ui/icons/RenameIcon.svelte";
  import TrashIcon from "$lib/ui/icons/TrashIcon.svelte";

  export let open = false;
  export let action: "rename" | "delete" = "rename";
  export let report: StepPresetReport | null = null;
  export let value = "";
  export let busy = false;
  export let error: string | null = null;
  export let onValue: (value: string) => void = () => {};
  export let onCancel: () => void = () => {};
  export let onConfirm: () => void = () => {};

  const MAX_NAME_BYTES = 31;
  let dialogElement: HTMLElement;
  let previousFocus: HTMLElement | null = null;
  let lastOpen = false;

  function utf8Bytes(text: string): number {
    return new TextEncoder().encode(text).length;
  }

  function semanticNameError(text: string): string | null {
    const trimmed = text.trim();
    if (!trimmed) return "Enter a name.";
    if (trimmed !== text) return "Remove leading or trailing spaces.";
    if (utf8Bytes(text) > MAX_NAME_BYTES) return `Use at most ${MAX_NAME_BYTES} UTF-8 bytes.`;
    for (const character of text) {
      const code = character.codePointAt(0) ?? 0;
      if (code < 32 || (code >= 127 && code <= 159)) {
        return "Control characters are not allowed.";
      }
    }
    return null;
  }

  function scaleLabel(value: StepPresetReport): string {
    const roots = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"];
    const types = [
      "chromatic", "major", "natural minor", "harmonic minor", "melodic minor",
      "dorian", "phrygian", "lydian", "mixolydian", "locrian",
      "major pentatonic", "minor pentatonic", "blues", "whole tone",
    ];
    const source = `${roots[value.sourceScale.root] ?? "?"} ${types[value.sourceScale.type] ?? "unknown scale"}`;
    if (value.scalePolicy === "mixed") return `Mixed pitch · relative nodes use ${source}`;
    if (value.scalePolicy === "scale_relative") return `Scale-relative · ${source}`;
    return "Chromatic pitch";
  }

  async function syncOpenState(nextOpen: boolean): Promise<void> {
    if (!browser || nextOpen === lastOpen) return;
    lastOpen = nextOpen;
    if (nextOpen) {
      previousFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      await tick();
      const preferred = dialogElement?.querySelector<HTMLElement>(
        action === "rename" ? "input:not(:disabled)" : "button.primary:not(:disabled)",
      );
      preferred?.focus();
      return;
    }
    await tick();
    if (previousFocus?.isConnected) previousFocus.focus();
    previousFocus = null;
  }

  function onDialogKeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && !busy) {
      event.preventDefault();
      onCancel();
      return;
    }
    if (event.key === "Enter" && canConfirm) {
      event.preventDefault();
      onConfirm();
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = Array.from(
      dialogElement.querySelectorAll<HTMLElement>(
        'input:not(:disabled), button:not(:disabled), [tabindex]:not([tabindex="-1"])',
      ),
    );
    if (!focusable.length) {
      event.preventDefault();
      dialogElement.focus();
      return;
    }
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  $: nameBytes = utf8Bytes(value);
  $: nameError = semanticNameError(value);
  $: unchanged = value === (report?.semanticName ?? "");
  $: nameHelp = nameError ?? (unchanged ? "Choose a different name." : `${nameBytes}/${MAX_NAME_BYTES} UTF-8 bytes`);
  $: canConfirm = !!report && !busy &&
    (action === "delete" || (!nameError && !unchanged));
  $: void syncOpenState(open);
</script>

{#if open && report}
  <div class="backdrop" role="presentation" onclick={() => !busy && onCancel()}></div>
  <div
    bind:this={dialogElement}
    class="dialog"
    class:destructive={action === "delete"}
    role="dialog"
    aria-modal="true"
    aria-labelledby="step-preset-dialog-title"
    tabindex="-1"
    onkeydown={onDialogKeydown}
  >
    <header>
      <span class="titleIcon" aria-hidden="true">
        {#if action === "rename"}<RenameIcon size={16} />{:else}<TrashIcon size={16} />{/if}
      </span>
      <div>
        <h2 id="step-preset-dialog-title">
          {action === "rename" ? "Rename Step Preset" : "Delete Step Preset"}
        </h2>
        <p>{action === "rename" ? "The file and technical identity stay unchanged." : "This removes the exact asset shown below."}</p>
      </div>
    </header>

    <div class="identity" aria-label="Step Preset identity">
      <div class="identityName">{report.semanticName}</div>
      <code>{report.technicalId}</code>
      <div class="pitchPolicy">{scaleLabel(report)}</div>
    </div>

    {#if action === "rename"}
      <label>
        <span>Name</span>
        <input
          type="text"
          value={value}
          oninput={(event) => onValue((event.currentTarget as HTMLInputElement).value)}
          disabled={busy}
          autocomplete="off"
          spellcheck="false"
          aria-invalid={!!nameError}
          aria-describedby="step-preset-name-help"
        />
        <small id="step-preset-name-help" class:limitError={!!nameError}>
          {nameHelp}
        </small>
      </label>
    {:else}
      <div class="warning">
        The preset content cannot be recovered from MIDI Studio after deletion.
      </div>
    {/if}

    {#if error}
      <div class="error" role="alert">{error}</div>
    {/if}

    <footer>
      <button type="button" class="secondary" onclick={onCancel} disabled={busy}>Cancel</button>
      <button
        type="button"
        class:danger={action === "delete"}
        class="primary"
        onclick={onConfirm}
        disabled={!canConfirm}
      >
        {busy ? (action === "rename" ? "Renaming…" : "Deleting…") : (action === "rename" ? "Rename" : "Delete")}
      </button>
    </footer>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 2200;
    background: rgba(0, 0, 0, 0.42);
  }

  .dialog {
    position: fixed;
    z-index: 2201;
    left: 50%;
    top: 18%;
    transform: translateX(-50%);
    width: min(440px, calc(100vw - 24px));
    border: 1px solid var(--border-strong);
    border-radius: var(--control-radius);
    background: var(--panel);
    box-shadow: 0 18px 56px rgba(0, 0, 0, 0.35);
  }

  header {
    display: flex;
    gap: var(--space-3);
    align-items: flex-start;
    padding: var(--space-4);
    border-bottom: 1px solid var(--border);
  }

  .titleIcon {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    flex: 0 0 auto;
    border: 1px solid color-mix(in srgb, var(--value) 42%, var(--border));
    border-radius: 50%;
    color: var(--value);
  }

  .destructive .titleIcon {
    border-color: color-mix(in srgb, var(--err) 48%, var(--border));
    color: var(--err);
  }

  h2,
  p {
    margin: 0;
  }

  h2 {
    color: var(--fg);
    font-size: 14px;
    line-height: 18px;
  }

  p {
    margin-top: 3px;
    color: var(--muted);
    font-size: 11px;
    line-height: 15px;
  }

  .identity,
  label,
  .warning,
  .error {
    margin: var(--space-3) var(--space-4) 0;
  }

  .identity {
    display: grid;
    gap: 3px;
    padding: 9px 10px;
    border: 1px solid var(--border);
    border-radius: var(--control-radius);
    background: color-mix(in srgb, var(--bg) 30%, transparent);
  }

  .identityName {
    color: var(--fg);
    font-weight: 800;
  }

  code {
    color: var(--muted);
    font-size: 10px;
    overflow-wrap: anywhere;
  }

  .pitchPolicy {
    color: var(--muted);
    font-size: 10px;
    line-height: 13px;
  }

  label {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 5px 8px;
    color: var(--muted);
    font-size: 11px;
    font-weight: 700;
  }

  input {
    grid-column: 1 / -1;
    width: 100%;
    box-sizing: border-box;
    border: 1px solid var(--border);
    border-radius: var(--control-radius);
    background: var(--bg);
    color: var(--fg);
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    padding: 8px 9px;
    outline: none;
  }

  input:focus {
    border-color: var(--value);
  }

  small {
    grid-column: 2;
    color: var(--muted);
    font-weight: 500;
  }

  .limitError {
    color: var(--err);
  }

  .warning {
    border-left: 3px solid var(--err);
    padding: 7px 9px;
    color: var(--muted);
    background: color-mix(in srgb, var(--err) 7%, transparent);
    font-size: 11px;
    line-height: 15px;
  }

  .error {
    color: var(--err);
    font-size: 11px;
    line-height: 15px;
    overflow-wrap: anywhere;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    margin-top: var(--space-4);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--border);
  }

  button {
    appearance: none;
    min-height: 30px;
    border: 1px solid var(--border);
    border-radius: var(--control-radius);
    padding: 6px 11px;
    font: inherit;
    font-size: 11px;
    font-weight: 800;
    cursor: pointer;
  }

  .secondary {
    background: transparent;
    color: var(--muted);
  }

  .primary {
    border-color: var(--value);
    background: var(--value);
    color: var(--bg);
  }

  .primary.danger {
    border-color: var(--err);
    background: var(--err);
    color: white;
  }

  button:disabled {
    opacity: 0.52;
    cursor: not-allowed;
  }
</style>
