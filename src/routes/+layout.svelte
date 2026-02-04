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

    --bg: #0b0d10;
    --panel: #0f1217;
    --fg: #e6e6e6;
    --muted: #9aa0a6;
    --border: #2a2f37;
    --border-strong: #3a404a;

    --font-mono: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono",
      "Courier New", monospace;

    font-size: 14px;
    line-height: 20px;
    text-rendering: geometricPrecision;
    -webkit-font-smoothing: antialiased;
    -moz-osx-font-smoothing: grayscale;
  }

  :global(:root[data-theme="light"]) {
    color-scheme: light;

    --bg: #f7f7f7;
    --panel: #ffffff;
    --fg: #101318;
    --muted: #4b5563;
    --border: #d4d4d4;
    --border-strong: #a3a3a3;
  }

  :global(body) {
    margin: 0;
    background: var(--bg);
    color: var(--fg);
    font-family: var(--font-mono);
  }
</style>
