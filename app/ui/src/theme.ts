// Theme preference: follow Windows ("system") or force light/dark.
// Stored in localStorage: it is a display preference, never a secret.

export type Theme = "system" | "light" | "dark";

const KEY = "llavero.theme";

export function loadTheme(): Theme {
  try {
    const value = localStorage.getItem(KEY);
    if (value === "light" || value === "dark") return value;
  } catch {
    // Storage unavailable: fall back to the system theme.
  }
  return "system";
}

/** Applies the theme to the page and remembers it. */
export function applyTheme(theme: Theme): void {
  const root = document.documentElement;
  if (theme === "system") {
    delete root.dataset.theme;
  } else {
    root.dataset.theme = theme;
  }
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    // Not remembered, but still applied for this session.
  }
}
