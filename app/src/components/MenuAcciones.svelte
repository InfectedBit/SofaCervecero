<script lang="ts">
  import type { Snippet } from "svelte";

  /**
   * Menú desplegable para acciones secundarias (guión A.0_3, punto A11).
   *
   * Existe para sacar **Excluir** y **Eliminar** de la fila principal: estaban al mismo nivel
   * que Lanzar y Editar, y son irreversibles (o casi). Aquí hacen falta dos gestos.
   */
  let {
    etiqueta = "⋯ Más",
    children,
  }: {
    etiqueta?: string;
    children: Snippet<[() => void]>;
  } = $props();

  let abierto = $state(false);
  let raiz = $state<HTMLElement | null>(null);

  /** Cerrar al pulsar fuera, pero sin tragarse el clic que abre el menú. */
  function fuera(e: MouseEvent) {
    if (abierto && raiz && !raiz.contains(e.target as Node)) abierto = false;
  }
</script>

<svelte:window
  onmousedown={fuera}
  onkeydown={(e) => {
    if (e.key === "Escape" && abierto) {
      e.stopPropagation();
      abierto = false;
    }
  }}
/>

<div class="menu-wrap" bind:this={raiz}>
  <button class="btn" aria-expanded={abierto} onclick={() => (abierto = !abierto)}>
    {etiqueta}
  </button>
  {#if abierto}
    <div class="menu-pop" role="menu">
      {@render children(() => (abierto = false))}
    </div>
  {/if}
</div>
