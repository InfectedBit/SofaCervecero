/**
 * Temas visuales. Las 9 paletas vienen de TimeTrack (`ui/static/shared.js`), para que las dos
 * apps del mismo usuario se vean iguales. Ver `ARCHITECTURE/02-integracion-timetrack.md` §5.
 *
 * TimeTrack define 9 variables; nosotros usamos una escala de fondo de 4 niveles
 * (`--bg` < `--bg2` < `--panel` < `--panel2`). Las que faltan se derivan aclarando en HSL, así
 * que un tema solo necesita declarar `bg`/`surface` — y puede fijarlas a mano si le conviene.
 */
export type Theme = {
  id: string;
  name: string;
  bg: string;
  surface: string;
  border: string;
  accent: string;
  accent2: string;
  green: string;
  text: string;
  muted: string;
  danger: string;
  /** Overrides opcionales de la escala derivada. */
  bg2?: string;
  panel?: string;
  panel2?: string;
};

/** Variables editables en el editor de temas propios. */
export const THEME_VARS = [
  ["bg", "Fondo"],
  ["surface", "Superficies"],
  ["border", "Bordes"],
  ["accent", "Acento"],
  ["accent2", "Acento 2"],
  ["green", "Positivo"],
  ["text", "Texto"],
  ["muted", "Texto suave"],
  ["danger", "Peligro"],
] as const;

export const THEMES: Theme[] = [
  // El aspecto actual de la app: es el tema por defecto y no se toca.
  {
    id: "sofa",
    name: "SofaCervecero",
    bg: "#0e0f13",
    bg2: "#15171e",
    surface: "#1b1e27",
    panel: "#1b1e27",
    panel2: "#232734",
    border: "#2a2e3a",
    accent: "#4ade80",
    accent2: "#22c55e",
    green: "#4ade80",
    text: "#e7e9ee",
    muted: "#9aa0ad",
    danger: "#f87171",
  },
  { id: "synthwave", name: "Synthwave", bg: "#0a0a0f", surface: "#111118", border: "#1e1e2e", accent: "#6366f1", accent2: "#a855f7", green: "#22d3ee", text: "#e2e8f0", muted: "#64748b", danger: "#f43f5e" },
  { id: "nord", name: "Nord", bg: "#0f1117", surface: "#161b22", border: "#2d333b", accent: "#5e81ac", accent2: "#81a1c1", green: "#a3be8c", text: "#d8dee9", muted: "#5e6982", danger: "#bf616a" },
  { id: "gruvbox", name: "Gruvbox", bg: "#1d2021", surface: "#282828", border: "#3c3836", accent: "#d79921", accent2: "#fe8019", green: "#98971a", text: "#ebdbb2", muted: "#a89984", danger: "#cc241d" },
  { id: "catppuccin", name: "Catppuccin", bg: "#1e1e2e", surface: "#181825", border: "#313244", accent: "#cba6f7", accent2: "#f5c2e7", green: "#a6e3a1", text: "#cdd6f4", muted: "#585b70", danger: "#f38ba8" },
  { id: "midnight", name: "Midnight", bg: "#000000", surface: "#0d0d0d", border: "#1a1a1a", accent: "#00ff87", accent2: "#00d4ff", green: "#00ff87", text: "#ffffff", muted: "#666666", danger: "#ff4444" },
  { id: "tokyo-night", name: "Tokyo Night", bg: "#1a1b26", surface: "#16161e", border: "#2a2b3d", accent: "#7aa2f7", accent2: "#bb9af7", green: "#9ece6a", text: "#c0caf5", muted: "#565f89", danger: "#f7768e" },
  { id: "dracula", name: "Dracula", bg: "#282a36", surface: "#21222c", border: "#44475a", accent: "#bd93f9", accent2: "#ff79c6", green: "#50fa7b", text: "#f8f8f2", muted: "#6272a4", danger: "#ff5555" },
  { id: "monokai", name: "Monokai", bg: "#272822", surface: "#1e1f1c", border: "#3e3d32", accent: "#f92672", accent2: "#66d9e8", green: "#a6e22e", text: "#f8f8f2", muted: "#75715e", danger: "#f92672" },
  { id: "solarized-dark", name: "Solarized", bg: "#002b36", surface: "#073642", border: "#1b4a57", accent: "#268bd2", accent2: "#2aa198", green: "#859900", text: "#839496", muted: "#586e75", danger: "#dc322f" },
];

// ── Utilidades de color ──────────────────────────────────────────────────────

function hexToRgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  const full = h.length === 3 ? h.split("").map((c) => c + c).join("") : h;
  return [
    parseInt(full.slice(0, 2), 16),
    parseInt(full.slice(2, 4), 16),
    parseInt(full.slice(4, 6), 16),
  ];
}

function rgbToHex(r: number, g: number, b: number): string {
  const c = (n: number) =>
    Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, "0");
  return `#${c(r)}${c(g)}${c(b)}`;
}

/** Luminosidad relativa aproximada (0–255), para comparar dos tonos. */
export function luminancia(hex: string): number {
  const [r, g, b] = hexToRgb(hex);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/**
 * Aclara (`delta > 0`) u oscurece un color en HSL, conservando tono y saturación.
 * `delta` va en puntos de luminosidad (0–100).
 */
export function aclarar(hex: string, delta: number): string {
  let [r, g, b] = hexToRgb(hex).map((v) => v / 255);
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  let h = 0;
  const l = (max + min) / 2;
  const d = max - min;
  const s = d === 0 ? 0 : d / (1 - Math.abs(2 * l - 1));
  if (d !== 0) {
    if (max === r) h = ((g - b) / d) % 6;
    else if (max === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h *= 60;
    if (h < 0) h += 360;
  }
  const l2 = Math.max(0, Math.min(1, l + delta / 100));
  // HSL → RGB
  const c = (1 - Math.abs(2 * l2 - 1)) * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = l2 - c / 2;
  const seg = Math.floor(h / 60) % 6;
  const tabla: [number, number, number][] = [
    [c, x, 0],
    [x, c, 0],
    [0, c, x],
    [0, x, c],
    [x, 0, c],
    [c, 0, x],
  ];
  const [r2, g2, b2] = tabla[seg];
  return rgbToHex((r2 + m) * 255, (g2 + m) * 255, (b2 + m) * 255);
}

/** Resuelve la escala de fondo completa de un tema (con los overrides que declare). */
export function escala(t: Theme) {
  // En algunos temas `surface` es más oscuro que `bg` (Catppuccin, Dracula…). Nuestra escala
  // debe crecer siempre, así que en ese caso se deriva aclarando el fondo.
  const panel =
    t.panel ?? (luminancia(t.surface) > luminancia(t.bg) ? t.surface : aclarar(t.bg, 4));
  return {
    "--bg": t.bg,
    "--bg2": t.bg2 ?? aclarar(t.bg, 2),
    "--panel": panel,
    "--panel2": t.panel2 ?? aclarar(panel, 4),
    "--border": t.border,
    "--text": t.text,
    "--muted": t.muted,
    "--accent": t.accent,
    "--accent2": t.accent2,
    "--green": t.green,
    "--danger": t.danger,
  };
}

/** Aplica un tema al documento. `accento` sobrescribe solo el color de acento. */
export function aplicarTema(t: Theme, acento?: string | null) {
  const root = document.documentElement;
  const vars = escala(t);
  for (const [k, v] of Object.entries(vars)) root.style.setProperty(k, v);
  if (acento) {
    root.style.setProperty("--accent", acento);
    // El acento secundario se deriva para que botones y hovers sigan siendo coherentes.
    root.style.setProperty("--accent2", aclarar(acento, -8));
  }
}

export function buscarTema(id: string, propios: Theme[] = []): Theme {
  return THEMES.find((t) => t.id === id) ?? propios.find((t) => t.id === id) ?? THEMES[0];
}
