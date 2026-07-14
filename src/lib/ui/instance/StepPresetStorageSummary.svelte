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

  function compatibilityLabel(value: StepPresetReport): string {
    switch (value.compatibility) {
      case "ready": return "Ready";
      case "ready_mixed": return "Ready · mixed pitch";
      case "warning_legacy_defaulted": return "Legacy · chromatic default";
      case "unsupported_version": return "Unsupported version";
      case "blocked_invalid": return "Blocked · invalid";
      default: return "Compatibility unknown";
    }
  }

  function scaleLabel(value: StepPresetReport): string {
    const roots = ["C", "C♯", "D", "D♯", "E", "F", "F♯", "G", "G♯", "A", "A♯", "B"];
    const types = [
      "chromatic", "major", "natural minor", "harmonic minor", "melodic minor",
      "dorian", "phrygian", "lydian", "mixolydian", "locrian",
      "major pentatonic", "minor pentatonic", "blues", "whole tone",
    ];
    const source = `${roots[value.sourceScale.root] ?? "?"} ${types[value.sourceScale.type] ?? "unknown scale"}`;
    if (value.scalePolicy === "mixed") {
      const base = value.defaultScalePolicy === "scale_relative" ? "scale-relative" : "chromatic";
      return `mixed pitch · ${base} default · relative ${source}`;
    }
    if (value.scalePolicy === "scale_relative") {
      return `scale-relative · ${source}`;
    }
    return value.scalePolicy === "chromatic" ? "chromatic" : "pitch policy unknown";
  }
</script>

<section class="presetSummary" aria-live="polite">
  <div class="presetSummaryHeader">
    <div>
      <div class="presetTitle">{side === "local" ? "Step preset" : "Step preset on controller"}</div>
      <div class="presetPath">{report?.semanticName || name}</div>
      {#if report?.technicalId}
        <code class="technicalId">{report.technicalId}</code>
      {/if}
    </div>
    <div class="presetActions">
      <button class="smallButton" type="button" disabled={loading} onclick={onInspect}>Inspect</button>
      <button class="smallButton" type="button" disabled={loading} onclick={onValidate}>Validate</button>
    </div>
  </div>

  {#if loading}
    <div class="presetLine">{action === "validate" ? "Validating" : "Inspecting"}…</div>
  {:else if error}
    <div class="presetLine errorText">{error}</div>
  {:else if report}
    <div class="presetReport">
      <span class="presetStatus" class:ok={report.status === "ok"}>{statusLabel(report)}</span>
      <span
        class:warning={report.compatibility === "warning_legacy_defaulted" || report.compatibility === "ready_mixed"}
        class:errorBadge={report.compatibility === "unsupported_version" || report.compatibility === "blocked_invalid"}
      >{compatibilityLabel(report)}</span>
      <span>{contextLabel(report)}</span>
      <span>{graphLabel(report)}</span>
      <span>{scaleLabel(report)}</span>
      {#if report.flags.rootValues}
        <span>root values</span>
      {/if}
      {#if report.flags.graphPayload}
        <span>graph payload</span>
      {/if}
    </div>
  {:else}
    <div class="presetLine">Select a Step Preset to inspect it.</div>
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

  .technicalId {
    display: block;
    margin-top: 2px;
    color: var(--muted);
    font-size: 9px;
    line-height: 12px;
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

  .presetReport .warning {
    color: var(--warn);
    border-color: color-mix(in srgb, var(--warn) 38%, var(--border));
  }

  .presetReport .errorBadge {
    color: var(--err);
    border-color: color-mix(in srgb, var(--err) 38%, var(--border));
  }

</style>
