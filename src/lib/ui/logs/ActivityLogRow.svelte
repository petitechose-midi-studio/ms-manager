<script lang="ts">
  import type { ActivityEntry } from "$lib/state/activity";
  import { presentActivityEntry } from "$lib/state/activity";

  export let entry: ActivityEntry;

  $: line = presentActivityEntry(entry);
</script>

<div class="row" data-marker-tone={line.markerTone} data-scope={entry.scope} title={line.text}>
  <span class="timestamp">{line.timestamp}</span>
  <span class="marker">{line.marker}</span>
  <span class="scope">{line.scopeLabel}</span>
  <span class="message">
    {#if line.sourceLabel}
      <span class="source">[{line.sourceLabel}]</span>
    {/if}
    <span class="body">{line.message}</span>
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
