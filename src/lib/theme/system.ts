export type Theme = "light" | "dark";

function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
}

function detectByMedia(): Theme {
  const mql = window.matchMedia?.("(prefers-color-scheme: light)");
  return mql?.matches ? "light" : "dark";
}

export async function startSystemThemeSync(): Promise<() => void> {
  // Default fallback if the platform cannot detect a theme.
  applyTheme("dark");

  // Prefer Tauri's window theme tracking when available.
  try {
    const mod = await import("@tauri-apps/api/window");
    const win = mod.getCurrentWindow();

    const initial = (await win.theme()) ?? detectByMedia();
    applyTheme(initial);

    const unlisten = await win.onThemeChanged(({ payload }) => {
      applyTheme(payload ?? "dark");
    });

    // Ensure native window follows system theme.
    try {
      await win.setTheme(undefined);
    } catch {
      // ignore
    }

    return () => {
      try {
        unlisten();
      } catch {
        // ignore
      }
    };
  } catch {
    // Fall back to CSS media queries.
    const mql = window.matchMedia?.("(prefers-color-scheme: light)");
    applyTheme(mql?.matches ? "light" : "dark");

    if (!mql) {
      return () => {};
    }

    const handler = (e: MediaQueryListEvent) => {
      applyTheme(e.matches ? "light" : "dark");
    };

    if (typeof mql.addEventListener === "function") {
      mql.addEventListener("change", handler);
      return () => mql.removeEventListener("change", handler);
    }

    // Legacy API.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (mql as any).addListener(handler);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    return () => (mql as any).removeListener(handler);
  }
}
