<script lang="ts">
  /**
   * La pregunta de entrada al Modo Sofá (guión E.0_4 §6.1).
   *
   * Solo aparece **si hay más de una pantalla**. Con una sola no hay nada que elegir y entrar
   * directo es lo correcto; quien lo abra con un monitor no debería ver un diálogo que solo
   * tiene una opción.
   *
   * En esta fase no se pregunta aún qué hacer con las demás pantallas (cambiar la principal o
   * apagarlas): eso es la fase 3. Aquí solo se coloca el hub, que no cambia nada del sistema y
   * por tanto no necesita advertencia.
   */
  import { dialogo, fondoModal } from "../lib/focus";
  import { api, type Pantalla, type Reparto } from "../lib/api";
  import SelectorPantalla from "./SelectorPantalla.svelte";

  let {
    onclose,
    onentrar,
  }: {
    onclose: () => void;
    onentrar: (monitor: string | null, reparto: Reparto) => void;
  } = $props();

  /**
   * Qué hacer con las demás pantallas. Solo se pregunta cuando el destino **no** es la principal:
   * si ya lo es, no hay nada que repartir (guión E.0_4 §5.2).
   *
   * Por defecto, **primaria**: es reversible sin consecuencias y resuelve el arranque de los
   * juegos, que es lo que se busca. «Solo esa» queda para quien la pida.
   */
  let reparto = $state<Reparto>("primaria");

  let pantallas = $state<Pantalla[]>([]);
  let elegida = $state<string | null>(null);
  let error = $state<string | null>(null);
  let cargando = $state(true);

  api
    .monitorsList()
    .then((p) => {
      pantallas = p;
      elegida = p.find((x) => x.primaria)?.id ?? p[0]?.id ?? null;
      cargando = false;
      // Una sola pantalla: no hay nada que preguntar.
      if (p.length === 1) onentrar(null, "ninguna");
    })
    .catch((e) => {
      error = String(e);
      cargando = false;
    });

  let destino = $derived(pantallas.find((p) => p.id === elegida));
  let principal = $derived(pantallas.find((p) => p.primaria));
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="overlay" role="presentation" use:fondoModal={onclose}>
  <div class="dialog" role="dialog" aria-modal="true" aria-label="Modo Sofá" tabindex="-1" use:dialogo>
    <button class="close" onclick={onclose}>✕</button>
    <h3>🛋 Modo Sofá</h3>

    {#if cargando}
      <p class="muted small">Leyendo las pantallas…</p>
    {:else if error}
      <p class="err">{error}</p>
    {:else}
      <p class="muted small">¿En qué pantalla quieres el hub?</p>
      <SelectorPantalla {pantallas} bind:elegida />

      {#if destino?.primaria}
        <small class="hint">
          Es la pantalla que ya estás usando: el hub se pondrá a pantalla completa aquí mismo y
          <b>no se tocará nada del sistema</b>.
        </small>
      {:else if destino}
        <!-- La pregunta, en los términos del usuario y no en los del sistema: lo que está
             pasando es que se levanta y se pone delante de otra pantalla. -->
        <div class="section-title">
          Vas a pasar de <b>{principal?.nombre ?? "tu pantalla"}</b> a <b>{destino.nombre}</b>
        </div>
        <div class="reparto">
          <label class="check">
            <input type="radio" bind:group={reparto} value="primaria" />
            <span>
              <b>Hacer principal {destino.nombre}</b>
              <small class="hint">
                Lo normal. Casi todos los juegos abren en la pantalla principal, así que con esto
                ya caen aquí. Las demás siguen encendidas. Se deshace al salir.
              </small>
            </span>
          </label>
          <label class="check">
            <input type="radio" bind:group={reparto} value="solo" />
            <span>
              <b>Dejar solo {destino.nombre}</b>
              <small class="hint">
                Lo más contundente: se apagan las demás, así que <b>todo</b> abre aquí. Las
                ventanas que tengas abiertas se amontonarán en esta pantalla y <b>no volverán
                solas</b> a su sitio al salir.
              </small>
            </span>
          </label>
          <label class="check">
            <input type="radio" bind:group={reparto} value="ninguna" />
            <span>
              <b>No tocar nada</b>
              <small class="hint">
                Solo se mueve el hub. Los juegos seguirán abriéndose donde cada uno acostumbre.
              </small>
            </span>
          </label>
        </div>
        {#if reparto !== "ninguna"}
          <small class="hint">
            🛟 Tras el cambio tendrás <b>20 segundos</b> para confirmar que ves bien la pantalla.
            Si no, se deshace solo — por si el televisor está apagado.
          </small>
        {/if}
      {/if}

      <div class="actions">
        <button class="btn" onclick={onclose}>Cancelar</button>
        <button
          class="btn primary"
          onclick={() => onentrar(elegida, destino?.primaria ? "ninguna" : reparto)}
        >Entrar en Modo Sofá</button>
      </div>
    {/if}
  </div>
</div>
