export const THEME_STORAGE_KEY = "bakingrl-theme";

// Add new themes here, then add the matching CSS variable block in theme.css.
export const THEMES = [
  {
    id: "modern-dark",
    label: "Modern Dark",
    description: "Neutral, compact dark interface.",
    preview: {
      background: "#0f1115",
      surface: "#232832",
      accent: "#3b82f6",
      text: "#e2e8f0"
    }
  },
  {
    id: "neon-cyber",
    label: "Neon Cyber",
    description: "High-contrast cyber palette.",
    preview: {
      background: "#050505",
      surface: "#0A0F1A",
      accent: "#FF6B00",
      text: "#00E5FF"
    }
  },
  {
    id: "industrial-bakery",
    label: "Industrial Bakery",
    description: "Dense workshop controls with warm accents.",
    preview: {
      background: "#1A1A1A",
      surface: "#2C2C2C",
      accent: "#D95C14",
      text: "#E0E0E0"
    }
  },
  {
    id: "pro-streamer",
    label: "Pro Streamer",
    description: "Broadcast-focused purple and pink.",
    preview: {
      background: "#0D0814",
      surface: "#21162e",
      accent: "#FF007F",
      text: "#FFFFFF"
    }
  },
  {
    id: "hacker-terminal",
    label: "Hacker Terminal",
    description: "Monospace terminal contrast.",
    preview: {
      background: "#0D1110",
      surface: "#02150a",
      accent: "#00FF66",
      text: "#00FF66"
    }
  }
] as const;

export type ThemeId = (typeof THEMES)[number]["id"];

export const DEFAULT_THEME: ThemeId = "modern-dark";

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
