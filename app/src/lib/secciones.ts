/**
 * Partir la cuadrícula en secciones, ahora **anidadas** (guión F.0_6 / ADR-005).
 *
 * La idea de fondo es la de *Manage Apps* de TimeTrack —`[A] ──────` y debajo los suyos— pero
 * generalizada a **ejes**: se elige por qué eje partir, y opcionalmente por cuál dentro de
 * cada sección. Un eje es una categoría raíz (*Clientes*, *Nº Jugadores*, *TAGS*), o uno de
 * los ejes fijos (inicial, estado, plataforma, origen).
 *
 * Lo que resuelve, y es la razón de existir de todo esto: *Coop* decía **con cuánta gente se
 * juega** y *LEGO* **qué clase de juego es**, pero estaban en el mismo grupo. Al ser hermanas,
 * un LEGO cooperativo tenía que salir dos veces. Separadas en ejes distintos, sale una vez.
 */

import { STATE_LABEL, type Category, type GameCard, type GameState } from "./api";
import { ORIGEN_LABEL } from "./filtros";

/**
 * Por qué eje se parte. Los fijos son literales; un eje de categorías es `cat:<id>`.
 * `none` = no partir (la cuadrícula de siempre).
 */
export type ModoSeccion = string;

export const EJES_FIJOS: { id: ModoSeccion; texto: string; ayuda: string }[] = [
  { id: "none", texto: "Sin secciones", ayuda: "Una sola cuadrícula" },
  { id: "inicial", texto: "Inicial del título", ayuda: "[A], [B], [C]…" },
  { id: "estado", texto: "Estado", ayuda: "Instalados · No instalados · Excluidos" },
  { id: "plataforma", texto: "Plataforma", ayuda: "PC, emulado…" },
  { id: "origen", texto: "Origen", ayuda: "Steam · carpeta · app añadida a mano" },
];

/**
 * **Todas las categorías**: encadena *todos* los ejes, en su orden, como si se hubieran
 * elegido a mano uno detrás de otro.
 *
 * ```
 * [Steam] → [Split-Screen] → [LEGO] → "Lego Batman"
 * ```
 *
 * Corrige un error de bulto de la primera versión, que ponía los ejes **uno al lado de otro**
 * (una sección por eje). Así *Lego Batman* salía tres veces, una por eje, y se dio por
 * inevitable — *«ver todas las categorías y no repetir no pueden cumplirse a la vez»*.
 * **Era falso:** solo lo es con los ejes en paralelo. **Encadenados se ven todos los ejes y
 * ningún juego se repite**, porque cada uno cae en una única rama.
 */
export const INDICE = "cat:*";

/** Los ejes que se pueden elegir: los fijos, el índice, y una entrada por categoría raíz. */
export function ejesDisponibles(
  categorias: Category[],
): { id: ModoSeccion; texto: string; ayuda: string }[] {
  const raices = categorias
    .filter((c) => c.parent_id === null)
    .map((c) => ({
      id: `cat:${c.id}`,
      texto: c.name,
      ayuda: "Una sección por cada categoría de este eje. Ningún juego se repite",
    }));
  const indice =
    raices.length > 0
      ? [
          {
            id: INDICE,
            texto: "Categorías (todas)",
            ayuda: "Encadena todos los ejes en orden. Ningún juego se repite",
          },
        ]
      : [];
  return [...EJES_FIJOS, ...indice, ...raices];
}

export type Seccion = {
  /** Identidad estable para el `{#each}`. */
  clave: string;
  /** Lo que se pinta entre corchetes: `[A]`, `[Coop + LEGO]`, `[Steam]`… Vacío = sin cabecera. */
  titulo: string;
  /** Todos los de esta rama, repartidos o no. Es lo que enseña el contador. */
  juegos: GameCard[];
  /**
   * Los que se quedan **en este nivel**: no tienen clasificación en ninguno de los ejes que
   * vienen por debajo, así que se pintan aquí mismo en vez de en un `[Sin clasificar]`.
   */
  propios: GameCard[];
  /** Subsecciones de los ejes siguientes. */
  hijas: Seccion[];
};

/** Un juego sin dato en el eje que se está mirando. Nunca llega a pintarse como sección. */
const SIN_DATO = "Sin clasificar";
/** Los que no encajan en **ningún** eje. Van al final, con cabecera, y se pueden plegar. */
const SIN_CATEGORIA = "Sin categoría";

/** Primera letra en mayúscula; números y símbolos caen en `#`, como en TimeTrack. */
function inicial(titulo: string): string {
  const c = titulo.trim().charAt(0).toUpperCase();
  if (!c) return "#";
  // Se normalizan los acentos para que «Ángel» caiga en [A] y no en una sección [Á] suelta.
  const base = c.normalize("NFD").replace(/[\u0300-\u036f]/g, "");
  return /[A-Z]/.test(base) ? base : "#";
}

/** Clave de un juego en un eje fijo. Siempre una sola: aquí no hay combinaciones. */
function claveFija(g: GameCard, modo: ModoSeccion): string {
  switch (modo) {
    case "inicial":
      return inicial(g.title);
    case "estado":
      return STATE_LABEL[g.state as GameState] ?? g.state;
    case "plataforma":
      return g.platform || SIN_DATO;
    case "origen":
      return g.origin ? (ORIGEN_LABEL[g.origin] ?? g.origin) : SIN_DATO;
    default:
      return SIN_DATO;
  }
}

/**
 * Clave de un juego en un eje de categorías: **el conjunto** de categorías de ese eje a las
 * que pertenece, no una de ellas.
 *
 * Es la decisión de ADR-005 §3.1: un juego que es *Couch Co-Op* **y** *Online Co-Op* va a su
 * propia sección `[Couch Co-Op + Online Co-Op]`, en vez de salir repetido en las dos. Así
 * ningún juego aparece dos veces y ninguna sección miente sobre lo que contiene.
 *
 * El orden dentro de la etiqueta es **canónico** (el del eje): si no, `Coop + LEGO` y
 * `LEGO + Coop` serían dos secciones distintas para el mismo conjunto.
 */
function claveCategorias(g: GameCard, hijas: Category[]): { titulo: string; orden: number[] } {
  const suyas = hijas.filter((c) => g.categories.includes(c.id));
  if (suyas.length === 0) return { titulo: SIN_DATO, orden: [Number.MAX_SAFE_INTEGER] };
  return {
    titulo: suyas.map((c) => c.name).join(" + "),
    orden: suyas.map((c) => hijas.indexOf(c)),
  };
}

/** Hijas directas de un eje, ya en su orden (el core las devuelve ordenadas). */
function hijasDe(categorias: Category[], ejeId: number): Category[] {
  return categorias.filter((c) => c.parent_id === ejeId);
}

/**
 * Reparte una lista por los ejes indicados. Devuelve **dos cosas**: las secciones, y los
 * juegos que se quedan sueltos en este nivel.
 *
 * La regla que lo gobierna, y que es lo que lo hace usable con datos reales:
 *
 * > **Un eje que un juego no tiene, no se le aplica.** No va a `[Sin clasificar]`: baja al eje
 * > siguiente **en el mismo nivel**, y si ya no quedan, se queda en el listado de donde esté.
 *
 * Con la biblioteca real la diferencia es enorme: 28 juegos tienen cliente pero ni modo de
 * juego ni etiquetas, y 15 al revés. Con la regla estricta acababan en
 * `[Steam] → [Sin clasificar]` y `[Sin clasificar] → [Coop]`; ahora salen en `[Steam]` y en
 * `[Coop]`, que es donde se les busca.
 */
function repartir(
  juegos: GameCard[],
  ejes: ModoSeccion[],
  categorias: Category[],
): { propios: GameCard[]; secciones: Seccion[] } {
  const [modo, ...resto] = ejes;
  if (!modo) return { propios: juegos, secciones: [] };

  const esCat = modo.startsWith("cat:");
  const hijas = esCat ? hijasDe(categorias, Number(modo.slice(4))) : [];

  // Se respeta el orden de entrada: el core ya ha ordenado la lista.
  const grupos = new Map<string, { juegos: GameCard[]; orden: number[] }>();
  const sinEste: GameCard[] = [];
  for (const g of juegos) {
    const { titulo, orden } = esCat
      ? claveCategorias(g, hijas)
      : { titulo: claveFija(g, modo), orden: [0] };
    if (titulo === SIN_DATO) {
      sinEste.push(g);
      continue;
    }
    const previo = grupos.get(titulo);
    if (previo) previo.juegos.push(g);
    else grupos.set(titulo, { juegos: [g], orden });
  }

  let claves = [...grupos.keys()];
  if (modo === "inicial") {
    // Un índice A–Z se espera alfabético, venga la lista ordenada por lo que venga.
    claves.sort((a, b) => (a === "#" ? 1 : b === "#" ? -1 : a.localeCompare(b, "es")));
  } else if (esCat) {
    // Por la posición de sus miembros en el eje: `[Couch]` antes que `[Couch + Online]`.
    // El `-1` de relleno es lo que hace que **la combinación más corta vaya primero**: al
    // agotarse una lista, «no tiene más miembros» ordena antes que «tiene uno más».
    claves.sort((a, b) => {
      const oa = grupos.get(a)!.orden;
      const ob = grupos.get(b)!.orden;
      for (let i = 0; i < Math.max(oa.length, ob.length); i++) {
        const d = (oa[i] ?? -1) - (ob[i] ?? -1);
        if (d !== 0) return d;
      }
      return 0;
    });
  }

  const secciones: Seccion[] = claves.map((clave) => {
    const suyos = grupos.get(clave)!.juegos;
    const dentro = repartir(suyos, resto, categorias);
    return {
      clave: `${modo}:${clave}`,
      titulo: clave,
      juegos: suyos,
      propios: dentro.propios,
      hijas: dentro.secciones,
    };
  });

  // Los que no tienen nada en este eje bajan al siguiente **sin perder nivel**: así un juego
  // que solo es «Coop» sale en `[Coop]` y no enterrado en `[Sin clasificar] → [Coop]`.
  const sueltos = repartir(sinEste, resto, categorias);
  return { propios: sueltos.propios, secciones: [...secciones, ...sueltos.secciones] };
}

/**
 * Parte una lista de juegos por los ejes indicados, en orden. Cada eje añade un nivel.
 *
 * `ejes = []` o `["none"]` devuelve una única sección sin cabecera: la vista de siempre.
 */
export function partir(
  juegos: GameCard[],
  ejes: ModoSeccion[],
  categorias: Category[],
): Seccion[] {
  let utiles = ejes.filter((e) => e && e !== "none");
  // «Todas» se sustituye por la lista de ejes en su orden. Encadenarlos —en vez de ponerlos
  // en paralelo— es lo que mantiene a cada juego en una sola rama.
  if (utiles.includes(INDICE)) {
    const todos = categorias.filter((c) => c.parent_id === null).map((c) => `cat:${c.id}`);
    utiles = utiles.flatMap((e) => (e === INDICE ? todos : [e]));
  }
  if (utiles.length === 0) {
    return [{ clave: "todo", titulo: "", juegos, propios: juegos, hijas: [] }];
  }

  const { propios, secciones } = repartir(juegos, utiles, categorias);
  // Lo que no encaja en ningún eje va al final, **con cabecera**: así se sabe que están ahí,
  // se ven contados y se pueden plegar de un clic — con 143 juegos sin etiquetar, importa.
  // El nombre depende de por qué se partió: «Sin categoría» solo si se partió por categorías.
  if (propios.length > 0) {
    const porCategorias = utiles.some((e) => e.startsWith("cat:"));
    secciones.push({
      clave: `${utiles[0]}:__resto`,
      titulo: porCategorias ? SIN_CATEGORIA : SIN_DATO,
      juegos: propios,
      propios,
      hijas: [],
    });
  }
  return secciones;
}
