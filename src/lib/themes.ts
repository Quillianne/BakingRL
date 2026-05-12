export const THEME_STORAGE_KEY = "bakingrl-theme";

// Add new themes here, then add the matching CSS variable block in theme.css.
export const THEMES = [
  {
    id: "industrial-bakery",
    label: "Industrial Bakery",
    description: "Dark broadcast workshop.",
    preview: {
      background: "#121110",
      surface: "#292520",
      accent: "#d75f1d",
      text: "#f1ece4"
    }
  },
  {
    id: "light-bakery",
    label: "Light Bakery",
    description: "Light studio control room.",
    preview: {
      background: "#f4eadc",
      surface: "#fff8ef",
      accent: "#b95219",
      text: "#201a15"
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
