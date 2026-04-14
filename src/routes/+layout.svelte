<script lang="ts">
  import { onMount } from "svelte";
  import { startSystemThemeSync } from "$lib/theme/system";

  onMount(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;

    void (async () => {
      try {
        unlisten = await startSystemThemeSync();
        if (cancelled) {
          unlisten();
        }
      } catch {
        // ignore
      }
    })();

    return () => {
      cancelled = true;
      unlisten?.();
    };
  });
</script>

<slot />

<style>
  @font-face {
    font-family: "JetBrainsMono Nerd Font Mono";
    src: url("/fonts/JetBrainsMonoNerdFontMono-Regular.ttf") format("truetype");
    font-style: normal;
    font-weight: 400;
    font-display: swap;
  }

  :global(*) {
    box-sizing: border-box;
  }

  :global(html, body) {
    height: 100%;
  }

  :global(:root) {
    color-scheme: dark;

    --bg: #0c0f14;
    --panel: #11151c;
    --fg: #d9dde3;
    --muted: #9aa3ad;
    --border: #2b313a;
    --border-strong: #3b4350;
    --value: #bfc6d1;
    --ok: #3fd07f;
    --warn: #d7b35b;
    --err: #e16b6b;
    --log-rx: #57c7ff;
    --log-tx: #3fd07f;
    --log-system: #d9b85c;
    --log-info: #7dcfff;
    --log-source: #c58cff;
    --log-row-hover: #171d27;

    --font-mono: "JetBrainsMono Nerd Font Mono", "IBM Plex Mono", ui-monospace, SFMono-Regular,
      Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
    --font-log: var(--font-mono);
    --font-sans: "Aptos", "Segoe UI Variable Text", "Segoe UI", Arial, sans-serif;

    --space-1: 6px;
    --space-2: 8px;
    --space-3: 10px;
    --space-4: 12px;
    --space-5: 14px;
    --radius-panel: 6px;
    --radius-card: 8px;
    --tabs-strip-height: 78px;
    --control-height: 40px;
    --control-radius: 6px;
    --control-padding-x: 10px;
    --pill-padding-y: 4px;
    --pill-padding-x: 10px;

    font-size: 15px;
    line-height: 22px;
    text-rendering: geometricPrecision;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  :global(:root[data-theme="light"]) {
    color-scheme: light;

    --bg: #f6f7f8;
    --panel: #ffffff;
    --fg: #10131a;
    --muted: #475569;
    --border: #d1d5db;
    --border-strong: #9ca3af;
    --value: #0f172a;
    --ok: #0a7f41;
    --warn: #8a6a0b;
    --err: #b42318;
    --log-rx: #0b6ccf;
    --log-tx: #0a7f41;
    --log-system: #9a6700;
    --log-info: #155eef;
    --log-source: #7a3ef0;
    --log-row-hover: #eef2f7;
  }

  :global(body) {
    margin: 0;
    background: var(--bg);
    color: var(--fg);
    font-family: var(--font-mono);
  }

  :global(*) {
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--border-strong) 72%, transparent) transparent;
  }

  :global(*::-webkit-scrollbar) {
    width: 8px;
    height: 8px;
  }

  :global(*::-webkit-scrollbar-track) {
    background: transparent;
  }

  :global(*::-webkit-scrollbar-thumb) {
    background: color-mix(in srgb, var(--border-strong) 72%, transparent);
    border-radius: 999px;
    border: 2px solid transparent;
    background-clip: padding-box;
  }

  :global(*::-webkit-scrollbar-thumb:hover) {
    background: color-mix(in srgb, var(--muted) 52%, transparent);
    border: 2px solid transparent;
    background-clip: padding-box;
  }

  :global(*::-webkit-scrollbar-corner) {
    background: transparent;
  }

  :global(*::-webkit-scrollbar-button) {
    display: none;
    width: 0;
    height: 0;
  }
</style>
