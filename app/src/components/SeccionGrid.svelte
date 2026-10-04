<script lang="ts">
  /**
   * Una sección de la cuadrícula, que puede contener subsecciones (guión F.0_6).
   *
   * Es recursivo a propósito: el número de ejes por los que se parte no está fijado en el
   * marcado, solo en `partir()`. Hoy el panel ofrece dos, pero el componente aguanta más.
   */
  import type { GameCard } from "../lib/api";
  import type { Seccion } from "../lib/secciones";
  import { prefs } from "../lib/prefs.svelte";
  import GameCardEl from "./GameCard.svelte";
  import GameRow from "./GameRow.svelte";
  import Self from "./SeccionGrid.svelte";

  let {
    seccion,
    nivel,
    lista,
    plegada,
    onplegar,
    onmenuseccion,
    onopen,
    onlaunch,
    onmenu,
  }: {
    seccion: Seccion;
    /** 0 = eje principal, 1+ = anidada. El `top` es `nivel × 34px`, el alto fijo de la
     *  cabecera: así las de varios niveles se apilan sin dejar hueco entre ellas. */
    nivel: number;
    lista: boolean;
    plegada: (clave: string) => boolean;
    onplegar: (clave: string) => void;
    onmenuseccion: (e: MouseEvent, seccion: Seccion) => void;
    onopen: (id: number) => void;
    onlaunch: (id: number) => void;
    onmenu: (e: MouseEvent, game: GameCard) => void;
  } = $props();

  let cerrada = $derived(plegada(seccion.clave));
  /** Sin título = la sección única de «sin secciones»: se pinta sin cabecera. */
  let conCabecera = $derived(seccion.titulo !== "");

  /** El mismo menú que el clic derecho, para teclado y mando: la cabecera también se enfoca. */
  function teclado(e: KeyboardEvent) {
    if (e.key !== "ContextMenu" && !(e.shiftKey && e.key === "F10")) return;
    e.preventDefault();
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    onmenuseccion(
      new MouseEvent("contextmenu", { clientX: r.left + 40, clientY: r.bottom }),
      seccion,
    );
  }
</script>

<section class="seccion" class:anidada={nivel > 0}>
  <!-- `[A] ─────`, igual que Manage Apps de TimeTrack. -->
  {#if conCabecera}
    <button
      class="seccion-cab"
      class:n2={nivel > 0}
      style="top: {nivel * 34}px"
      aria-expanded={!cerrada}
      title={cerrada ? "Desplegar" : "Plegar"}
      onclick={() => onplegar(seccion.clave)}
      oncontextmenu={(e) => onmenuseccion(e, seccion)}
      onkeydown={teclado}
    >
      <span class="seccion-flecha">{cerrada ? "▸" : "▾"}</span>
      <span class="seccion-nombre">[{seccion.titulo}]</span>
      <span class="seccion-linea"></span>
      <span class="seccion-cuenta">{seccion.juegos.length}</span>
    </button>
  {/if}

  {#if cerrada}
    <!-- Plegada: no se pinta nada. Al desaparecer del DOM, la navegación las salta sola. -->
  {:else}
    <!-- Primero los que se quedan en este nivel: los que no tienen clasificación en ningún
         eje de más abajo. Antes acababan en un `[Sin clasificar]` que no aportaba nada. -->
    {#if seccion.propios.length > 0}
      {#if lista}
        <div class="list">
          {#each seccion.propios as g (g.id)}
            <GameRow game={g} {onopen} {onlaunch} {onmenu} />
          {/each}
        </div>
      {:else}
        <div
          class="grid"
          class:square={prefs.cardShape === "square"}
          style="--card-size: {prefs.cardSize}px"
        >
          {#each seccion.propios as g (g.id)}
            <GameCardEl game={g} {onopen} {onlaunch} {onmenu} />
          {/each}
        </div>
      {/if}
    {/if}

    {#each seccion.hijas as hija (hija.clave)}
      <Self
        seccion={hija}
        nivel={nivel + 1}
        {lista}
        {plegada}
        {onplegar}
        {onmenuseccion}
        {onopen}
        {onlaunch}
        {onmenu}
      />
    {/each}
  {/if}
</section>
