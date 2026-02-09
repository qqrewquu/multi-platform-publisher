import { create } from "zustand";

type Theme = "light" | "dark" | "system";

interface ThemeStore {
  theme: Theme;
  setTheme: (theme: Theme) => void;
  resolvedTheme: "light" | "dark";
}

function getSystemTheme(): "light" | "dark" {
  if (typeof window !== "undefined" && window.matchMedia) {
    return window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  }
  return "light";
}

function applyTheme(theme: "light" | "dark") {
  const root = document.documentElement;
  if (theme === "dark") {
    root.classList.add("dark");
  } else {
    root.classList.remove("dark");
  }
}

const savedTheme = (localStorage.getItem("theme") as Theme) || "system";
const initialResolved = savedTheme === "system" ? getSystemTheme() : savedTheme;
applyTheme(initialResolved);

export const useThemeStore = create<ThemeStore>((set) => ({
  theme: savedTheme,
  resolvedTheme: initialResolved,

  setTheme: (theme) => {
    const resolved = theme === "system" ? getSystemTheme() : theme;
    applyTheme(resolved);
    localStorage.setItem("theme", theme);
    set({ theme, resolvedTheme: resolved });
  },
}));

// Listen for system theme changes
if (typeof window !== "undefined") {
  window
    .matchMedia("(prefers-color-scheme: dark)")
    .addEventListener("change", () => {
      const state = useThemeStore.getState();
      if (state.theme === "system") {
        const resolved = getSystemTheme();
        applyTheme(resolved);
        useThemeStore.setState({ resolvedTheme: resolved });
      }
    });
}
