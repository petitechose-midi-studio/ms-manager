<script lang="ts">
  import type { StepPresetReport } from "$lib/api/types";

  export let side: "local" | "remote";
  export let name: string;
  export let loading = false;
  export let action: "inspect" | "validate" | null = null;
  export let report: StepPresetReport | null = null;
  export let error: string | null = null;
  export let onInspect: () => void = () => {};
  export let onValidate: () => void = () => {};

  function statusLabel(value: StepPresetReport): string {
    if (value.status === "ok") return "OK";
    return value.status.replaceAll("_", " ");
  }

  function contextLabel(value: StepPresetReport): string {
    if (value.rootContext) return value.rootValues ? "root step" : "root graph";
    return "child step";
  }

  function graphLabel(value: StepPresetReport): string {
    const parts = [`${value.stepNodeCount} node${value.stepNodeCount === 1 ? "" : "s"}`];
    if (value.sequenceCount) {
      parts.push(`${value.sequenceCount} micro-seq${value.sequenceCount === 1 ? "" : "s"}`);
    }
    if (value.cycleSetCount) {
      parts.push(`${value.cycleSetCount} cycle${value.cycleSetCount === 1 ? "" : "s"}`);
    }
    return parts.join(" · ");
  }
</script>

<section class="presetSummary" class:mutedSummary={side === "remote"} aria-live="polite">
  <div class="presetSummaryHeader">
    <div>
      <div class="presetTitle">{side === "local" ? "Step preset" : "Step preset on controller"}</div>
      <div class="presetPath">{name}</div>
    </div>
    {#if side === "local"}
      <div class="presetActions">
        <button class="smallButton" type="button" disabled={loading} onclick={onInspect}>Inspect</button>
        <button class="smallButton" type="button" disabled={loading} onclick={onValidate}>Validate</button>
      </div>
    {/if}
  </div>

  {#if side === "remote"}
    <div class="presetLine">Download it to PC storage to inspect and validate the payload.</div>
  {:else if loading}
    <div class="presetLine">{action === "validate" ? "Validating" : "Inspecting"}…</div>
  {:else if error}
    <div class="presetLine errorText">{error}</div>
  {:else if report}
    <div class="presetReport">
      <span class="presetStatus" class:ok={report.status === "ok"}>{statusLabel(report)}</span>
      <span>{contextLabel(report)}</span>
      <span>{graphLabel(report)}</span>
      {#if report.flags.rootValues}
        <span>root values</span>
      {/if}
      {#if report.flags.graphPayload}
        <span>graph payload</span>
      {/if}
    </div>
  {:else}
    <div class="presetLine">Select a local Step Preset to inspect it.</div>
  {/if}
</section>

<style>
  .presetSummary {
    display: grid;
    gap: var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--control-radius);
    padding: var(--space-3);
    background: color-mix(in srgb, var(--bg) 26%, transparent);
  }

  .presetSummaryHeader {
    display: flex;
    align-items: start;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .presetTitle {
    color: var(--fg);
    font-size: 12px;
    line-height: 15px;
    font-weight: 800;
  }

  .presetPath,
  .presetLine {
    color: var(--muted);
    font-size: 11px;
    line-height: 14px;
    overflow-wrap: anywhere;
  }

  .presetLine.errorText {
    color: var(--err);
  }

  .presetActions {
    display: flex;
    gap: var(--space-2);
    flex: 0 0 auto;
  }

  .smallButton {
    appearance: none;
    min-height: 24px;
    border: 1px solid var(--border);
    border-radius: var(--control-radius);
    background: transparent;
    color: var(--fg);
    cursor: pointer;
    font: inherit;
    font-size: 11px;
    line-height: 14px;
    font-weight: 800;
    padding: 4px 8px;
  }

  .smallButton:hover:not(:disabled) {
    background: color-mix(in srgb, var(--value) 12%, transparent);
  }

  .smallButton:disabled {
    color: var(--muted);
    cursor: not-allowed;
  }

  .presetReport {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    color: var(--muted);
    font-size: 11px;
    line-height: 14px;
  }

  .presetReport span {
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 2px 6px;
    background: color-mix(in srgb, var(--bg) 24%, transparent);
  }

  .presetStatus.ok {
    color: var(--ok);
    border-color: color-mix(in srgb, var(--ok) 35%, var(--border));
  }

  .mutedSummary {
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
  }
</style>
