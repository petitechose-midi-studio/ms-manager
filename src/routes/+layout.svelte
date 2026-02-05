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

    --font-mono: "IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas,
      "Liberation Mono", "Courier New", monospace;
    --font-sans: "IBM Plex Sans", "Space Grotesk", ui-sans-serif, "Noto Sans", sans-serif;

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
  }

  :global(body) {
    margin: 0;
    background: var(--bg);
    color: var(--fg);
    font-family: var(--font-mono);
  }
</style>
