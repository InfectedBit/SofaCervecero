/**
 * Capa activa y navegación por foco dentro de diálogos (guión E.0_2).
 *
 * El problema que resuelve: al abrir un menú emergente el foco se quedaba en la card del
 * grid, así que las flechas del mando seguían llegando al grid **por detrás** del diálogo.
 * Ahora hay una sola regla: **manda el diálogo más alto**; si no hay ninguno, manda el grid.
 */

/** Controles que pueden recibir foco dentro de un diálogo. */
const SELECTOR_FOCUSABLES = [
  "button:not([disabled])",
  "input:not([disabled]):not([type=hidden])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "a[href]",
  '[tabindex]:not([tabindex="-1"])',
].join(",");

/**
 * Lo que puede quedarse la navegación: un diálogo o el menú contextual. El del clic derecho
 * cuenta igual que un diálogo — si no, las flechas se colaban al grid **por detrás** del menú,
 * que es exactamente el fallo que arregló E.0_2 para los emergentes.
 *
 * `alertdialog` está por el mismo motivo, y faltaba: la confirmación de pantalla del Modo Sofá
 * usa ese rol —es el correcto, una interrupción que exige respuesta— y al no contar como capa,
 * el mando navegaba el grid por detrás de ella. Había que coger el ratón para contestar algo
 * que solo aparece cuando estás lejos del teclado.
 */
const SELECTOR_CAPAS = '[role="dialog"],[role="alertdialog"],[role="menu"]';

/**
 * Contenedor que debe recibir la navegación: la capa más alta (la última en el DOM, que es la
 * que se pinta encima), o el grid si no hay ninguna abierta.
 */
export function capaActiva(): HTMLElement | null {
  const capas = document.querySelectorAll<HTMLElement>(SELECTOR_CAPAS);
  if (capas.length > 0) return capas[capas.length - 1];
  return document.querySelector<HTMLElement>(".grid-area");
}

/** ¿Hay algún menú emergente abierto? La barra de ayuda lo usa para cambiar de contexto. */
export function hayDialogo(): boolean {
  return document.querySelector(SELECTOR_CAPAS) !== null;
}

/**
 * Marca que se está navegando con el mando.
 *
 * Hace falta porque **`:focus-visible` no se activa con eventos sintéticos**: Chromium solo
 * lo aplica cuando cree que la última interacción fue de teclado, y las pulsaciones que
 * fabricamos desde el mando no cuentan como tal. Resultado: el foco sí se movía dentro de
 * los menús, pero **no se veía** — parecía que el mando no navegaba.
 *
 * Con esta marca en `<html>`, el CSS puede resaltar el `:focus` a secas mientras se usa el
 * mando, y volver a `:focus-visible` en cuanto se toca el ratón.
 */
export function marcarNavegacionMando() {
  document.documentElement.dataset.nav = "mando";
}

/** Instante del último movimiento de cursor provocado por el mando. */
let ultimoCursorMando = 0;

/**
 * Avisa de que el cursor lo está moviendo el mando, no la mano.
 *
 * Mover el puntero con `SetCursorPos` genera un `mousemove` **real**, indistinguible del de
 * un ratón físico. Sin esta marca, empujar el stick derecho apagaría al instante el resalte
 * de foco que acabamos de encender.
 */
export function marcarCursorMando() {
  ultimoCursorMando = performance.now();
}

/**
 * Acción para el **fondo** de un diálogo: cierra al pulsar fuera, pero solo si la pulsación
 * **empezó** fuera (guión A.0_4 §3).
 *
 * El problema: al seleccionar el texto de un campo y arrastrar el ratón fuera del recuadro, el
 * `click` se dispara sobre el fondo (porque cuenta dónde se **suelta** el botón) y el diálogo
 * se cerraba en plena edición, perdiendo lo escrito.
 *
 * La regla es la que pedía el usuario: **manda dónde empezó la pulsación**. Si empezó dentro,
 * soltar fuera no cierra nada.
 */
export function fondoModal(node: HTMLElement, alCerrar: () => void) {
  let empezoFuera = false;

  const abajo = (e: MouseEvent) => {
    empezoFuera = e.target === node;
  };
  const arriba = (e: MouseEvent) => {
    // Tiene que cumplirse en los dos extremos: empezó en el fondo y termina en el fondo.
    if (empezoFuera && e.target === node) alCerrar();
    empezoFuera = false;
  };

  node.addEventListener("mousedown", abajo);
  node.addEventListener("mouseup", arriba);
  return {
    destroy() {
      node.removeEventListener("mousedown", abajo);
      node.removeEventListener("mouseup", arriba);
    },
  };
}

/**
 * Lo mismo para un desplegable que se cierra al pulsar fuera (filtros, menús): solo cuenta
 * si la pulsación **empezó y terminó** fuera del elemento.
 */
export function pulsacionFuera(node: HTMLElement, alCerrar: () => void) {
  let empezoFuera = false;
  const dentro = (e: Event) => node.contains(e.target as Node);

  const abajo = (e: MouseEvent) => {
    empezoFuera = !dentro(e);
  };
  const arriba = (e: MouseEvent) => {
    if (empezoFuera && !dentro(e)) alCerrar();
    empezoFuera = false;
  };

  window.addEventListener("mousedown", abajo, true);
  window.addEventListener("mouseup", arriba, true);
  return {
    destroy() {
      window.removeEventListener("mousedown", abajo, true);
      window.removeEventListener("mouseup", arriba, true);
    },
  };
}

/** Empieza a escuchar el ratón para quitar la marca. Devuelve la función de limpieza. */
export function escucharRaton(): () => void {
  const quitar = () => {
    if (performance.now() - ultimoCursorMando < 250) return; // lo movió el mando
    if (document.documentElement.dataset.nav === "mando") {
      delete document.documentElement.dataset.nav;
    }
  };
  window.addEventListener("mousemove", quitar, { passive: true });
  window.addEventListener("mousedown", quitar, { passive: true });
  return () => {
    window.removeEventListener("mousemove", quitar);
    window.removeEventListener("mousedown", quitar);
  };
}

/** ¿Este elemento tiene scroll propio y le queda recorrido? */
function desplazable(el: HTMLElement): boolean {
  if (el.scrollHeight <= el.clientHeight) return false;
  const overflow = getComputedStyle(el).overflowY;
  return overflow === "auto" || overflow === "scroll";
}

/**
 * Desplaza el panel que corresponda, como haría la rueda del ratón (stick izquierdo).
 *
 * Busca el contenedor con scroll **más cercano al foco** hacia arriba: dentro de un diálogo
 * puede haber varios (la lista de categorías, la de fuentes, el propio diálogo), y desplazar
 * el de fuera cuando el foco está en una lista interna se siente roto.
 */
export function desplazar(dy: number) {
  const capa = capaActiva();
  if (!capa) return;
  const activo = document.activeElement as HTMLElement | null;
  let el: HTMLElement | null = activo && capa.contains(activo) ? activo : capa;
  while (el) {
    if (desplazable(el)) {
      el.scrollBy({ top: dy });
      return;
    }
    if (el === capa) break;
    el = el.parentElement;
  }
  // Nadie con scroll propio: el contenedor de la capa (o la página) manda.
  (capa.scrollHeight > capa.clientHeight ? capa : document.scrollingElement)?.scrollBy({
    top: dy,
  });
}

function visible(el: HTMLElement): boolean {
  return el.offsetParent !== null || el.getClientRects().length > 0;
}

function focusables(raiz: HTMLElement): HTMLElement[] {
  return Array.from(raiz.querySelectorAll<HTMLElement>(SELECTOR_FOCUSABLES)).filter(visible);
}

/**
 * ¿El control usa las flechas horizontales para lo suyo? Un campo de texto mueve el cursor,
 * un desplegable cambia de opción y un deslizador cambia de valor: ahí izquierda/derecha
 * **no** deben mover el foco. Arriba/abajo siempre navegan.
 */
function usaHorizontal(el: Element | null): boolean {
  if (!el) return false;
  const tag = el.tagName;
  if (tag === "TEXTAREA" || tag === "SELECT") return true;
  if (tag !== "INPUT") return false;
  const tipo = (el as HTMLInputElement).type;
  return !["checkbox", "radio", "button", "submit", "file"].includes(tipo);
}

/**
 * Acción de Svelte para la raíz de un diálogo. Hace tres cosas:
 * 1. lleva el foco dentro al abrirse (y lo devuelve al cerrarse),
 * 2. navega entre sus controles con las flechas,
 * 3. **corta la propagación** de las teclas de navegación, para que nunca lleguen al grid.
 */
export function dialogo(node: HTMLElement) {
  const anterior = document.activeElement as HTMLElement | null;

  // El elemento con `autofocus` manda; si no, el primer control útil.
  const inicial =
    node.querySelector<HTMLElement>("[autofocus]") ?? focusables(node)[0] ?? node;
  // `setTimeout` para que ocurra tras el montaje del contenido del diálogo.
  const t = setTimeout(() => inicial.focus(), 0);

  function onKeydown(e: KeyboardEvent) {
    const nav = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Home", "End"];
    if (!nav.includes(e.key)) return;

    const activo = document.activeElement as HTMLElement | null;
    const dentro = !!activo && node.contains(activo) && activo !== node;

    const horizontal = e.key === "ArrowLeft" || e.key === "ArrowRight";
    if (dentro && horizontal && usaHorizontal(activo)) {
      // Deja que el control haga lo suyo: en fase de captura, cortar aquí impediría que la
      // tecla llegase al campo. No hay riesgo de que alcance el grid — los diálogos se
      // pintan fuera de `.app`, así que no comparten ancestro con `.grid-area`.
      return;
    }

    const items = focusables(node);
    if (items.length === 0) return;
    const i = dentro ? items.indexOf(activo!) : -1;
    let destino: number;
    if (e.key === "Home") destino = 0;
    else if (e.key === "End") destino = items.length - 1;
    // Si el foco se había quedado fuera (p. ej. en una card del grid), la primera
    // pulsación entra al diálogo en vez de perderse.
    else if (i < 0) destino = 0;
    else {
      const paso = e.key === "ArrowDown" || e.key === "ArrowRight" ? 1 : -1;
      // Circular: desde el último, la siguiente vuelve al primero.
      destino = (i + paso + items.length) % items.length;
    }
    e.preventDefault();
    e.stopPropagation();
    items[destino].focus();
    // Los diálogos altos tienen zonas con scroll propio (categorías, fuentes, temas).
    items[destino].scrollIntoView({ block: "nearest" });
  }

  /** Trampa de Tab: con el diálogo abierto no se sale a la ventana de detrás. */
  function onTab(e: KeyboardEvent) {
    if (e.key !== "Tab") return;
    const items = focusables(node);
    if (items.length === 0) return;
    const i = items.indexOf(document.activeElement as HTMLElement);
    const ultimo = items.length - 1;
    if (!e.shiftKey && i === ultimo) {
      e.preventDefault();
      items[0].focus();
    } else if (e.shiftKey && i <= 0) {
      e.preventDefault();
      items[ultimo].focus();
    }
  }

  // En fase de captura: así la navegación se resuelve aquí aunque un control interno
  // (un `<select>`, un campo de texto) quisiera quedarse la tecla.
  node.addEventListener("keydown", onKeydown, true);
  node.addEventListener("keydown", onTab);

  return {
    destroy() {
      clearTimeout(t);
      node.removeEventListener("keydown", onKeydown, true);
      node.removeEventListener("keydown", onTab);
      // Devuelve el foco a donde estaba: al cerrar el detalle se vuelve a su card.
      if (anterior && document.contains(anterior)) anterior.focus();
    },
  };
}
