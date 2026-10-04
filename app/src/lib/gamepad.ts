/**
 * Mando (guiones E.0_1 y E.0_2). Traduce el mando a las **mismas teclas** que ya navegan la
 * interfaz, así que no hay una segunda lógica de navegación que mantener.
 *
 * Se usa la **Gamepad API del WebView**, no una dependencia de Rust:
 * - Chromium ya soporta XInput (Xbox) y DirectInput/HID (DualShock, DualSense); el Steam
 *   Controller se presenta como mando estándar cuando Steam está activo.
 * - El mando solo hace falta con el hub **abierto**, que es justo cuando existe el WebView:
 *   así el core residente no paga nada por esto (requisito de ultraeficiencia).
 *
 * *Limitación conocida:* no sirve para **abrir** el hub estando cerrado — eso necesita un
 * escucha en el core (punto E6 del backlog, con `gilrs`).
 */
import { capaActiva } from "./focus";

/** Acciones remapeables. La navegación (cruceta/stick izquierdo) no se remapea. */
export type AccionMando =
  | "detalle"
  | "lanzar"
  | "atras"
  | "buscar"
  | "categoria_anterior"
  | "categoria_siguiente"
  | "guia"
  | "ajustes"
  | "escanear"
  | "clic"
  | "clic_derecho"
  | "menu";

/** Orden en que se muestran en la guía y en Ajustes. */
export const ACCIONES: { id: AccionMando; etiqueta: string; ayuda: string }[] = [
  { id: "detalle", etiqueta: "Opciones del juego", ayuda: "Abre la ficha del juego" },
  { id: "lanzar", etiqueta: "Lanzar juego", ayuda: "Lo inicia sin abrir la ficha" },
  { id: "atras", etiqueta: "Atrás / cerrar", ayuda: "Cierra el menú abierto" },
  { id: "buscar", etiqueta: "Buscar", ayuda: "Salta al buscador" },
  { id: "categoria_anterior", etiqueta: "Categoría anterior", ayuda: "Sube en la barra lateral" },
  { id: "categoria_siguiente", etiqueta: "Categoría siguiente", ayuda: "Baja en la barra lateral" },
  { id: "clic", etiqueta: "Clic izquierdo", ayuda: "Con el cursor del stick derecho" },
  { id: "clic_derecho", etiqueta: "Clic derecho", ayuda: "Clic derecho real, donde esté el cursor" },
  { id: "menu", etiqueta: "Menú contextual", ayuda: "Sobre lo que esté enfocado, sin usar el cursor" },
  { id: "guia", etiqueta: "Guía de botones", ayuda: "Muestra este mapa" },
  { id: "ajustes", etiqueta: "Ajustes", ayuda: "Abre las opciones de la app" },
  { id: "escanear", etiqueta: "Reescanear", ayuda: "Vuelve a escanear las fuentes" },
];

/** Distribución de botones del mando: cambia etiquetas y colores, no los índices. */
export type Layout = "xbox" | "playstation";

/**
 * Etiquetas por índice del *Standard Gamepad* del W3C. Los índices son los mismos en los dos
 * mandos; lo que cambia es cómo se llaman los botones serigrafiados.
 */
const ETIQUETAS: Record<Layout, Record<number, string>> = {
  xbox: {
    0: "A", 1: "B", 2: "X", 3: "Y",
    4: "LB", 5: "RB", 6: "LT", 7: "RT",
    8: "Select", 9: "Start", 10: "L3", 11: "R3", 16: "Guía",
  },
  playstation: {
    0: "✕", 1: "○", 2: "□", 3: "△",
    4: "L1", 5: "R1", 6: "L2", 7: "R2",
    8: "Share", 9: "Options", 10: "L3", 11: "R3", 16: "PS",
  },
};

/**
 * Color serigrafiado de los cuatro botones de acción. Es el de cada mando real, así que en
 * PlayStation la ✕ es azul y el ○ rojo, mientras que en Xbox la A es verde y la X azul.
 */
const COLORES: Record<Layout, Record<number, string>> = {
  xbox: { 0: "#4ade80", 1: "#f87171", 2: "#60a5fa", 3: "#fbbf24" },
  playstation: { 0: "#60a5fa", 1: "#f87171", 2: "#f0a5d0", 3: "#4ade80" },
};

export function nombreBoton(i: number | null | undefined, layout: Layout = "xbox"): string {
  if (i === null || i === undefined) return "—";
  return ETIQUETAS[layout][i] ?? `Botón ${i}`;
}

/** Color del botón, o `null` si no es uno de los cuatro de acción. */
export function colorBoton(i: number | null | undefined, layout: Layout = "xbox"): string | null {
  if (i === null || i === undefined) return null;
  return COLORES[layout][i] ?? null;
}

/**
 * Deduce la distribución a partir del identificador que da el navegador. Sony usa el id de
 * fabricante `054c`; los DualShock/DualSense también se anuncian por nombre.
 */
export function detectarLayout(id: string): Layout {
  const s = id.toLowerCase();
  const sony = /054c|dualsense|dualshock|playstation|\bps[345]\b|wireless controller/.test(s);
  return sony ? "playstation" : "xbox";
}

/**
 * Una asignación puede ser **un botón o una combinación**. Lo guardado antiguo son números
 * sueltos, así que se normaliza al leer en vez de migrar nada.
 */
export type Asignacion = number | number[];

export function comoLista(v: Asignacion | null | undefined): number[] {
  if (v === null || v === undefined) return [];
  return Array.isArray(v) ? v.filter((n) => Number.isInteger(n)) : [v];
}

/** `LB + RB`, o `Select`. Para enseñar la asignación en Ajustes y en la guía. */
export function nombreAsignacion(v: Asignacion | null | undefined, layout: Layout): string {
  const l = comoLista(v);
  if (l.length === 0) return "—";
  return l.map((i) => ETIQUETAS[layout][i] ?? `#${i}`).join(" + ");
}

/**
 * Mapa por defecto: **Start abre Ajustes**, **Select muestra la guía**, y los gatillos son los
 * **dos clics del ratón** — RT el izquierdo y **LT el derecho**, que es donde los espera
 * cualquiera que haya usado un mando.
 *
 * LT estuvo un tiempo asignado al reescaneo. Fue un error: el clic derecho **se daba por hecho
 * desde `E.0_3`** y en realidad nunca se había implementado —solo existía el menú contextual
 * sobre lo enfocado, que es otra cosa—. El reescaneo se va a **R3**, que estaba libre.
 */
export const MAPA_POR_DEFECTO: Record<AccionMando, Asignacion> = {
  detalle: 0, // A / ✕
  atras: 1, // B / ○
  lanzar: 2, // X / □
  buscar: 3, // Y / △
  categoria_anterior: 4, // LB / L1
  categoria_siguiente: 5, // RB / R1
  clic_derecho: 6, // LT / L2
  clic: 7, // RT / R2
  guia: 8, // Select / Share
  ajustes: 9, // Start / Options
  menu: 10, // L3
  escanear: 11, // R3
};

type Opciones = {
  /** Mapa activo (de las preferencias). Se lee en cada sondeo: los cambios son inmediatos. */
  mapa: () => Record<AccionMando, Asignacion>;
  /** Se llama con la tecla equivalente, para reutilizar la navegación por foco. */
  tecla: (key: string, ctrl?: boolean) => void;
  accion: (a: AccionMando) => void;
  /**
   * Botón que **se reserva el núcleo** y que este sondeo no debe tocar (guión E.0_4 §9.1).
   * Devuelve `-1` si no hay ninguno.
   */
  reservado?: () => number;
  conexion?: (conectado: boolean, nombre: string, layout: Layout) => void;
  /**
   * Modo captura para remapear. Recibe **la combinación** pulsada —uno o varios botones—
   * cuando se sueltan todos, y se desactiva solo.
   */
  captura?: () => ((botones: number[]) => void) | null;
  /** Stick izquierdo: desplaza el panel como la rueda del ratón, en píxeles. */
  desplazar?: (dy: number) => void;
  /** Stick derecho: desplazamiento del cursor del sistema, en píxeles. */
  cursor?: (dx: number, dy: number) => void;
  /** Flancos del botón de clic (los dos, para poder arrastrar). */
  clic?: (pulsado: boolean) => void;
  /** Lo mismo con el clic **derecho**, que hasta ahora no existía (solo el menú contextual). */
  clicDerecho?: (pulsado: boolean) => void;
};

const REPETICION_INICIAL = 420; // ms antes de empezar a repetir al mantener
const REPETICION = 110; // ms entre repeticiones
/** Zona muerta de los sticks: por debajo, se consideran centrados. */
const ZONA_MUERTA = 0.18;
/** Píxeles por segundo con el stick a tope. */
const VELOCIDAD_CURSOR = 1100;
const VELOCIDAD_SCROLL = 1500;

/**
 * Arranca el bucle de sondeo. Devuelve la función para pararlo.
 * `requestAnimationFrame` solo corre mientras haya un mando conectado, para no gastar
 * ciclos con el hub abierto y sin mando.
 */
export function iniciarMando(op: Opciones): () => void {
  let raf = 0;
  let activo = false;
  let ultimoFrame = performance.now();
  /** Restos fraccionarios del cursor (ver abajo). */
  let restoX = 0;
  let restoY = 0;
  let restoScroll = 0;
  /** Por control: instante en que toca la siguiente repetición (o ausente si está suelto). */
  const pulsado = new Map<string, number>();
  /** Botones acumulados durante una captura de combinación. */
  const acumulado: number[] = [];

  /**
   * Auto-repetición al mantener: dispara al pulsar, espera `REPETICION_INICIAL` y luego
   * repite cada `REPETICION`. Se guarda el **próximo instante** en vez de calcularlo con un
   * módulo sobre el tiempo transcurrido, que con frames de ~16 ms se salta o duplica pulsos.
   */
  function repetir(id: string, ahora: number, activado: boolean, fn: () => void) {
    if (!activado) {
      pulsado.delete(id);
      return;
    }
    const proximo = pulsado.get(id);
    if (proximo === undefined) {
      pulsado.set(id, ahora + REPETICION_INICIAL);
      fn();
    } else if (ahora >= proximo) {
      pulsado.set(id, ahora + REPETICION);
      fn();
    }
  }

  function sondear() {
    const mandos = navigator.getGamepads?.() ?? [];
    const gp = Array.from(mandos).find((g): g is Gamepad => !!g && g.connected);
    if (!gp) {
      activo = false;
      return; // sin mando: se para el bucle hasta el próximo `gamepadconnected`
    }
    const ahora = performance.now();
    const b = (i: number) => gp.buttons[i]?.pressed ?? false;
    const eje = (i: number) => gp.axes[i] ?? 0;

    const pulsacion = (id: string, activado: boolean, fn: () => void) => {
      const antes = pulsado.has(id);
      if (activado && !antes) {
        pulsado.set(id, ahora);
        fn();
      } else if (!activado && antes) {
        pulsado.delete(id);
      }
    };

    // Modo captura (remapeo en Ajustes): el primer botón que se pulse se asigna y ya.
    // ── Captura para remapear ────────────────────────────────────────────────────────
    //
    // Admite **combinaciones**, no solo un botón: se acumula lo que se vaya pulsando y se
    // confirma **al soltarlo todo**. Así `LB`+`RB` es una asignación y no dos seguidas, que
    // es como funciona el remapeo de cualquier juego.
    const capturar = op.captura?.();
    if (capturar) {
      const ahoraPulsados: number[] = [];
      for (let i = 0; i < gp.buttons.length; i++) if (b(i)) ahoraPulsados.push(i);
      if (ahoraPulsados.length > 0) {
        for (const i of ahoraPulsados) if (!acumulado.includes(i)) acumulado.push(i);
      } else if (acumulado.length > 0) {
        const combo = [...acumulado];
        acumulado.length = 0;
        capturar(combo);
      }
      raf = requestAnimationFrame(sondear);
      return;
    }
    acumulado.length = 0;

    // **Solo la cruceta** mueve el foco entre opciones y paneles. La recibe la capa activa
    // (el diálogo abierto, o el grid).
    repetir("arriba", ahora, b(12), () => op.tecla("ArrowUp"));
    repetir("abajo", ahora, b(13), () => op.tecla("ArrowDown"));
    repetir("izq", ahora, b(14), () => op.tecla("ArrowLeft"));
    repetir("der", ahora, b(15), () => op.tecla("ArrowRight"));

    // Botones de acción: sin repetición (una pulsación = una acción).
    const mapa = op.mapa();
    const reservado = op.reservado?.() ?? -1;

    // Las **combinaciones van primero**: si `LB`+`RB` está asignada a algo, pulsarlas no debe
    // disparar además lo que tenga `LB` suelto. Los botones que consume una combinación activa
    // quedan marcados y las asignaciones de un solo botón los respetan.
    const entradas = (Object.entries(mapa) as [AccionMando, number | number[]][])
      .map(([accion, v]) => [accion, comoLista(v)] as const)
      .filter(([accion]) => accion !== "clic" && accion !== "clic_derecho")
      .sort((a, b2) => b2[1].length - a[1].length);

    const consumidos = new Set<number>();
    for (const [accion, botones] of entradas) {
      if (botones.length === 0) continue;
      // El botón que lleva el núcleo se salta aquí **entero**. Si no, su acción saltaría al
      // pulsar y se solaparía con el gesto de mantener: se abriría la guía nada más empezar a
      // mantener (guión E.0_4 §9.1). El núcleo avisa al soltar, y entonces se ejecuta.
      if (botones.length === 1 && botones[0] === reservado) continue;

      const todos = botones.every((x) => b(x));
      const libre = botones.length > 1 || !consumidos.has(botones[0]);
      const activa = todos && libre;
      if (activa && botones.length > 1) for (const x of botones) consumidos.add(x);
      pulsacion(`a:${accion}`, activa, () => op.accion(accion));
    }

    // Los dos clics del ratón: se mandan **los dos flancos**, para poder arrastrar y para que
    // el menú contextual de Windows salga al soltar, como con un ratón de verdad.
    for (const [cual, cb] of [
      ["clic", op.clic],
      ["clic_derecho", op.clicDerecho],
    ] as const) {
      const botones = comoLista(mapa[cual]);
      if (botones.length === 0 || !cb) continue;
      const ahora_pulsado = botones.every((x) => b(x));
      const antes = pulsado.has(cual);
      if (ahora_pulsado && !antes) {
        pulsado.set(cual, ahora);
        cb(true);
      } else if (!ahora_pulsado && antes) {
        pulsado.delete(cual);
        cb(false);
      }
    }

    // Stick izquierdo → desplazar el panel, como la rueda del ratón. Separado de la
    // navegación a propósito: la cruceta salta de opción en opción, el stick recorre.
    if (op.desplazar) {
      const ly = eje(1);
      if (Math.abs(ly) > ZONA_MUERTA) {
        const dt = Math.min((ahora - ultimoFrame) / 1000, 0.05);
        const util = (Math.abs(ly) - ZONA_MUERTA) / (1 - ZONA_MUERTA);
        restoScroll += Math.sign(ly) * util * util * VELOCIDAD_SCROLL * dt;
        const paso = Math.trunc(restoScroll);
        if (paso !== 0) {
          restoScroll -= paso;
          op.desplazar(paso);
        }
      } else {
        restoScroll = 0;
      }
    }

    // Stick derecho → cursor del sistema. El desplazamiento se calcula por tiempo real
    // transcurrido, no por frame, para que la velocidad no dependa de los FPS.
    if (op.cursor) {
      const rx = eje(2);
      const ry = eje(3);
      const mag = Math.hypot(rx, ry);
      if (mag > ZONA_MUERTA) {
        const dt = Math.min((ahora - ultimoFrame) / 1000, 0.05);
        // Curva cuadrática sobre la magnitud útil: precisión cerca del centro, velocidad
        // al fondo. Sin esto, apuntar a un botón pequeño desde el sofá es imposible.
        const util = (mag - ZONA_MUERTA) / (1 - ZONA_MUERTA);
        const paso = util * util * VELOCIDAD_CURSOR * dt;
        // Se acumulan los restos: si no, un movimiento lento redondea a 0 y no avanza.
        restoX += (rx / mag) * paso;
        restoY += (ry / mag) * paso;
        const dx = Math.trunc(restoX);
        const dy = Math.trunc(restoY);
        if (dx !== 0 || dy !== 0) {
          restoX -= dx;
          restoY -= dy;
          op.cursor(dx, dy);
        }
      } else {
        restoX = 0;
        restoY = 0;
      }
    }

    ultimoFrame = ahora;
    raf = requestAnimationFrame(sondear);
  }

  function arrancar() {
    if (activo) return;
    activo = true;
    raf = requestAnimationFrame(sondear);
  }

  function onConectado(e: Event) {
    const gp = (e as GamepadEvent).gamepad;
    op.conexion?.(true, gp.id, detectarLayout(gp.id));
    arrancar();
  }
  function onDesconectado(e: Event) {
    const gp = (e as GamepadEvent).gamepad;
    op.conexion?.(false, gp.id, detectarLayout(gp.id));
  }

  window.addEventListener("gamepadconnected", onConectado);
  window.addEventListener("gamepaddisconnected", onDesconectado);
  // Si ya había uno emparejado antes de abrir el hub, arrancamos igualmente.
  if (Array.from(navigator.getGamepads?.() ?? []).some((g) => g)) arrancar();

  return () => {
    window.removeEventListener("gamepadconnected", onConectado);
    window.removeEventListener("gamepaddisconnected", onDesconectado);
    cancelAnimationFrame(raf);
    activo = false;
  };
}

/**
 * ¿El navegador activaría este control con Enter **por sí solo**?
 *
 * Importa porque un `KeyboardEvent` fabricado con `dispatchEvent` **no tiene acción por
 * defecto**: Chromium solo convierte Enter en `click` para eventos *de confianza* (los que
 * vienen de una tecla real). Con el mando, el botón A movía el foco bien pero no pulsaba
 * nada dentro de los menús — los `<button>` de los diálogos esperaban esa conversión.
 *
 * Los `div[role="button"]` (las cards y filas del grid) **no** entran aquí a propósito:
 * tienen su propio `onkeydown` y necesitan recibir la tecla de verdad, entre otras cosas
 * para distinguir Enter de Ctrl+Enter.
 */
function seActivaSolo(el: Element | null): el is HTMLElement {
  if (!el) return false;
  if ((el as HTMLButtonElement).disabled) return false;
  switch (el.tagName) {
    case "BUTTON":
      return true;
    case "A":
      return el.hasAttribute("href");
    case "INPUT":
      // Un campo de texto necesita la tecla (hay diálogos que guardan con Enter); una
      // casilla o un botón se pulsan.
      return ["checkbox", "radio", "button", "submit", "reset", "file", "color"].includes(
        (el as HTMLInputElement).type,
      );
    default:
      return false;
  }
}

/**
 * Envía una tecla a la **capa activa**: el diálogo abierto si lo hay, y si no el grid.
 * Si el foco ya está dentro de esa capa se respeta; si no (por ejemplo, seguía en una card
 * al abrirse un menú), se redirige a la capa para que la navegación no se vaya por detrás.
 *
 * Con Enter hay un paso extra: si el control se activaría solo con una tecla real, se le
 * hace `click()` en vez de mandarle un evento que no haría nada (ver `seActivaSolo`).
 */
export function enviarTecla(key: string, ctrl = false) {
  const capa = capaActiva();
  const activo = document.activeElement as HTMLElement | null;
  const dentro = !!capa && !!activo && activo !== document.body && capa.contains(activo);
  const destino = dentro ? activo! : (capa ?? document.body);

  if (!ctrl && (key === "Enter" || key === " ") && seActivaSolo(destino)) {
    destino.click();
    return;
  }

  destino.dispatchEvent(
    new KeyboardEvent("keydown", { key, ctrlKey: ctrl, bubbles: true, cancelable: true }),
  );
}
