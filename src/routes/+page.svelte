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
    color-scheme: dark;

    --h-teal: 192;
    --h-orange: 28;

    --bg0: hsl(200 34% 7%);
    --bg1: hsl(200 28% 10%);
    --bg-gradient:
      radial-gradient(1200px 700px at 22% 92%, hsl(var(--h-teal) 78% 52% / 0.18), transparent 58%),
      radial-gradient(900px 600px at 88% 12%, hsl(var(--h-orange) 95% 60% / 0.10), transparent 55%),
      linear-gradient(180deg, hsl(200 34% 7%), hsl(200 24% 10%));

    --ink: hsl(210 18% 92%);
    --muted: hsl(210 12% 72%);
    --card: hsl(196 22% 12% / 0.78);
    --surface: hsl(196 22% 12% / 0.58);
    --surface-2: hsl(196 20% 15% / 0.58);
    --stroke: hsl(200 16% 20%);

    --primary: hsl(var(--h-teal) 74% 52%);
    --accent: hsl(var(--h-orange) 92% 60%);
    --accent-ink: var(--ink);

    --accent-soft-bg: hsl(var(--h-teal) 78% 52% / 0.16);
    --accent-soft-stroke: hsl(var(--h-teal) 78% 52% / 0.24);
    --primary-border: hsl(var(--h-teal) 78% 52% / 0.34);
    --primary-bg-top: hsl(var(--h-teal) 78% 52% / 0.18);
    --primary-bg-bot: hsl(var(--h-teal) 78% 52% / 0.08);

    --ok-stroke: hsl(var(--h-teal) 78% 52% / 0.28);
    --warn-stroke: hsl(var(--h-orange) 92% 60% / 0.30);
    --err-stroke: hsl(2 82% 58% / 0.30);

    font-family: "IBM Plex Sans", "Space Grotesk", "Noto Sans", ui-sans-serif;
    font-size: 16px;
    line-height: 24px;
    color: var(--ink);
    background: var(--bg-gradient);
    font-synthesis: none;
    text-rendering: optimizeLegibility;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  :global(:root[data-theme="light"]) {
    color-scheme: light;

    --bg0: hsl(var(--h-orange) 26% 98%);
    --bg1: hsl(210 28% 97%);
    --bg-gradient:
      radial-gradient(1200px 700px at 18% 88%, hsl(var(--h-teal) 60% 50% / 0.16), transparent 55%),
      radial-gradient(900px 600px at 90% 12%, hsl(var(--h-orange) 95% 55% / 0.12), transparent 52%),
      linear-gradient(180deg, hsl(var(--h-orange) 26% 99%), hsl(200 28% 96%));

    --ink: hsl(210 18% 14%);
    --muted: hsl(210 12% 38%);
    --card: hsl(200 28% 96% / 0.75);
    --surface: hsl(200 28% 96% / 0.58);
    --surface-2: hsl(200 26% 94% / 0.62);
    --stroke: hsl(210 18% 88%);

    --primary: hsl(var(--h-teal) 72% 38%);
    --accent: hsl(var(--h-orange) 92% 56%);
    --accent-ink: var(--ink);

    --accent-soft-bg: hsl(var(--h-teal) 72% 38% / 0.16);
    --accent-soft-stroke: hsl(var(--h-teal) 72% 38% / 0.22);
    --primary-border: hsl(var(--h-teal) 72% 38% / 0.28);
    --primary-bg-top: hsl(var(--h-teal) 72% 38% / 0.16);
    --primary-bg-bot: hsl(var(--h-teal) 72% 38% / 0.08);

    --ok-stroke: hsl(158 62% 38% / 0.26);
    --warn-stroke: hsl(var(--h-orange) 92% 56% / 0.30);
    --err-stroke: hsl(2 78% 50% / 0.26);
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
    background: linear-gradient(
      180deg,
      hsl(var(--h-teal) 78% 52% / 0.18),
      hsl(var(--h-teal) 78% 52% / 0.06)
    );
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
    background: var(--surface);
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
    background: var(--accent-soft-bg);
    border-color: var(--accent-soft-stroke);
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
    border: 1px solid var(--primary-border);
    background: linear-gradient(180deg, var(--primary-bg-top), var(--primary-bg-bot));
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
    background: var(--surface-2);
  }

  .callout-title {
    font-weight: 700;
    margin-bottom: 6px;
  }

  .callout.ok {
    border-color: var(--ok-stroke);
  }

  .callout.warn {
    border-color: var(--warn-stroke);
  }

  .callout.error {
    border-color: var(--err-stroke);
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
