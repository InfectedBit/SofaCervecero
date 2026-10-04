<script lang="ts">
  import type { GameCard } from "../lib/api";
  import { prefs } from "../lib/prefs.svelte";
  import Caratula from "./Caratula.svelte";

  let {
    game,
    onopen,
    onlaunch,
    onmenu,
  }: {
    game: GameCard;
    onopen: (id: number) => void;
    onlaunch: (id: number) => void;
    /** Clic derecho: abre el menú contextual del juego (guión A.0_4 §4). */
    onmenu: (e: MouseEvent, game: GameCard) => void;
  } = $props();

  let instalado = $derived(game.state === "installed");
  let lanzable = $derived(instalado && !game.needs_exe);
  let etiqueta = $derived(
    game.state === "uninstalled"
      ? { texto: "no instalado", ayuda: "No está en disco ahora mismo", warn: true }
      : game.state === "excluded"
        ? { texto: "excluido", ayuda: "No volverá a aparecer al escanear", warn: false }
        : game.needs_exe
          ? {
              texto: "sin ejecutable",
              ayuda: "Ábrelo y usa Editar para indicar la ruta",
              warn: false,
            }
          : null,
  );

  let cajaTitulo = $state<HTMLElement | null>(null);
  let textoTitulo = $state<HTMLElement | null>(null);
  /** Píxeles que sobresalen del contenedor; 0 si el título cabe entero. */
  let sobra = $state(0);
  /** Velocidad constante (~55 px/s) para que un título largo no vaya más rápido. */
  let duracion = $derived(Math.max(3, (sobra / 55) * 2 + 1.5));

  // El ancho depende del tamaño de card elegido en Ajustes y del texto real, así que se mide
  // en cuanto ambos existen y cada vez que cambia el título.
  $effect(() => {
    void game.title;
    void prefs.cardSize;
    if (!cajaTitulo || !textoTitulo) return;
    sobra = Math.max(0, textoTitulo.scrollWidth - cajaTitulo.clientWidth);
  });

  /** Enter/Espacio abren el detalle; Ctrl+Enter lanza directamente. */
  function teclado(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      if (e.ctrlKey && lanzable) onlaunch(game.id);
      else onopen(game.id);
    } else if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
      // Mismo menú que el clic derecho, para teclado y mando. Se coloca sobre la card.
      e.preventDefault();
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      onmenu(
        new MouseEvent("contextmenu", { clientX: r.left + r.width / 2, clientY: r.top + 40 }),
        game,
      );
    }
  }
</script>

<div
  class="card"
  class:missing={!instalado}
  role="button"
  tabindex="0"
  aria-label={game.title}
  title={game.title}
  onclick={() => onopen(game.id)}
  oncontextmenu={(e) => onmenu(e, game)}
  onkeydown={teclado}
>
  <div class="cover">
    <Caratula titulo={game.title} cover={game.cover_path} storeId={game.store_id} />
    {#if game.favorite}
      <span class="fav" title="Favorito">★</span>
    {/if}
    {#if etiqueta}
      <span class="badge" class:warn={etiqueta.warn} title={etiqueta.ayuda}>{etiqueta.texto}</span>
    {/if}
    {#if game.players_max && game.players_max > 1}
      <span class="badge players" title="Nº de jugadores">{game.players_max}P</span>
    {/if}
    {#if lanzable}
      <button
        class="play"
        title="Lanzar"
        tabindex="-1"
        onclick={(e) => {
          e.stopPropagation();
          onlaunch(game.id);
        }}>▶</button
      >
    {/if}
  </div>
  <!-- El título se desliza al enfocar solo si de verdad no cabe (guión A.0_3, A13). -->
  <div class="title" bind:this={cajaTitulo}>
    <span
      class="title-marquesina"
      bind:this={textoTitulo}
      data-sobra={sobra > 0 ? "" : undefined}
      style={sobra > 0 ? `--sobra:${sobra}px; --duracion:${duracion}s` : undefined}
      >{game.title}</span
    >
  </div>
</div>
