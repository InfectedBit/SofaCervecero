/**
 * Estado del **Modo Sofá** (guión E.0_4 fase 2).
 *
 * De momento vive aquí, en la interfaz, y no en el core. Es deliberado para esta fase: hasta que
 * el modo no tenga que **sobrevivir al cierre de la ventana** —que es lo que pasará cuando el
 * hub vuelva solo al salir de un juego, fase 7— no hay nada que persistir salvo la pantalla
 * elegida, y esa sí va al core porque hará falta con la ventana cerrada.
 *
 * La regla que no hay que olvidar cuando llegue ese momento: `localStorage` se vuelca en diferido
 * y cerrar el hub destruye el WebView2, así que **no vale para nada que deba sobrevivir al
 * cierre** (ver `H.0_3` §2.2).
 */
import { api, type Reparto } from "./api";

/** Clave en el core: la pantalla elegida la última vez. */
const CLAVE_MONITOR = "sofa.monitor";

export const sofa = $state<{
  /** El modo está activo: tipografía grande, interfaz podada. */
  activo: boolean;
  /** Además, la ventana está a pantalla completa. F11 lo alterna sin salir del modo. */
  pantallaCompleta: boolean;
  /** `id` de la pantalla elegida (`\\.\DISPLAY3`), o `null` si manda la principal. */
  monitor: string | null;
  /** Qué se le hizo al escritorio al entrar. Hay que deshacerlo al salir. */
  repartoAplicado: Reparto;
  /** Hay una cuenta atrás en marcha: sin confirmar, el núcleo lo deshará solo. */
  esperandoConfirmar: boolean;
}>({
  activo: false,
  pantallaCompleta: false,
  monitor: null,
  repartoAplicado: "ninguna",
  esperandoConfirmar: false,
});

/** Recupera la última pantalla elegida. Se llama al arrancar. */
export async function cargarMonitorSofa() {
  sofa.monitor = (await api.settingGet(CLAVE_MONITOR)) || null;
}

/**
 * Entra en el modo y lleva la ventana al monitor indicado.
 *
 * Si algo falla al colocar la ventana **no se entra en el modo**: una interfaz de sofá en una
 * ventana pequeña del monitor de trabajo no es un estado útil, es un fallo a medias.
 */
export async function entrarSofa(
  monitor: string | null,
  reparto: Reparto = "ninguna",
): Promise<string | null> {
  // El reparto va **antes** de colocar la ventana: si se cambia la principal o se apagan
  // pantallas después, la ventana ya colocada se descoloca sola (guión E.0_4 §5.4).
  if (reparto !== "ninguna" && monitor) {
    try {
      await api.sofaAplicar(reparto, monitor);
      sofa.repartoAplicado = reparto;
      sofa.esperandoConfirmar = true;
    } catch (e) {
      return String(e);
    }
  }
  try {
    await api.sofaSet(true, monitor);
  } catch (e) {
    // La ventana no se ha podido colocar: se deshace también lo del escritorio, que si no
    // quedaría cambiado sin que el modo llegue a existir.
    if (sofa.repartoAplicado !== "ninguna") await api.sofaRestaurar().catch(() => []);
    sofa.repartoAplicado = "ninguna";
    sofa.esperandoConfirmar = false;
    return String(e);
  }
  sofa.monitor = monitor;
  sofa.activo = true;
  sofa.pantallaCompleta = true;
  if (monitor) await api.settingSet(CLAVE_MONITOR, monitor);
  return null;
}

/** «Sí, se ve bien.» Para la cuenta atrás del núcleo. */
export async function confirmarSofa() {
  await api.sofaConfirmar().catch(() => {});
  sofa.esperandoConfirmar = false;
}

/** Lo contrario: deshacer ya, sin esperar a que venza la cuenta atrás. */
export async function revertirReparto() {
  await api.sofaRestaurar().catch(() => []);
  sofa.repartoAplicado = "ninguna";
  sofa.esperandoConfirmar = false;
}

export async function salirSofa() {
  try {
    await api.sofaSet(false, null);
  } catch {
    // Si la ventana ya no está a pantalla completa, da igual: lo que importa es salir del modo.
  }
  // El escritorio se devuelve siempre, haya o no cambiado algo: `sofaRestaurar` es idempotente.
  await api.sofaRestaurar().catch(() => []);
  sofa.repartoAplicado = "ninguna";
  sofa.esperandoConfirmar = false;
  sofa.activo = false;
  sofa.pantallaCompleta = false;
}

/** F11: alterna la pantalla completa **sin** salir del modo, como en cualquier otra app. */
export async function alternarPantallaCompleta() {
  const quiere = !sofa.pantallaCompleta;
  try {
    await api.sofaSet(quiere, quiere ? sofa.monitor : null);
    sofa.pantallaCompleta = quiere;
  } catch {
    /* sin cambios */
  }
}
