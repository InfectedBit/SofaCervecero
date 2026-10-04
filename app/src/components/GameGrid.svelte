<script lang="ts">
  import type { Category, GameCard } from "../lib/api";
  import { prefs, setPrefs } from "../lib/prefs.svelte";
  import { partir, type Seccion } from "../lib/secciones";
  import { abrirContextual } from "../lib/contextual.svelte";
  import SeccionGrid from "./SeccionGrid.svelte";

  let {
    games,
    categories,
    scanning,
    onopen,
    onlaunch,
    onadd,
    onmenu,
  }: {
    games: GameCard[];
    categories: Category[];
    scanning: boolean;
    onopen: (id: number) => void;
    onlaunch: (id: number) => void;
    onadd: () => void;
    onmenu: (e: MouseEvent, game: GameCard) => void;
  } = $props();

  let gridEl = $state<HTMLElement | null>(null);
  let lista = $derived(prefs.view === "list");

  /**
   * La lista ya viene ordenada del core; aquí solo se reparte en secciones (guión F.0_5).
   * Con `seccion = "none"` sale una única sección sin cabecera: la vista de siempre.
   */
  let secciones = $derived(partir(games, [prefs.seccion, prefs.seccion2, prefs.seccion3], categories));

  /** Secciones plegadas. La clave ya lleva el eje delante (`cat:3:Steam`), así que no se
   *  mezclan las de modos distintos. */
  let plegadas = $derived(new Set(prefs.plegadas));
  const estaPlegada = (clave: string) => plegadas.has(clave);

  function alternar(clave: string) {
    const s = new Set(prefs.plegadas);
    if (!s.delete(clave)) s.add(clave);
    setPrefs({ plegadas: [...s] });
  }

  /** Claves de todo el árbol visible, en orden de documento. La sección sin cabecera no cuenta. */
  function clavesDe(secs: Seccion[]): string[] {
    return secs.flatMap((s) => (s.titulo === "" ? [] : [s.clave, ...clavesDe(s.hijas)]));
  }

  /**
   * Menú contextual de una cabecera de sección (guión A.0_5 §3).
   *
   * «Comprimir todas» pliega **solo el primer nivel**: lo que se busca con eso es el índice de
   * categorías de un vistazo, y plegar también las anidadas las dejaría cerradas al abrir su
   * padre, lo que no se ve venir. «Desplegar todas» sí limpia el árbol entero, para que no
   * quede nada cerrado a escondidas.
   *
   * Las claves de otros ejes se conservan: cada una lleva su eje delante (`cat:3:Steam`), así
   * que cambiar de vista no pierde lo que estuviera plegado en la anterior.
   */
  function menuSeccion(e: MouseEvent, sec: Seccion) {
    const raiz = secciones.filter((s) => s.titulo !== "").map((s) => s.clave);
    const arbol = clavesDe(secciones);
    const cerrada = estaPlegada(sec.clave);
    abrirContextual(e, sec.titulo, [
      {
        etiqueta: cerrada ? "Desplegar esta categoría" : "Comprimir esta categoría",
        icono: cerrada ? "▾" : "▸",
        accion: () => alternar(sec.clave),
      },
      {
        etiqueta: "Comprimir todas las categorías",
        icono: "⊟",
        separar: true,
        accion: () => setPrefs({ plegadas: [...new Set([...prefs.plegadas, ...raiz])] }),
      },
      {
        etiqueta: "Desplegar todas las categorías",
        icono: "⊞",
        accion: () =>
          setPrefs({ plegadas: prefs.plegadas.filter((k) => !arbol.includes(k)) }),
      },
    ]);
  }

  /** Todas las cards en orden de documento, atravesando las secciones. */
  function cards(): HTMLElement[] {
    return gridEl
      ? Array.from(gridEl.querySelectorAll<HTMLElement>(".card, .row, .seccion-cab"))
      : [];
  }

  /**
   * Nº de columnas del contenedor **donde está esa card**, no del primero de la página: con
   * secciones hay varias cuadrículas, y la última fila de cada una suele estar incompleta.
   */
  function columnasDe(el: HTMLElement): number {
    if (lista) return 1;
    const cont = el.closest(".grid");
    if (!cont) return 1;
    const hijos = Array.from(cont.children) as HTMLElement[];
    const top = hijos[0]?.offsetTop ?? 0;
    let n = 0;
    while (n < hijos.length && hijos[n].offsetTop === top) n++;
    return Math.max(1, n);
  }

  /**
   * Navegación con flechas, que es también la del mando (el D-pad se traduce a estas teclas).
   * El índice es plano sobre todas las secciones, así que bajar desde la última fila de una
   * sección entra en la siguiente sin tratarlo como caso especial.
   */
  function teclado(e: KeyboardEvent) {
    const nav = ["ArrowRight", "ArrowLeft", "ArrowDown", "ArrowUp", "Home", "End"];
    if (!nav.includes(e.key)) return;
    const els = cards();
    if (els.length === 0) return;
    const i = els.indexOf(document.activeElement as HTMLElement);
    const cols = i >= 0 ? columnasDe(els[i]) : 1;
    let destino = 0;
    if (i >= 0) {
      if (e.key === "ArrowRight") destino = i + 1;
      else if (e.key === "ArrowLeft") destino = i - 1;
      else if (e.key === "ArrowDown") destino = i + cols;
      else if (e.key === "ArrowUp") destino = i - cols;
      else if (e.key === "End") destino = els.length - 1;
      // "Home" → 0
    }
    e.preventDefault();
    const elegido = els[Math.min(Math.max(destino, 0), els.length - 1)];
    elegido.focus();
    // Con secciones hay cabeceras por medio: sin esto la card enfocada puede quedar tapada.
    elegido.scrollIntoView({ block: "nearest" });
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<main class="grid-area" onkeydown={teclado}>
  {#if scanning && games.length === 0}
    <div class="empty">Escaneando biblioteca…</div>
  {:else if games.length === 0}
    <div class="empty">
      <p>No hay juegos que coincidan.</p>
      <button class="btn primary" onclick={onadd}>Añadir biblioteca / app</button>
    </div>
  {:else}
    <div class="secciones" bind:this={gridEl}>
      {#each secciones as sec (sec.clave)}
        <SeccionGrid
          seccion={sec}
          nivel={0}
          {lista}
          plegada={estaPlegada}
          onplegar={alternar}
          onmenuseccion={menuSeccion}
          {onopen}
          {onlaunch}
          {onmenu}
        />
      {/each}
    </div>
  {/if}
</main>
