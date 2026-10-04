/**
 * Etiquetas del panel de **Vista** (guión F.0_5, antes F.0_3). En un módulo aparte porque las
 * usan la barra de filtros activos, el panel y, más adelante, los filtros rápidos del Modo
 * Sofá: conviene que digan lo mismo en los tres sitios.
 */

/** Origen = `kind` de la fuente de la que salió el juego. */
export const ORIGEN_LABEL: Record<string, string> = {
  steam: "Steam",
  folder_library: "Carpeta de juegos",
  app_entry: "App añadida a mano",
};

/**
 * Criterios de orden. La **dirección va aparte** (guión F.0_5 §2): antes iban pegados
 * —«Más jugados» ya significaba descendente— y no había forma de pedir el orden inverso.
 *
 * `desc` es la dirección **natural** del criterio: al elegirlo se pone esa, porque nadie
 * busca «el juego que menos he jugado» a la primera. El título es la excepción: sube.
 */
export const ORDEN: {
  id: string;
  texto: string;
  desc: boolean;
  /** Cómo se lee cada dirección, para que el botón diga algo útil y no «asc/desc». */
  asc: string;
  descTexto: string;
}[] = [
  { id: "title", texto: "Título", desc: false, asc: "A → Z", descTexto: "Z → A" },
  {
    id: "recent",
    texto: "Última vez jugado",
    desc: true,
    asc: "Primero los que hace más",
    descTexto: "Primero los recientes",
  },
  {
    id: "playtime",
    texto: "Tiempo jugado",
    desc: true,
    asc: "Primero los menos jugados",
    descTexto: "Primero los más jugados",
  },
  {
    id: "players",
    texto: "Nº de jugadores",
    desc: true,
    asc: "Primero los de menos",
    descTexto: "Primero los de más",
  },
  {
    id: "added",
    texto: "Añadido a la biblioteca",
    desc: true,
    asc: "Primero los de siempre",
    descTexto: "Primero los últimos",
  },
];

export function buscarOrden(id: string | null | undefined) {
  return ORDEN.find((o) => o.id === id) ?? ORDEN[0];
}

/** Texto corto del orden activo, para el *chip* de la barra de filtros. */
export function textoOrden(id: string | null | undefined, desc: boolean): string {
  const o = buscarOrden(id);
  return `${o.texto} · ${desc ? o.descTexto : o.asc}`;
}

/** Opciones del desplegable de nº de jugadores. Es el filtro más usado: se juega en el sofá. */
export const JUGADORES = [
  { valor: 2, texto: "2 o más (coop / versus)" },
  { valor: 3, texto: "3 o más" },
  { valor: 4, texto: "4 o más" },
  { valor: 6, texto: "6 o más" },
];

/**
 * El tiempo jugado, en **un solo desplegable** (guión T.0_2 §2).
 *
 * Ya había un filtro de «jugado / sin jugar» y lo que faltaba era poder pedir por horas. Van
 * juntos a propósito, en vez de añadir un segundo control al lado: son la **misma dimensión**, y
 * con dos controles se puede pedir «sin jugar» *y* «más de 20 h» a la vez — una combinación que
 * nunca devuelve nada y que no se entiende hasta que miras los dos desplegables.
 *
 * Cada opción fija los tres campos, también los que no usa. Así el backend no sabe de bandas:
 * solo compone `played`, `min_hours` y `max_hours`, y cualquier banda nueva es una línea aquí.
 *
 * `max` es **exclusivo**, para que dos bandas contiguas no devuelvan dos veces el juego que cae
 * justo en la frontera.
 */
export const TIEMPO: {
  id: string;
  texto: string;
  /** Texto del *chip* de la barra de filtros activos; más corto que el del desplegable. */
  chip: string;
  played: boolean | null;
  min: number | null;
  max: number | null;
}[] = [
  { id: "sin", texto: "Todavía sin jugar", chip: "Sin jugar", played: false, min: null, max: null },
  { id: "con", texto: "Ya jugados", chip: "Ya jugados", played: true, min: null, max: null },
  { id: "-1", texto: "Jugados menos de 1 h", chip: "< 1 h", played: true, min: null, max: 1 },
  { id: "1-5", texto: "Entre 1 y 5 h", chip: "1 – 5 h", played: true, min: 1, max: 5 },
  { id: "5-20", texto: "Entre 5 y 20 h", chip: "5 – 20 h", played: true, min: 5, max: 20 },
  { id: "20-50", texto: "Entre 20 y 50 h", chip: "20 – 50 h", played: true, min: 20, max: 50 },
  { id: "+50", texto: "Más de 50 h", chip: "> 50 h", played: true, min: 50, max: null },
];

/** Los tres campos del filtro de tiempo, tal y como viajan al núcleo. */
export type Tiempo = { played: boolean | null; min_hours: number | null; max_hours: number | null };

/** Qué opción de `TIEMPO` describe el filtro actual. `null` = indiferente. */
export function tiempoActivo(f: Tiempo) {
  if (f.played == null && f.min_hours == null && f.max_hours == null) return null;
  return (
    TIEMPO.find(
      (t) =>
        t.played === (f.played ?? null) &&
        t.min === (f.min_hours ?? null) &&
        t.max === (f.max_hours ?? null),
    ) ?? null
  );
}

/** Los tres campos que hay que enviar al elegir una opción (o limpiarla). */
export function tiempoFiltro(id: string): Tiempo {
  const t = TIEMPO.find((x) => x.id === id);
  return t
    ? { played: t.played, min_hours: t.min, max_hours: t.max }
    : { played: null, min_hours: null, max_hours: null };
}
