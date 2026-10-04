<script lang="ts">
  /**
   * La cuenta atrás tras cambiar la disposición de pantallas (guión E.0_4 §5.5).
   *
   * Es la misma idea que el «¿Conservar esta configuración?» del propio panel de Windows, y por
   * la misma razón: si la pantalla que se acaba de elegir **no se ve** —el televisor apagado,
   * con el HDMI puesto, que Windows sigue dando por conectado—, nadie podría pulsar nada. Así
   * que lo que no se confirma, se deshace.
   *
   * Esto es solo la parte visible. **La cuenta atrás de verdad vive en el núcleo**: si lo que
   * falla es la propia ventana, un temporizador dentro de ella no serviría de nada.
   *
   * Es un **diálogo modal completo** y no un aviso flotante, y eso no es estética. Era una barra
   * suelta: se quedaba por debajo de otras capas, y como `role="alertdialog"` no contaba como
   * capa activa, el mando seguía navegando el grid **por detrás**. Había que mover el ratón para
   * contestar — justo lo contrario de lo que debe pasar en el momento en que acabas de encender
   * un televisor y estás en el sofá.
   */
  import { onDestroy } from "svelte";
  import { dialogo } from "../lib/focus";

  let {
    segundos = 20,
    onconfirmar,
    onrevertir,
  }: {
    segundos?: number;
    onconfirmar: () => void;
    onrevertir: () => void;
  } = $props();

  // Se captura el valor inicial a propósito: la cuenta atrás arranca una vez y no se
  // reinicia aunque cambie la prop.
  // svelte-ignore state_referenced_locally
  let restan = $state(segundos);
  const tic = setInterval(() => {
    restan -= 1;
    // Al llegar a cero no se hace nada: lo deshace el núcleo. Aquí solo se deja de prometer.
    if (restan <= 0) clearInterval(tic);
  }, 1000);
  onDestroy(() => clearInterval(tic));
</script>

<!-- Escape deshace, que es la opción segura: es lo que iba a pasar solo de todas formas. -->
<svelte:window onkeydown={(e) => e.key === "Escape" && onrevertir()} />

<!-- Sin `fondoModal`: pulsar fuera **no** cierra nada. Las dos respuestas posibles tienen
     consecuencias, así que hay que decir cuál, y un clic perdido no es ninguna de las dos. -->
<div class="overlay sofa-confirmar" role="presentation">
  <div
    class="dialog confirmar"
    role="alertdialog"
    aria-modal="true"
    aria-label="Confirmar la pantalla"
    tabindex="-1"
    use:dialogo
  >
    <h3>¿Ves bien esta pantalla?</h3>
    <p class="muted small confirmar-msg">
      {#if restan > 0}
        Si no confirmas, todo vuelve a como estaba en <b>{restan}&nbsp;s</b>.
      {:else}
        Deshaciendo…
      {/if}
    </p>
    <div class="actions">
      <button class="btn" onclick={onrevertir}>Deshacer ya</button>
      <!-- svelte-ignore a11y_autofocus -->
      <button class="btn primary" autofocus onclick={onconfirmar} disabled={restan <= 0}>
        Sí, se ve bien
      </button>
    </div>
  </div>
</div>
