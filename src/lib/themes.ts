export const THEME_STORAGE_KEY = "bakingrl-theme";

// Add new themes here, then add the matching CSS variable block in theme.css.
export const THEMES = [
  {
    id: "industrial-bakery",
    label: "Signal Black",
    description: "Dark control desk.",
    preview: {
      background: "#0d0f0d",
      surface: "#252b23",
      accent: "#b7e45c",
      text: "#f1f4ed"
    }
  },
  {
    id: "light-bakery",
    label: "Studio Paper",
    description: "Light control desk.",
    preview: {
      background: "#ecefe8",
      surface: "#f8faf5",
      accent: "#527d10",
      text: "#171b15"
    }
  }
] as const;

export type ThemeId = (typeof THEMES)[number]["id"];

export const DEFAULT_THEME: ThemeId = "industrial-bakery";

export function isThemeId(value: string | null | undefined): value is ThemeId {
  return THEMES.some((theme) => theme.id === value);
}

export function getStoredTheme(): ThemeId {
  if (typeof localStorage === "undefined") return DEFAULT_THEME;
  const storedTheme = localStorage.getItem(THEME_STORAGE_KEY);
  return isThemeId(storedTheme) ? storedTheme : DEFAULT_THEME;
}

export function applyTheme(themeId: string | null | undefined): ThemeId {
  const nextTheme = isThemeId(themeId) ? themeId : DEFAULT_THEME;

  if (typeof document !== "undefined") {
    document.documentElement.dataset.theme = nextTheme;
  }

  if (typeof localStorage !== "undefined") {
    localStorage.setItem(THEME_STORAGE_KEY, nextTheme);
  }

  return nextTheme;
}
