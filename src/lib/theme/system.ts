export type Theme = "light" | "dark";

function applyTheme(theme: Theme) {
  document.documentElement.dataset.theme = theme;
  document.documentElement.style.colorScheme = theme;
}

function detectByMedia(): Theme {
  // Prefer a conservative default: if we can't confidently detect, stay on dark.
  const dark = window.matchMedia?.("(prefers-color-scheme: dark)");
  const light = window.matchMedia?.("(prefers-color-scheme: light)");

  const darkMatches = !!dark?.matches;
  const lightMatches = !!light?.matches;

  if (darkMatches && !lightMatches) return "dark";
  if (lightMatches && !darkMatches) return "light";

  return "dark";
}

export async function startSystemThemeSync(): Promise<() => void> {
  // Default fallback if the platform cannot detect a theme.
  applyTheme("dark");

  // Prefer Tauri's window theme tracking when available.
  try {
    const mod = await import("@tauri-apps/api/window");
    const win = mod.getCurrentWindow();

    const initial = await win.theme();
    applyTheme(initial ?? "dark");

    const unlisten = await win.onThemeChanged(({ payload }) => {
      applyTheme(payload ?? "dark");
    });

    return () => {
      try {
        unlisten();
      } catch {
        // ignore
      }
    };
  } catch {
    // Fall back to CSS media queries.
    applyTheme(detectByMedia());

    const mql = window.matchMedia?.("(prefers-color-scheme: dark)");

    if (!mql) {
      return () => {};
    }

    const handler = () => {
      applyTheme(detectByMedia());
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
