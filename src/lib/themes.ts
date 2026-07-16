export const THEME_STORAGE_KEY = "bakingrl-theme";

// Add new themes here, then add the matching CSS variable block in theme.css.
export const THEMES = [
  {
    id: "industrial-bakery",
    label: "Control Room",
    description: "Dark broadcast workspace.",
    preview: {
      background: "#0f1216",
      surface: "#20262d",
      accent: "#f06445",
      text: "#f3f5f7"
    }
  },
  {
    id: "light-bakery",
    label: "Daylight",
    description: "Light broadcast workspace.",
    preview: {
      background: "#f4f6f8",
      surface: "#ffffff",
      accent: "#d84e32",
      text: "#17212b"
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
