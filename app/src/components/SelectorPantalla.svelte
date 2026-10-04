<script lang="ts">
  /**
   * Las pantallas conectadas, dibujadas **a escala y en su posición real**, como el panel de
   * Windows (guión E.0_4 §4.2).
   *
   * Que estén colocadas igual que en el escritorio no es decoración: es lo único que permite
   * reconocer «el televisor es el de la izquierda» sin leer un nombre de dispositivo. En este
   * equipo el televisor está en (−4096, −706), o sea arriba y a la izquierda del origen, así que
   * el lienzo se calcula a partir de los extremos reales y no se puede suponer que empieza en 0.
   */
  import type { Pantalla } from "../lib/api";

  let {
    pantallas,
    elegida = $bindable(),
  }: {
    pantallas: Pantalla[];
    /** `id` de la seleccionada. */
    elegida: string | null;
  } = $props();

  /** Caja que envuelve a todas, para poder normalizar las posiciones a porcentajes. */
  let caja = $derived.by(() => {
    const x0 = Math.min(...pantallas.map((p) => p.x));
    const y0 = Math.min(...pantallas.map((p) => p.y));
    const x1 = Math.max(...pantallas.map((p) => p.x + p.ancho));
    const y1 = Math.max(...pantallas.map((p) => p.y + p.alto));
    return { x0, y0, ancho: Math.max(1, x1 - x0), alto: Math.max(1, y1 - y0) };
  });

  const pct = (n: number, total: number) => `${(n / total) * 100}%`;
</script>

<div class="pantallas" style="aspect-ratio: {caja.ancho} / {caja.alto}">
  {#each pantallas as p (p.id)}
    <button
      class="pantalla"
      class:elegida={elegida === p.id}
      aria-pressed={elegida === p.id}
      title={`${p.nombre} — ${p.ancho}×${p.alto}${p.escala !== 1 ? ` al ${Math.round(p.escala * 100)} %` : ""}`}
      style="
        left:   {pct(p.x - caja.x0, caja.ancho)};
        top:    {pct(p.y - caja.y0, caja.alto)};
        width:  {pct(p.ancho, caja.ancho)};
        height: {pct(p.alto, caja.alto)};
      "
      onclick={() => (elegida = p.id)}
    >
      <span class="pantalla-nombre">{p.nombre}</span>
      <span class="pantalla-datos">
        {p.ancho}×{p.alto}{#if p.escala !== 1}&nbsp;· {Math.round(p.escala * 100)}%{/if}
      </span>
      {#if p.primaria}<span class="pantalla-marca">principal</span>{/if}
    </button>
  {/each}
</div>
