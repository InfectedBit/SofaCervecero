/**
 * Confirmaciones propias, en lugar del `confirm()` del navegador (guión A.0_4 §2).
 *
 * Tres razones para no usar el nativo:
 * 1. **No se puede manejar con el mando.** Es una ventana del sistema, fuera del WebView:
 *    justo lo contrario de una app que se usa desde el sofá.
 * 2. **No sigue el tema.**
 * 3. En este WebView `confirm()` **no devolvía un booleano**, sino una promesa, así que
 *    `if (!confirm(...))` nunca entraba y el valor acababa llegando al core como un objeto
 *    — de ahí el error *"invalid type: map, expected a boolean"*.
 */

export type Peticion = {
  titulo: string;
  mensaje?: string;
  /** Texto del botón que confirma. */
  aceptar?: string;
  cancelar?: string;
  /** Pinta el botón de aceptar como destructivo. */
  peligro?: boolean;
  resolver: (ok: boolean) => void;
};

export const confirmacion = $state<{ actual: Peticion | null }>({ actual: null });

/**
 * Pregunta y espera respuesta. Devuelve siempre un booleano de verdad.
 *
 * ```ts
 * if (await confirmar({ titulo: "¿Eliminar?", peligro: true })) { … }
 * ```
 */
export function confirmar(opciones: Omit<Peticion, "resolver">): Promise<boolean> {
  return new Promise((resolver) => {
    confirmacion.actual = { ...opciones, resolver };
  });
}

export function responder(ok: boolean) {
  const p = confirmacion.actual;
  confirmacion.actual = null;
  p?.resolver(ok);
}
