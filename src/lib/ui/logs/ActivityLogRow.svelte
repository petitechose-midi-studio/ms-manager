<script lang="ts">
  import type { UxRecorderEvent } from "$lib/api/types";
  import type { ActivityEntry } from "$lib/state/activity";
  import { presentActivityEntry } from "$lib/state/activity";

  export let entry: ActivityEntry;

  $: line = presentActivityEntry(entry);
  $: uxEvent = uxRecordedEvent(entry.details);

  function uxRecordedEvent(details: unknown): Extract<UxRecorderEvent, { type: "event_recorded" }> | null {
    if (!details || typeof details !== "object") return null;
    const value = details as Partial<UxRecorderEvent>;
    if (value.type !== "event_recorded") return null;
    return value as Extract<UxRecorderEvent, { type: "event_recorded" }>;
  }
</script>

<div class="row" data-marker-tone={line.markerTone} data-scope={entry.scope} title={line.text}>
  <span class="timestamp">{line.timestamp}</span>
  <span class="marker">{line.marker}</span>
  <span class="scope">{line.scopeLabel}</span>
  <span class="message">
    {#if line.sourceLabel}
      <span class="source">[{line.sourceLabel}]</span>
    {/if}
    {#if uxEvent}
      <span class="uxBody" data-ux-kind={uxEvent.presentation.kind}>
        <span class="uxKind">{uxEvent.presentation.kind}</span>
        <span class="uxAction">{uxEvent.presentation.action}</span>
        {#if uxEvent.presentation.control}
          <span class="uxControl">{uxEvent.presentation.control}</span>
        {/if}
        {#if uxEvent.presentation.value}
          <span class="uxValue">{uxEvent.presentation.value}</span>
        {/if}
        {#if uxEvent.presentation.target}
          <span class="uxTarget">{uxEvent.presentation.target}</span>
        {/if}
        {#if uxEvent.presentation.effect}
          <span class="uxEffect">{uxEvent.presentation.effect}</span>
        {/if}
        {#if uxEvent.presentation.state}
          <span class="uxState">{uxEvent.presentation.state}</span>
        {/if}
        {#if uxEvent.presentation.detail}
          <span class="uxDetail">{uxEvent.presentation.detail}</span>
        {/if}
      </span>
    {:else}
      <span class="body">{line.message}</span>
    {/if}
  </span>
</div>

<style>
  .row {
    min-width: 100%;
    width: max-content;
    min-height: 16px;
    padding: 0 4px;
    margin: 0 -4px;
    border-radius: 4px;
    display: grid;
    grid-template-columns: 12ch 6ch 8ch auto;
    gap: 12px;
    align-items: baseline;
    color: var(--value);
    font-family: var(--font-log);
    font-size: 12px;
    line-height: 16px;
    transition: background-color 120ms ease;
  }

  .row:hover {
    background: var(--log-row-hover);
  }

  .timestamp {
    color: var(--muted);
  }

  .marker,
  .scope {
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .marker {
    color: var(--muted);
  }

  .scope {
    color: color-mix(in srgb, var(--muted) 82%, var(--fg) 18%);
  }

  .message {
    white-space: pre;
    display: inline-flex;
    align-items: baseline;
    gap: 8px;
  }

  .source {
    color: var(--log-source);
    font-weight: 700;
  }

  .body {
    color: var(--value);
  }

  .uxBody {
    display: inline-flex;
    align-items: baseline;
    gap: 7px;
  }

  .uxKind,
  .uxAction,
  .uxControl,
  .uxValue,
  .uxTarget,
  .uxEffect,
  .uxState,
  .uxDetail {
    font-weight: 700;
  }

  .uxKind {
    color: var(--log-info);
  }

  .uxAction {
    color: var(--muted);
  }

  .uxControl {
    color: var(--fg);
  }

  .uxValue {
    color: var(--ok);
  }

  .uxTarget {
    color: var(--log-source);
  }

  .uxEffect {
    color: var(--log-tx);
  }

  .uxState {
    color: color-mix(in srgb, var(--muted) 65%, var(--fg) 35%);
  }

  .uxDetail {
    color: var(--muted);
  }

  .uxBody[data-ux-kind="button"] .uxKind {
    color: var(--warn);
  }

  .uxBody[data-ux-kind="encoder"] .uxKind {
    color: var(--log-rx);
  }

  .row[data-marker-tone="info"] .marker,
  .row[data-marker-tone="info"] .body {
    color: var(--log-info);
  }

  .row[data-marker-tone="ok"] .marker,
  .row[data-marker-tone="ok"] .body {
    color: var(--ok);
  }

  .row[data-marker-tone="warn"] .marker,
  .row[data-marker-tone="warn"] .body {
    color: var(--warn);
  }

  .row[data-marker-tone="error"] .marker,
  .row[data-marker-tone="error"] .body {
    color: var(--err);
  }

  .row[data-marker-tone="rx"] .marker,
  .row[data-scope="bridge"] .scope {
    color: var(--log-rx);
  }

  .row[data-marker-tone="tx"] .marker {
    color: var(--log-tx);
  }

  .row[data-marker-tone="system"] .marker,
  .row[data-marker-tone="system"] .body {
    color: var(--log-system);
  }
</style>
