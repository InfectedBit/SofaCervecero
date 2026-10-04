<script lang="ts">
  import { dialogo, fondoModal } from "../lib/focus";
  import { confirmacion, responder } from "../lib/confirmar.svelte";

  let p = $derived(confirmacion.actual);
</script>

<!-- Escape siempre cancela. Va fuera del `{#if}` porque `<svelte:window>` solo puede estar
     en el nivel más alto; la guarda la pone el propio manejador. -->
<svelte:window onkeydown={(e) => p && e.key === "Escape" && responder(false)} />

{#if p}
  <div class="overlay" role="presentation" use:fondoModal={() => responder(false)}>
    <div
      class="dialog confirmar"
      role="dialog"
      aria-modal="true"
      aria-label={p.titulo}
      tabindex="-1"
      use:dialogo
    >
      <h3>{p.titulo}</h3>
      {#if p.mensaje}<p class="muted small confirmar-msg">{p.mensaje}</p>{/if}
      <div class="actions">
        <button class="btn" onclick={() => responder(false)}>{p.cancelar ?? "Cancelar"}</button>
        <!-- svelte-ignore a11y_autofocus -->
        <button
          class="btn"
          class:primary={!p.peligro}
          class:danger={p.peligro}
          autofocus
          onclick={() => responder(true)}>{p.aceptar ?? "Aceptar"}</button
        >
      </div>
    </div>
  </div>
{/if}
