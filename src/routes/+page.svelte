<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";

  import type {
    ApiError,
    Channel,
    InstallEvent,
    InstallState,
    LatestManifestResponse,
    Platform,
    Status,
  } from "$lib/api/types";
  import {
    installLatest,
    resolveLatestManifest,
    settingsSetChannel,
    settingsSetProfile,
    statusGet,
  } from "$lib/api/client";

  let channel = $state<Channel>("stable");
  let profile = $state<string>("default");
  let platform = $state<Platform | null>(null);

  let loadingLatest = $state(false);
  let installing = $state(false);
  let err = $state<ApiError | null>(null);
  let latest = $state<LatestManifestResponse | null>(null);

  let installed = $state<InstallState | null>(null);
  let payloadRoot = $state<string | null>(null);

  let profileOptions = $state<string[]>(["default", "bitwig"]);
  let installEvent = $state<InstallEvent | null>(null);

  onMount(() => {
    let unlistenInstall: (() => void) | null = null;
    let cancelled = false;

    void (async () => {
      try {
        unlistenInstall = await listen<InstallEvent>("ms-manager://install", (e) => {
          installEvent = e.payload;
        });
      } catch {
        // ignore
      }
    })();

    void (async () => {
      try {
        const st: Status = await statusGet();
        channel = st.settings.channel;
        profile = st.settings.profile;
        platform = st.platform;
        installed = st.installed;
        payloadRoot = st.payload_root;

        if (!cancelled) {
          void refreshLatest();
        }
      } catch (e) {
        err = e as ApiError;
      }
    })();

    return () => {
      cancelled = true;
      unlistenInstall?.();
    };
  });

  async function setChannel(next: Channel) {
    if (installing) return;
    err = null;
    latest = null;
    try {
      const s = await settingsSetChannel(next);
      channel = s.channel;
      profile = s.profile;
    } catch (e) {
      err = e as ApiError;
    }

    void refreshLatest();
  }

  async function setProfile(next: string) {
    if (installing) return;
    err = null;
    try {
      const s = await settingsSetProfile(next);
      profile = s.profile;
    } catch (e) {
      err = e as ApiError;
    }
  }

  async function refreshLatest() {
    loadingLatest = true;
    err = null;
    latest = null;
    try {
      const out = await resolveLatestManifest(channel);
      latest = out;

      const m = out.manifest;
      const p = platform;
      if (m && p) {
        const ids = m.install_sets
          .filter((s) => s.os === p.os && s.arch === p.arch)
          .map((s) => s.id);
        const uniq = Array.from(new Set(ids));
        profileOptions = uniq.length ? uniq : ["default"];

        if (!profileOptions.includes(profile)) {
          profile = profileOptions[0] ?? "default";
          void setProfile(profile);
        }
      }
    } catch (e) {
      err = e as ApiError;
    } finally {
      loadingLatest = false;
    }
  }

  function installStatusLine(e: InstallEvent | null): string | null {
    if (!e) return null;
    if (e.type === "begin") return `Preparing ${e.tag} (${e.profile})…`;
    if (e.type === "downloading") return `Downloading ${e.index}/${e.total}: ${e.filename}`;
    if (e.type === "applying") return `Applying: ${e.step}`;
    if (e.type === "done") return `Installed ${e.tag} (${e.profile})`;
    return null;
  }

  async function runInstallLatest() {
    installing = true;
    err = null;
    installEvent = null;
    try {
      const out = await installLatest(channel, profile);
      installed = out;
      void refreshLatest();
    } catch (e) {
      err = e as ApiError;
    } finally {
      installing = false;
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
          disabled={installing}
          onclick={() => setChannel("stable")}
        >
          Stable
        </button>
        <button
          type="button"
          class:selected={channel === "beta"}
          disabled={installing}
          onclick={() => setChannel("beta")}
        >
          Beta
        </button>
        <button
          type="button"
          class:selected={channel === "nightly"}
          disabled={installing}
          onclick={() => setChannel("nightly")}
        >
          Nightly
        </button>
      </div>
    </div>

    <div class="row">
      <div class="label">Profile</div>
      <div class="segmented" role="radiogroup" aria-label="Profile">
        {#each profileOptions as p}
          <button type="button" class:selected={profile === p} disabled={installing} onclick={() => setProfile(p)}>
            {#if p === "default"}
              Standalone
            {:else if p === "bitwig"}
              Bitwig
            {:else}
              {p}
            {/if}
          </button>
        {/each}
      </div>
    </div>

    <div class="actions">
      <div class="buttons">
        <button class="primary" type="button" disabled={installing} onclick={runInstallLatest}>
          {#if installing}
            Installing…
          {:else}
            Install / Update latest
          {/if}
        </button>
        <button class="ghost" type="button" disabled={loadingLatest || installing} onclick={refreshLatest}>
          {#if loadingLatest}
            Checking…
          {:else}
            Check latest
          {/if}
        </button>
      </div>
      <div class="hint">
        Updates are scoped to the selected channel.
        {#if platform}
          Platform: {platform.os}/{platform.arch}.
        {/if}
      </div>
      {#if installStatusLine(installEvent)}
        <div class="progress">{installStatusLine(installEvent)}</div>
      {/if}
    </div>

    {#if err}
      <div class="callout error">
        <div class="callout-title">Error</div>
        <div class="callout-body">{err.message}</div>
      </div>
    {/if}

    {#if installed}
      <div class="callout ok">
        <div class="callout-title">Installed</div>
        <div class="callout-body">
          <div class="kv">
            <div class="k">Tag</div>
            <div class="v">{installed.tag}</div>
            <div class="k">Profile</div>
            <div class="v">{installed.profile}</div>
            {#if payloadRoot}
              <div class="k">Root</div>
              <div class="v">{payloadRoot}</div>
            {/if}
          </div>
        </div>
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
  .page {
    height: 100vh;
    padding: 14px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 100%;
  }

  .hero,
  .panel {
    border: 1px solid var(--border);
    background: var(--panel);
    border-radius: 6px;
  }

  .hero {
    padding: 12px;
  }

  .brand {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 12px;
    align-items: center;
  }

  .mark {
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid var(--border);
    font-weight: 800;
    letter-spacing: 0.06em;
  }

  h1 {
    margin: 0;
    font-size: 16px;
    line-height: 20px;
    font-weight: 800;
  }

  .titles p {
    margin: 4px 0 0;
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
  }

  .panel {
    padding: 12px;
  }

  .row {
    display: grid;
    grid-template-columns: 86px 1fr;
    gap: 10px;
    align-items: center;
    padding: 6px 0;
  }

  .label {
    color: var(--muted);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 11px;
  }

  .segmented {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: 1fr;
    gap: 8px;
  }

  .segmented button {
    appearance: none;
    font: inherit;
    padding: 10px 10px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-weight: 800;
    text-align: center;
    user-select: none;
  }

  .segmented button.selected {
    background: var(--fg);
    color: var(--bg);
    border-color: var(--fg);
  }

  button:focus-visible {
    outline: 2px solid var(--border-strong);
    outline-offset: 2px;
  }

  button:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }

  .actions {
    margin-top: 8px;
    display: grid;
    gap: 10px;
  }

  .buttons {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .primary,
  .ghost {
    appearance: none;
    font: inherit;
    padding: 12px 12px;
    border-radius: 6px;
    border: 1px solid var(--border);
    background: transparent;
    color: var(--fg);
    font-weight: 900;
    letter-spacing: 0.02em;
    text-align: left;
    cursor: pointer;
  }

  .primary {
    background: var(--fg);
    color: var(--bg);
    border-color: var(--fg);
  }

  .ghost {
    color: var(--muted);
  }

  .hint {
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
  }

  .progress {
    padding: 10px 12px;
    border-radius: 6px;
    border: 1px dashed var(--border);
    color: var(--muted);
    font-size: 12px;
    line-height: 16px;
  }

  .callout {
    margin-top: 10px;
    padding: 10px 12px;
    border-radius: 6px;
    border: 1px solid var(--border);
  }

  .callout-title {
    font-weight: 900;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 11px;
    margin-bottom: 6px;
    color: var(--muted);
  }

  .callout.ok {
    border-style: solid;
  }

  .callout.warn {
    border-style: dashed;
  }

  .callout.error {
    border-style: double;
    border-width: 3px;
  }

  .kv {
    display: grid;
    grid-template-columns: 90px 1fr;
    gap: 6px 10px;
    align-items: baseline;
  }

  .k {
    color: var(--muted);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 11px;
  }

  .v {
    overflow-wrap: anywhere;
  }

  @media (max-width: 560px) {
    .page {
      padding: 12px;
    }
    .row {
      grid-template-columns: 1fr;
    }
    .buttons {
      grid-template-columns: 1fr;
    }
    .segmented {
      grid-auto-flow: row;
      grid-auto-columns: auto;
    }
  }
  </style>
