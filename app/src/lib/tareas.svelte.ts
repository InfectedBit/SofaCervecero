/**
 * Cola de tareas en segundo plano (guión J.0_1).
 *
 * El problema que resuelve: añadir una carpeta dejaba el diálogo esperando al escaneo y, al
 * terminar, refrescaba la interfaz — si el usuario había abierto otro menú, se lo cerraba.
 * Ahora el diálogo se cierra al instante y la tarea sigue por su cuenta, visible en la barra
 * superior y **cancelable**.
 */
import { api, type ScanProgress } from "./api";

export type EstadoTarea = "corriendo" | "ok" | "error" | "cancelada";

export type Tarea = {
  id: string;
  etiqueta: string;
  estado: EstadoTarea;
  progreso: ScanProgress | null;
  /** Resumen al terminar, o el mensaje de error. */
  detalle: string | null;
  /** Momento en que terminó, para que se desvanezca sola. */
  fin: number | null;
};

/** Cuánto se queda visible una tarea terminada antes de desaparecer. */
const VISIBLE_TRAS_TERMINAR = 6000;

export const tareas = $state<{ lista: Tarea[] }>({ lista: [] });

let contador = 0;

function nuevaId(prefijo: string): string {
  contador += 1;
  return `${prefijo}-${Date.now()}-${contador}`;
}

function buscar(id: string): Tarea | undefined {
  return tareas.lista.find((t) => t.id === id);
}

export function hayTareas(): boolean {
  return tareas.lista.some((t) => t.estado === "corriendo");
}

/** Actualiza el progreso de la tarea en curso (lo llaman los eventos del core). */
export function progresoTarea(id: string, p: ScanProgress | null) {
  const t = buscar(id);
  if (t && t.estado === "corriendo") t.progreso = p;
}

function terminar(id: string, estado: EstadoTarea, detalle: string | null) {
  const t = buscar(id);
  if (!t) return;
  t.estado = estado;
  t.detalle = detalle;
  t.progreso = null;
  t.fin = Date.now();
  // Se quita sola pasado un rato, pero solo si no ha vuelto a arrancar con ese id.
  setTimeout(() => {
    const actual = buscar(id);
    if (actual && actual.fin === t.fin) {
      tareas.lista = tareas.lista.filter((x) => x.id !== id);
    }
  }, VISIBLE_TRAS_TERMINAR);
}

/**
 * Lanza una tarea y la registra. `fn` recibe el id, que debe pasar al comando del core para
 * que la cancelación pueda llegar hasta él. Devuelve el id.
 *
 * No se espera al resultado a propósito: quien la lanza sigue con lo suyo.
 */
export function lanzarTarea(
  prefijo: string,
  etiqueta: string,
  fn: (id: string) => Promise<string>,
  alTerminar?: () => void,
): string {
  const id = nuevaId(prefijo);
  tareas.lista = [
    ...tareas.lista,
    { id, etiqueta, estado: "corriendo", progreso: null, detalle: null, fin: null },
  ];
  fn(id)
    .then((resumen) => terminar(id, "ok", resumen))
    .catch((e) => terminar(id, "error", String(e)))
    .finally(() => alTerminar?.());
  return id;
}

/** Pide al core que pare. La tarea termina ordenadamente y conserva lo ya hecho. */
export async function cancelarTarea(id: string) {
  const t = buscar(id);
  if (!t || t.estado !== "corriendo") return;
  t.etiqueta = `${t.etiqueta} · cancelando…`;
  try {
    await api.taskCancel(id);
  } catch {
    // Si el core ya había terminado, no hay nada que cancelar.
  }
}

export function quitarTarea(id: string) {
  tareas.lista = tareas.lista.filter((t) => t.id !== id);
}
