<script lang="ts">
  import type { Category, GameState, StateCounts } from "../lib/api";
  import { porEje, SUELTAS } from "../lib/categorias";

  let {
    categories,
    counts,
    selectedCategory,
    selectedState,
    needsExe,
    soloFavoritos,
    soloRecientes,
    diasRecientes,
    onselect,
    onselectstate,
    onneedsexe,
    onfavoritos,
    onrecientes,
    onmenucategoria,
  }: {
    categories: Category[];
    counts: StateCounts;
    selectedCategory: number | null;
    selectedState: GameState | null;
    needsExe: boolean;
    soloFavoritos: boolean;
    soloRecientes: boolean;
    diasRecientes: number;
    onselect: (id: number | null) => void;
    onselectstate: (s: GameState | null) => void;
    onneedsexe: (activo: boolean) => void;
    onfavoritos: (activo: boolean) => void;
    onrecientes: (activo: boolean) => void;
    /** Clic derecho sobre una categoría (guión A.0_4 §4). */
    onmenucategoria: (e: MouseEvent, c: Category) => void;
  } = $props();

  let asideEl = $state<HTMLElement | null>(null);

  /**
   * Grupos fijos por estado (guión L.0_1 §1.2). Los vacíos no se pintan.
   * **Estado y categoría son filtros independientes**: se pueden combinar, y los dos se
   * resaltan a la vez (guión F.0_3 §1).
   */
  let estados = $derived([
    { id: null as GameState | null, label: "Toda la biblioteca", count: null as number | null },
    { id: "installed" as GameState, label: "Instalados", count: counts.installed },
    { id: "uninstalled" as GameState, label: "No instalados", count: counts.uninstalled },
    { id: "excluded" as GameState, label: "Excluidos", count: counts.excluded },
  ]);

  /** Cada eje con sus hijas, en el orden que manda el core (F.0_6). */
  let ejes = $derived(porEje(categories));

  /** Arriba/abajo mueven el foco entre categorías (base del mando, M5). */
  function teclado(e: KeyboardEvent) {
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const items = asideEl ? Array.from(asideEl.querySelectorAll<HTMLElement>(".cat")) : [];
    const i = items.indexOf(document.activeElement as HTMLElement);
    if (items.length === 0) return;
    e.preventDefault();
    const destino = i < 0 ? 0 : i + (e.key === "ArrowDown" ? 1 : -1);
    items[Math.min(Math.max(destino, 0), items.length - 1)].focus();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<aside class="sidebar" bind:this={asideEl} onkeydown={teclado}>
  <div class="side-header">BIBLIOTECA</div>
  {#each estados as e}
    {#if e.count === null || e.count > 0 || selectedState === e.id}
      <button
        class="cat"
        class:active={selectedState === e.id}
        onclick={() => onselectstate(e.id)}
      >
        <span>{e.label}</span>
        {#if e.count !== null}<span class="count">{e.count}</span>{/if}
      </button>
    {/if}
  {/each}
  <!-- Favoritos no es un estado: es un marcador aparte que se cruza con el resto
       (un favorito puede estar instalado o no). Por eso va suelto y se alterna. -->
  {#if counts.favorites > 0 || soloFavoritos}
    <button class="cat" class:active={soloFavoritos} onclick={() => onfavoritos(!soloFavoritos)}>
      <span>★ Favoritos</span><span class="count">{counts.favorites}</span>
    </button>
  {/if}

  <!-- Añadidos hace poco. No es un estado ni una categoría de verdad: es una ventana de
       tiempo, así que va suelto con Favoritos. Solo aparece si hay alguno. -->
  {#if counts.recent > 0 || soloRecientes}
    <button
      class="cat"
      class:active={soloRecientes}
      title="Añadidos en los últimos {diasRecientes} días"
      onclick={() => onrecientes(!soloRecientes)}
    >
      <span>🕑 Añadidos hace poco</span><span class="count">{counts.recent}</span>
    </button>
  {/if}

  <div class="side-header">CATEGORÍAS</div>
  <button class="cat" class:active={selectedCategory === null} onclick={() => onselect(null)}>
    <span>Todas</span>
  </button>
  {#each ejes as { eje, hijas }}
    <!-- El eje es pulsable: por herencia, trae los juegos de todas sus hijas. -->
    {#if eje}
      <button
        class="side-group eje"
        class:active={selectedCategory === eje.id}
        title="Todos los de este eje"
        onclick={() => onselect(eje.id)}
        oncontextmenu={(e) => onmenucategoria(e, eje)}
      >
        <span>{eje.name}</span><span class="count">{eje.count}</span>
      </button>
    {:else}
      <div class="side-group">{SUELTAS}</div>
    {/if}
    {#each hijas as c (c.id)}
      <button
        class="cat"
        class:active={selectedCategory === c.id}
        onclick={() => onselect(c.id)}
        oncontextmenu={(e) => onmenucategoria(e, c)}
      >
        <span>{c.name}</span><span class="count">{c.count}</span>
      </button>
    {/each}
  {/each}
  {#if categories.length === 0}
    <div class="side-empty">Aún no hay categorías.</div>
  {/if}

  <!-- Vista de mantenimiento: solo aparece si hay algo que arreglar (guión F.0_3 §3). -->
  {#if counts.needs_exe > 0 || needsExe}
    <div class="side-header">REVISAR</div>
    <button class="cat" class:active={needsExe} onclick={() => onneedsexe(!needsExe)}>
      <span>Sin ejecutable</span><span class="count">{counts.needs_exe}</span>
    </button>
  {/if}
</aside>
