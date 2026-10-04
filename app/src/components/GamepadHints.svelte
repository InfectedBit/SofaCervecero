<script lang="ts">
  import { prefs } from "../lib/prefs.svelte";
  import Tecla from "./Tecla.svelte";

  let { enDialogo }: { enDialogo: boolean } = $props();

  /**
   * Ayuda contextual: dentro de un menú emergente solo valen aceptar y cerrar, así que
   * enseñar ahí "lanzar juego" o "buscar" sería mentir.
   */
  let pistas = $derived(
    enDialogo
      ? [
          { boton: prefs.gamepadMap.detalle, texto: "Aceptar" },
          { boton: prefs.gamepadMap.atras, texto: "Cerrar" },
          { boton: prefs.gamepadMap.clic, texto: "Clic" },
        ]
      : [
          { boton: prefs.gamepadMap.lanzar, texto: "Lanzar juego" },
          { boton: prefs.gamepadMap.detalle, texto: "Opciones del juego" },
          { boton: prefs.gamepadMap.menu, texto: "Menú" },
          { boton: prefs.gamepadMap.clic, texto: "Clic" },
          { boton: prefs.gamepadMap.guia, texto: "Guía" },
          { boton: prefs.gamepadMap.ajustes, texto: "Ajustes" },
        ],
  );
</script>

<div class="hints" aria-hidden="true">
  {#each pistas as p}
    <span class="hint-item"><Tecla boton={p.boton} sm />{p.texto}</span>
  {/each}
</div>
