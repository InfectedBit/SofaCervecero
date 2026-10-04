<script lang="ts">
  import { fmtTiempo, STATE_LABEL, type GameCard } from "../lib/api";
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
    /** Clic derecho: el mismo menú contextual que en la cuadrícula (guión A.0_4 §4). */
    onmenu: (e: MouseEvent, game: GameCard) => void;
  } = $props();

  let instalado = $derived(game.state === "installed");
  let lanzable = $derived(instalado && !game.needs_exe);
  let jugadores = $derived(
    game.players_max && game.players_max > 1 ? `${game.players_max}P` : null,
  );

  function teclado(e: KeyboardEvent) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      if (e.ctrlKey && lanzable) onlaunch(game.id);
      else onopen(game.id);
    } else if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
      e.preventDefault();
      const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
      onmenu(
        new MouseEvent("contextmenu", { clientX: r.left + 80, clientY: r.bottom }),
        game,
      );
    }
  }
</script>

<div
  class="row"
  class:missing={!instalado}
  role="button"
  tabindex="0"
  aria-label={game.title}
  onclick={() => onopen(game.id)}
  oncontextmenu={(e) => onmenu(e, game)}
  onkeydown={teclado}
>
  <div class="row-cover">
    <Caratula titulo={game.title} cover={game.cover_path} storeId={game.store_id} />
  </div>
  {#if game.favorite}<span class="row-fav" title="Favorito">★</span>{/if}
  <span class="row-title">{game.title}</span>
  <span class="row-meta">{game.platform}</span>
  {#if jugadores}<span class="row-meta">{jugadores}</span>{/if}
  {#if game.playtime_secs > 0}
    <span class="row-meta" title="Tiempo jugado (TimeTrack)">{fmtTiempo(game.playtime_secs)}</span>
  {/if}
  {#if !instalado}
    <span class="row-meta warn">{STATE_LABEL[game.state]}</span>
  {:else if game.needs_exe}
    <span class="row-meta">sin ejecutable</span>
  {/if}
  {#if lanzable}
    <button
      class="btn sm primary"
      title="Lanzar"
      tabindex="-1"
      onclick={(e) => {
        e.stopPropagation();
        onlaunch(game.id);
      }}>▶</button
    >
  {/if}
</div>
