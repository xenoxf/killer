import {
  createContext,
  useContext,
  createSignal,
  createEffect,
  onMount,
  type JSX,
  type Accessor,
} from "solid-js";

export type ThemeName =
  | "light"
  | "dark"
  | "sakura"
  | "ocean"
  | "coffee"
  | "forest"
  | "sunset";

export interface ThemeDef {
  name: ThemeName;
  label: string;
  /* Clase que se aplica a <html>. Vacía = tema por defecto (:root). */
  className: string;
  /* Gradiente de preview (usado en el botón y el dropdown). */
  preview: string;
}

export const themes: ThemeDef[] = [
  {
    name: "light",
    label: "Claro",
    className: "",
    preview: "linear-gradient(135deg, #f1f5f9, #e2e8f0)",
  },
  {
    name: "dark",
    label: "Oscuro",
    className: "dark",
    preview: "linear-gradient(135deg, #334155, #020617)",
  },
  {
    name: "ocean",
    label: "Océano",
    className: "theme-ocean",
    preview: "linear-gradient(135deg, #38bdf8, #06b6d4)",
  },
  {
    name: "coffee",
    label: "Café",
    className: "theme-coffee",
    preview: "linear-gradient(135deg, #f59e0b, #ea580c)",
  },
  {
    name: "forest",
    label: "Bosque",
    className: "theme-forest",
    preview: "linear-gradient(135deg, #22c55e, #15803d)",
  },
  {
    name: "sunset",
    label: "Atardecer",
    className: "theme-sunset",
    preview: "linear-gradient(135deg, #a855f7, #ec4899)",
  },
  {
    name: "sakura",
    label: "Cerezo",
    className: "theme-sakura",
    preview: "hotpink",
  },
];

const STORAGE_KEY = "killer-theme";
const THEME_CLASSES = themes.map((t) => t.className).filter(Boolean);

function readStoredTheme(): ThemeName {
  try {
    const stored = localStorage.getItem(STORAGE_KEY) as ThemeName | null;
    if (stored && themes.some((t) => t.name === stored)) return stored;
  } catch {
    // localStorage no disponible (ej: Tauri con restricciones) -> default
  }
  return "light";
}

function applyThemeClass(name: ThemeName) {
  try {
    const def = themes.find((t) => t.name === name) ?? themes[0];
    const root = document.documentElement;
    THEME_CLASSES.forEach((c) => root.classList.remove(c));
    if (def.className) root.classList.add(def.className);
    // Atributo para debugging + color-scheme para scrollbars/form controls
    root.dataset.theme = def.name;
    root.style.colorScheme = def.name === "dark" ? "dark" : "light";
  } catch {
    // document no disponible (SSR/tests) -> no-op
  }
}

interface ThemeContextValue {
  theme: Accessor<ThemeName>;
  setTheme: (name: ThemeName) => void;
}

const ThemeContext = createContext<ThemeContextValue>();

export function ThemeProvider(props: { children: JSX.Element }) {
  const [theme, setThemeSignal] = createSignal<ThemeName>(readStoredTheme());

  // Aplicar al montar (evita tocar `document` durante el render, que en
  // Solid puede ejecutarse más de una vez) y en cada cambio.
  onMount(() => applyThemeClass(theme()));
  createEffect(() => applyThemeClass(theme()));

  const setTheme = (name: ThemeName) => {
    setThemeSignal(name);
    try {
      localStorage.setItem(STORAGE_KEY, name);
    } catch {
      // storage bloqueado -> el tema igual aplica en memoria
    }
    applyThemeClass(name);
  };

  return (
    <ThemeContext.Provider value={{ theme, setTheme }}>
      {props.children}
    </ThemeContext.Provider>
  );
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) {
    throw new Error("useTheme debe usarse dentro de <ThemeProvider>");
  }
  return ctx;
}
