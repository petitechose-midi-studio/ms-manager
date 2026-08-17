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

    --bg: #090909;
    --panel: #0e0e0f;
    --panel-elevated: #151516;
    --fg: #e8e8ea;
    --muted: #96969c;
    --border: #29292c;
    --border-strong: #3b3b40;
    --value: #d3d3d7;
    --brand: #f51b4b;
    --ok: #49e4b0;
    --warn: #ffa51a;
    --err: #ff5c72;
    --log-rx: #7fa7c7;
    --log-tx: #49e4b0;
    --log-system: #ffa51a;
    --log-info: #9ab6cc;
    --log-source: #b69ad9;
    --log-row-hover: #151516;

    --font-mono: "JetBrainsMono Nerd Font Mono", "IBM Plex Mono", ui-monospace, SFMono-Regular,
      Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
    --font-log: var(--font-mono);
    --font-sans: "Aptos", "Segoe UI Variable Text", "Segoe UI", Arial, sans-serif;

    --space-1: 4px;
    --space-2: 8px;
    --space-3: 12px;
    --space-4: 16px;
    --space-5: 24px;
    --radius-panel: 8px;
    --radius-card: 8px;
    --tabs-strip-height: 78px;
    --control-height: 40px;
    --control-radius: 6px;
    --control-padding-x: 12px;
    --pill-padding-y: 4px;
    --pill-padding-x: 8px;

    font-size: 15px;
    line-height: 22px;
    text-rendering: geometricPrecision;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  :global(:root[data-theme="light"]) {
    color-scheme: light;

    --bg: #f4f4f3;
    --panel: #ffffff;
    --panel-elevated: #ececed;
    --fg: #18181a;
    --muted: #606066;
    --border: #d3d3d5;
    --border-strong: #aaaab0;
    --value: #2b2b2e;
    --brand: #d90f3e;
    --ok: #087a55;
    --warn: #8a5700;
    --err: #bd2444;
    --log-rx: #416c8d;
    --log-tx: #087a55;
    --log-system: #8a5700;
    --log-info: #416c8d;
    --log-source: #72529a;
    --log-row-hover: #ececed;
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
