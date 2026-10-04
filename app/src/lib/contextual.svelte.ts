/**
 * Menú contextual propio para toda la app (guión A.0_4 §4), con **submenús** (F.0_6 §3.2).
 *
 * El del navegador no sirve: no tiene nuestras acciones, no sigue el tema y no se puede
 * manejar con el mando. Este se abre donde esté el cursor, se navega con las flechas y solo
 * aparece **donde hay algo que ofrecer** — en el resto de la app no se abre nada.
 *
 * Los submenús son **páginas**, no paneles flotantes al lado. Dos razones: un panel lateral
 * con quince categorías se sale de la pantalla y hay que calcular dónde cabe, y sobre todo
 * **con el mando no hay «pasar el ratón por encima»** — en una app que se usa desde el sofá,
 * entrar y volver con A y B es lo que funciona.
 */

export type OpcionContextual = {
  etiqueta: string;
  /** Emoji o símbolo corto a la izquierda. */
  icono?: string;
  /** Separador antes de esta opción. */
  separar?: boolean;
  peligro?: boolean;
  /** Rótulo no pulsable, para agrupar (los ejes de categorías). */
  cabecera?: boolean;
  /** Se pinta con el color de acento: ya está puesta (p. ej. el juego está en esa categoría). */
  activa?: boolean;
  /**
   * No cierra el menú al pulsarla. Es lo que permite marcar **varias seguidas** sin tener que
   * volver a abrirlo, y tras cada pulsación se repinta la página para que el acento se vea.
   */
  mantener?: boolean;
  /** Si está, al pulsarla se **entra** en esta página en vez de ejecutar `accion`. */
  submenu?: () => OpcionContextual[];
  accion?: () => void;
};

/** Una página del menú. La pila permite entrar en submenús y volver. */
type Nivel = {
  titulo: string;
  opciones: OpcionContextual[];
  /** Cómo regenerar esta página; hace falta para repintar tras una opción `mantener`. */
  recargar?: () => OpcionContextual[];
};

export const contextual = $state<{
  abierto: boolean;
  x: number;
  y: number;
  pila: Nivel[];
}>({ abierto: false, x: 0, y: 0, pila: [] });

/** Abre el menú en la posición del evento. Ignora la lista vacía. */
export function abrirContextual(e: MouseEvent, titulo: string, opciones: OpcionContextual[]) {
  if (opciones.length === 0) return;
  e.preventDefault();
  e.stopPropagation();
  contextual.x = e.clientX;
  contextual.y = e.clientY;
  contextual.pila = [{ titulo, opciones }];
  contextual.abierto = true;
}

/** Entra en un submenú. `construir` se guarda para poder repintarlo. */
export function entrarSubmenu(titulo: string, construir: () => OpcionContextual[]) {
  contextual.pila = [
    ...contextual.pila,
    { titulo, opciones: construir(), recargar: construir },
  ];
}

/** Vuelve a la página anterior. Devuelve `false` si ya estaba en la primera. */
export function volverContextual(): boolean {
  if (contextual.pila.length <= 1) return false;
  contextual.pila = contextual.pila.slice(0, -1);
  return true;
}

/** Repinta la página actual: tras marcar una categoría, el acento tiene que cambiar. */
export function refrescarContextual() {
  const nivel = contextual.pila[contextual.pila.length - 1];
  if (!nivel?.recargar) return;
  contextual.pila = [
    ...contextual.pila.slice(0, -1),
    { ...nivel, opciones: nivel.recargar() },
  ];
}

export function cerrarContextual() {
  contextual.abierto = false;
  contextual.pila = [];
}
