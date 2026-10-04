<script lang="ts">
  import { dialogo, fondoModal } from "../lib/focus";
  import { prefs } from "../lib/prefs.svelte";
  import { ACCIONES } from "../lib/gamepad";
  import Tecla from "./Tecla.svelte";

  let { onclose, onajustes }: { onclose: () => void; onajustes: () => void } = $props();
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="overlay" role="presentation" use:fondoModal={onclose}>
  <div
    class="dialog guia"
    role="dialog"
    aria-modal="true"
    aria-label="Guía de botones del mando"
    tabindex="-1"
    use:dialogo
  >
    <button class="close" onclick={onclose}>✕</button>
    <h3>🎮 Botones del mando</h3>

    <div class="guia-grid">
      <div class="guia-fila">
        <span class="tecla">✛</span>
        <span class="guia-texto">
          <b>Moverse</b>
          <em>Cruceta — salta de opción en opción, por el grid y por los menús</em>
        </span>
      </div>
      <div class="guia-fila">
        <span class="tecla">⊙</span>
        <span class="guia-texto">
          <b>Desplazar el panel</b>
          <em>Stick izquierdo — como la rueda del ratón</em>
        </span>
      </div>
      <div class="guia-fila">
        <span class="tecla">⊚</span>
        <span class="guia-texto">
          <b>Mover el cursor</b>
          <em>Stick derecho — para llegar a cualquier sitio como con el ratón</em>
        </span>
      </div>
      <!-- Esto ya funcionaba y no lo sabía nadie: arriba y abajo saltan de control en control,
           así que un desplegable enfocado se cambia **con izquierda y derecha**. -->
      <div class="guia-fila">
        <span class="tecla">◀▶</span>
        <span class="guia-texto">
          <b>Cambiar un desplegable</b>
          <em>Izquierda y derecha, con el desplegable enfocado — sin abrirlo</em>
        </span>
      </div>
      {#each ACCIONES as a (a.id)}
        <div class="guia-fila">
          <Tecla boton={prefs.gamepadMap[a.id]} />
          <span class="guia-texto">
            <b>{a.etiqueta}</b>
            <em>{a.ayuda}</em>
          </span>
        </div>
      {/each}
    </div>

    <div class="actions between">
      <button class="btn" onclick={onajustes}>⚙ Cambiar botones</button>
      <button class="btn primary" onclick={onclose}>Cerrar</button>
    </div>
  </div>
</div>
