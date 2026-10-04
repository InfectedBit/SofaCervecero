/**
 * Preferencias de la app (guión H.0_1). Viven en `localStorage` bajo `sc_prefs`, igual que
 * TimeTrack hace con `tt_prefs`: son ajustes **de esta interfaz**, no datos de la biblioteca,
 * así que no tienen por qué pasar por SQLite ni por el core residente.
 */
import { aplicarTema, buscarTema, type Theme } from "./themes";
import { MAPA_POR_DEFECTO, type AccionMando, type Asignacion, type Layout } from "./gamepad";
import type { ModoSeccion } from "./secciones";

export type CardShape = "rect" | "square";
export type ViewMode = "grid" | "list";

export type Prefs = {
  theme: string;
  /** Sobrescribe solo el color de acento del tema activo. */
  accent: string | null;
  customThemes: Theme[];
  cardShape: CardShape;
  view: ViewMode;
  /** Ancho mínimo de card en px: controla cuántas caben por fila. */
  cardSize: number;
  // ── Vista de la cuadrícula (guión F.0_5) ────────────────────────────────────
  // Van aquí y no en `LibraryFilter` porque describen **cómo se ve**, no **qué se ve**: se
  // conservan entre sesiones igual que el tema o el tamaño de card.
  /** Criterio de orden (`title` · `recent` · `playtime` · `players` · `added`). */
  orden: string;
  /** Dirección del orden. `null` = la natural del criterio. */
  ordenDesc: boolean | null;
  /** Eje por el que se parte la cuadrícula. `none` = sin secciones. */
  seccion: ModoSeccion;
  /** Segundo eje, que anida dentro del primero. `none` = un solo nivel. */
  seccion2: ModoSeccion;
  /** Tercer eje. Es el que permite la cadena completa `Cliente › Nº Jugadores › TAGS`. */
  seccion3: ModoSeccion;
  /**
   * Secciones plegadas, como `modo:clave` (p. ej. `categoria:Steam`). Lleva el modo delante
   * porque `[A]` de las iniciales y `[A]` de una categoría que se llame así no son la misma.
   */
  plegadas: string[];
  /** Ventana de «añadidos recientemente», en días. */
  recientesDias: number;
  /** Botón asignado a cada acción del mando (guión E.0_2). */
  gamepadMap: Record<AccionMando, Asignacion>;
  /** Mostrar la barra de ayuda de botones abajo a la derecha. */
  gamepadHints: boolean;
  /** Etiquetas y colores de los botones. `auto` los deduce del mando conectado. */
  gamepadLayout: "auto" | Layout;
  /** Último salto de la cascada de carátulas: el CDN de Steam (sin clave, con internet). */
  artSteamCdn: boolean;
};

const KEY = "sc_prefs";

const DEFAULTS: Prefs = {
  theme: "sofa",
  accent: null,
  customThemes: [],
  cardShape: "rect",
  view: "grid",
  cardSize: 150,
  orden: "title",
  ordenDesc: null,
  seccion: "none",
  seccion2: "none",
  seccion3: "none",
  plegadas: [],
  recientesDias: 14,
  gamepadMap: { ...MAPA_POR_DEFECTO },
  gamepadHints: true,
  gamepadLayout: "auto",
  artSteamCdn: true,
};

function cargar(): Prefs {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return { ...DEFAULTS };
    const guardado = JSON.parse(raw);
    return {
      ...DEFAULTS,
      ...guardado,
      // Si se añaden acciones nuevas, las que falten toman su botón por defecto.
      gamepadMap: { ...MAPA_POR_DEFECTO, ...(guardado.gamepadMap ?? {}) },
    };
  } catch {
    return { ...DEFAULTS };
  }
}

/** Estado reactivo global. Se muta por propiedades; nunca se reasigna. */
export const prefs = $state<Prefs>(cargar());

export function setPrefs(patch: Partial<Prefs>) {
  Object.assign(prefs, patch);
  try {
    localStorage.setItem(KEY, JSON.stringify(prefs));
  } catch {
    // localStorage lleno o deshabilitado: la sesión sigue, solo no persiste.
  }
  if ("theme" in patch || "accent" in patch || "customThemes" in patch) aplicarPrefsTema();
}

export function resetPrefs() {
  setPrefs({ ...DEFAULTS, customThemes: prefs.customThemes });
}

/** Devuelve el mapa del mando a sus valores de fábrica. */
export function resetMando() {
  setPrefs({ gamepadMap: { ...MAPA_POR_DEFECTO } });
}

/**
 * Distribución del mando **detectada** al conectarlo. No se persiste: depende de qué mando
 * haya enchufado ahora, no de una preferencia.
 */
export const mando = $state<{ layout: Layout }>({ layout: "xbox" });

/** Distribución que hay que pintar: la elegida a mano, o la detectada si está en `auto`. */
export function layoutActivo(): Layout {
  return prefs.gamepadLayout === "auto" ? mando.layout : prefs.gamepadLayout;
}

/** Aplica el tema guardado. Se llama al arrancar y en cada cambio. */
export function aplicarPrefsTema() {
  aplicarTema(buscarTema(prefs.theme, prefs.customThemes), prefs.accent);
}
