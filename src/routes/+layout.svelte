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
