/**
 * Ayudas para recorrer el árbol de categorías (guión F.0_6).
 *
 * El core devuelve la lista **ya ordenada** en recorrido en profundidad —cada padre seguido
 * de sus hijas—, así que aquí nadie vuelve a ordenar: solo se agrupa.
 */

import type { Category } from "./api";

/** Los ejes: categorías sin padre (*Clientes*, *Nº Jugadores*, *TAGS*…). */
export function raices(cats: Category[]): Category[] {
  return cats.filter((c) => c.parent_id === null);
}

/** Hijas directas de una categoría, en su orden. */
export function hijasDe(cats: Category[], id: number): Category[] {
  return cats.filter((c) => c.parent_id === id);
}

/** Nombre del padre, o `null` si es un eje. Para enseñar «TAGS → Coop». */
export function nombrePadre(cats: Category[], c: Category): string | null {
  return cats.find((x) => x.id === c.parent_id)?.name ?? null;
}

/**
 * El árbol listo para pintar: cada eje con sus hijas. Las categorías sueltas (sin padre y sin
 * hijas) van juntas al final, bajo `SUELTAS`, para no dejarlas huérfanas en la interfaz.
 */
export const SUELTAS = "Sin eje";

export function porEje(cats: Category[]): { eje: Category | null; hijas: Category[] }[] {
  const salida: { eje: Category | null; hijas: Category[] }[] = [];
  const sueltas: Category[] = [];
  for (const r of raices(cats)) {
    const h = hijasDe(cats, r.id);
    if (h.length > 0) salida.push({ eje: r, hijas: h });
    else sueltas.push(r);
  }
  if (sueltas.length > 0) salida.push({ eje: null, hijas: sueltas });
  return salida;
}
