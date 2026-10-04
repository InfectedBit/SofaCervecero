<script lang="ts">
  /**
   * Un botón del mando, o **una combinación** (guión E.0_8): `LB` + `RB` se pinta como dos
   * teclas con un `+` en medio, no como una sola etiqueta larga.
   */
  import { layoutActivo } from "../lib/prefs.svelte";
  import { colorBoton, comoLista, nombreBoton, type Asignacion } from "../lib/gamepad";

  let { boton, sm = false }: { boton: Asignacion | null | undefined; sm?: boolean } = $props();

  let layout = $derived(layoutActivo());
  let botones = $derived(comoLista(boton));
</script>

{#if botones.length === 0}
  <span class="tecla" class:sm>—</span>
{:else}
  {#each botones as b, i (b)}
    {#if i > 0}<span class="tecla-mas">+</span>{/if}
    {@const color = colorBoton(b, layout)}
    <span
      class="tecla"
      class:sm
      data-color={color ? "" : undefined}
      style={color ? `--color-boton:${color}` : undefined}>{nombreBoton(b, layout)}</span
    >
  {/each}
{/if}
