<script lang="ts">
  import {
    contextual,
    cerrarContextual,
    volverContextual,
    entrarSubmenu,
    refrescarContextual,
    type OpcionContextual,
  } from "../lib/contextual.svelte";

  let menu = $state<HTMLElement | null>(null);

  let nivel = $derived(contextual.pila[contextual.pila.length - 1]);
  let haySubnivel = $derived(contextual.pila.length > 1);

  /** Se recoloca si se sale por abajo o por la derecha de la ventana. */
  $effect(() => {
    if (!contextual.abierto || !menu) return;
    // Depender de la página activa: al entrar en un submenú cambia de alto y hay que recolocar.
    void contextual.pila.length;
    const r = menu.getBoundingClientRect();
    const margen = 8;
    if (r.right > window.innerWidth - margen) {
      menu.style.left = `${Math.max(margen, window.innerWidth - r.width - margen)}px`;
    }
    if (r.bottom > window.innerHeight - margen) {
      menu.style.top = `${Math.max(margen, window.innerHeight - r.height - margen)}px`;
    }
    menu.querySelector<HTMLElement>(".menu-item")?.focus();
  });

  function pulsar(o: OpcionContextual) {
    if (o.submenu) {
      entrarSubmenu(o.etiqueta, o.submenu);
      return;
    }
    if (o.mantener) {
      // Marcar varias seguidas: no se cierra, solo se repinta para que cambie el acento.
      o.accion?.();
      refrescarContextual();
      return;
    }
    cerrarContextual();
    o.accion?.();
  }

  function teclado(e: KeyboardEvent) {
    if (!contextual.abierto) return;
    if (e.key === "Escape") {
      e.stopPropagation();
      // Dentro de un submenú, atrás vuelve en vez de cerrarlo todo: es lo que se espera
      // del botón B del mando.
      if (!volverContextual()) cerrarContextual();
      return;
    }
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
    const items = menu ? Array.from(menu.querySelectorAll<HTMLElement>(".menu-item")) : [];
    if (items.length === 0) return;
    e.preventDefault();
    const i = items.indexOf(document.activeElement as HTMLElement);
    const paso = e.key === "ArrowDown" ? 1 : -1;
    items[(Math.max(i, 0) + paso + items.length) % items.length].focus();
  }
</script>

<svelte:window onkeydown={teclado} onresize={cerrarContextual} />

{#if contextual.abierto && nivel}
  <!-- Capa que captura el siguiente clic para cerrar, sin tapar nada visualmente. -->
  <div
    class="ctx-captura"
    role="presentation"
    onmousedown={cerrarContextual}
    oncontextmenu={(e) => {
      e.preventDefault();
      cerrarContextual();
    }}
  ></div>
  <div
    class="menu-pop ctx"
    role="menu"
    tabindex="-1"
    bind:this={menu}
    style="left:{contextual.x}px; top:{contextual.y}px"
  >
    {#if haySubnivel}
      <button class="menu-item ctx-volver" onclick={() => volverContextual()}>
        <span class="ctx-icono">‹</span>{nivel.titulo}
      </button>
      <div class="menu-sep"></div>
    {:else if nivel.titulo}
      <div class="ctx-titulo">{nivel.titulo}</div>
    {/if}

    {#each nivel.opciones as o (o.etiqueta)}
      {#if o.separar}<div class="menu-sep"></div>{/if}
      {#if o.cabecera}
        <div class="ctx-grupo">{o.etiqueta}</div>
      {:else}
        <button
          class="menu-item"
          class:danger={o.peligro}
          class:activa={o.activa}
          aria-pressed={o.mantener ? !!o.activa : undefined}
          onclick={() => pulsar(o)}
        >
          {#if o.icono}<span class="ctx-icono">{o.icono}</span>{/if}
          {o.etiqueta}
          {#if o.submenu}<span class="ctx-flecha">›</span>{/if}
        </button>
      {/if}
    {/each}
  </div>
{/if}
