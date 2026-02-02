<script lang="ts">
  import { onMount } from "svelte";
  import type { ApiError, Channel, LatestManifestResponse } from "$lib/api/types";
  import { resolveLatestManifest, settingsGet, settingsSetChannel } from "$lib/api/client";

  let channel = $state<Channel>("stable");
  let loading = $state(false);
  let err = $state<ApiError | null>(null);
  let latest = $state<LatestManifestResponse | null>(null);

  onMount(() => {
    void (async () => {
      try {
        const s = await settingsGet();
        channel = s.channel;
      } catch (e) {
        err = e as ApiError;
      }
    })();
  });

  async function setChannel(next: Channel) {
    err = null;
    latest = null;
    try {
      const s = await settingsSetChannel(next);
      channel = s.channel;
    } catch (e) {
      err = e as ApiError;
    }
  }

  async function refreshLatest() {
    loading = true;
    err = null;
    latest = null;
    try {
      latest = await resolveLatestManifest(channel);
    } catch (e) {
      err = e as ApiError;
    } finally {
      loading = false;
    }
  }
</script>

<main class="page">
  <header class="hero">
    <div class="brand">
      <div class="mark">MS</div>
      <div class="titles">
        <h1>MIDI Studio Manager</h1>
        <p>Install / update bundles from the official distribution feed.</p>
      </div>
    </div>
  </header>

  <section class="panel">
    <div class="row">
      <div class="label">Channel</div>
      <div class="segmented" role="radiogroup" aria-label="Channel">
        <button
          type="button"
          class:selected={channel === "stable"}
          onclick={() => setChannel("stable")}
        >
          Stable
        </button>
        <button type="button" class:selected={channel === "beta"} onclick={() => setChannel("beta")}>
          Beta
        </button>
        <button
          type="button"
          class:selected={channel === "nightly"}
          onclick={() => setChannel("nightly")}
        >
          Nightly
        </button>
      </div>
    </div>

    <div class="actions">
      <button class="primary" type="button" disabled={loading} onclick={refreshLatest}>
        {#if loading}
          Checking latest…
        {:else}
          Install / Update latest
        {/if}
      </button>
      <div class="hint">Updates are scoped to the selected channel. Stable is the default on first launch.</div>
    </div>

    {#if err}
      <div class="callout error">
        <div class="callout-title">Error</div>
        <div class="callout-body">{err.message}</div>
      </div>
    {/if}

    {#if latest}
      {#if !latest.available}
        <div class="callout warn">
          <div class="callout-title">No stable release yet</div>
          <div class="callout-body">{latest.message}</div>
        </div>
      {:else}
        <div class="callout ok">
          <div class="callout-title">Latest</div>
          <div class="callout-body">
            <div class="kv">
              <div class="k">Channel</div>
              <div class="v">{latest.channel}</div>
              <div class="k">Tag</div>
              <div class="v">{latest.tag}</div>
            </div>
          </div>
        </div>
      {/if}
    {/if}
  </section>
</main>

<style>
  :global(:root) {
    --bg0: #f7f3ea;
    --bg1: #efe7d6;
    --ink: #1c1b16;
    --muted: #5c574c;
    --card: rgba(255, 255, 255, 0.75);
    --stroke: rgba(30, 25, 15, 0.14);
    --accent: #166534;
    --accent-ink: #0b1b10;
    --warn: #9a3412;
    --err: #b42318;

    font-family: "IBM Plex Sans", "Space Grotesk", "Noto Sans", ui-sans-serif;
    font-size: 16px;
    line-height: 24px;
    color: var(--ink);
    background: radial-gradient(1200px 800px at 15% 10%, var(--bg1), var(--bg0));
    font-synthesis: none;
    text-rendering: optimizeLegibility;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  .page {
    min-height: 100vh;
    padding: 42px 22px;
    display: grid;
    gap: 22px;
    place-items: start center;
  }

  .hero {
    width: min(860px, 100%);
  }

  .brand {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 14px;
    align-items: center;
  }

  .mark {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    display: grid;
    place-items: center;
    background: linear-gradient(180deg, rgba(22, 101, 52, 0.18), rgba(22, 101, 52, 0.06));
    border: 1px solid var(--stroke);
    color: var(--accent-ink);
    font-weight: 700;
    letter-spacing: 0.04em;
  }

  h1 {
    margin: 0;
    font-size: 22px;
    line-height: 28px;
  }

  .titles p {
    margin: 2px 0 0;
    color: var(--muted);
  }

  .panel {
    width: min(860px, 100%);
    padding: 18px;
    border-radius: 18px;
    background: var(--card);
    border: 1px solid var(--stroke);
    backdrop-filter: blur(10px);
  }

  .row {
    display: grid;
    grid-template-columns: 92px 1fr;
    gap: 12px;
    align-items: center;
  }

  .label {
    font-weight: 600;
    color: var(--muted);
  }

  .segmented {
    display: inline-flex;
    gap: 6px;
    padding: 6px;
    border-radius: 14px;
    border: 1px solid var(--stroke);
    background: rgba(255, 255, 255, 0.55);
  }

  .segmented button {
    appearance: none;
    border: 1px solid transparent;
    background: transparent;
    padding: 10px 12px;
    border-radius: 12px;
    font-weight: 600;
    color: var(--muted);
    cursor: pointer;
  }

  .segmented button.selected {
    background: rgba(22, 101, 52, 0.12);
    border-color: rgba(22, 101, 52, 0.18);
    color: var(--accent-ink);
  }

  .actions {
    margin-top: 14px;
    display: grid;
    gap: 10px;
  }

  .primary {
    width: fit-content;
    border-radius: 14px;
    border: 1px solid rgba(22, 101, 52, 0.24);
    background: linear-gradient(180deg, rgba(22, 101, 52, 0.18), rgba(22, 101, 52, 0.08));
    padding: 12px 14px;
    font-weight: 700;
    cursor: pointer;
  }

  .primary:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .hint {
    color: var(--muted);
    font-size: 13px;
  }

  .callout {
    margin-top: 14px;
    padding: 12px 12px;
    border-radius: 14px;
    border: 1px solid var(--stroke);
    background: rgba(255, 255, 255, 0.6);
  }

  .callout-title {
    font-weight: 700;
    margin-bottom: 6px;
  }

  .callout.ok {
    border-color: rgba(22, 101, 52, 0.22);
  }

  .callout.warn {
    border-color: rgba(154, 52, 18, 0.22);
  }

  .callout.error {
    border-color: rgba(180, 35, 24, 0.22);
  }

  .kv {
    display: grid;
    grid-template-columns: 90px 1fr;
    gap: 6px 10px;
    align-items: baseline;
  }

  .k {
    color: var(--muted);
    font-weight: 600;
  }

  .v {
    font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono",
      "Courier New", monospace;
  }

  @media (max-width: 520px) {
    .row {
      grid-template-columns: 1fr;
      gap: 8px;
    }
    .segmented {
      width: 100%;
      justify-content: space-between;
    }
    .segmented button {
      flex: 1;
      text-align: center;
    }
  }
</style>
