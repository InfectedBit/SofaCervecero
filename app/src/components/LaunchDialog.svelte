<script lang="ts">
  import { dialogo, fondoModal } from "../lib/focus";
  import type { LaunchMode, LaunchOptions } from "../lib/api";

  let {
    options,
    onclose,
    onchoose,
  }: {
    options: LaunchOptions;
    onclose: () => void;
    onchoose: (mode: LaunchMode, remember: boolean) => void;
  } = $props();

  let recordar = $state(false);
  let esSteam = $derived(options.store === "steam");
  /** Confirmaciones que exige alguna categoría del juego (C8). Hay que marcarlas todas. */
  let confirmadas = $state(new Set<string>());
  let faltaConfirmar = $derived(
    options.confirmaciones.some(([cat]) => !confirmadas.has(cat)),
  );

  function alternarConfirmacion(cat: string, marcada: boolean) {
    if (marcada) confirmadas.add(cat);
    else confirmadas.delete(cat);
    confirmadas = new Set(confirmadas);
  }

  function nombre(ruta: string | null): string {
    if (!ruta) return "—";
    return ruta.split(/[\\/]/).pop() ?? ruta;
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="overlay" role="presentation" use:fondoModal={onclose}>
  <div
    class="dialog launch"
    role="dialog"
    aria-modal="true"
    aria-label={`Cómo iniciar ${options.title}`}
    tabindex="-1"
    use:dialogo
  >
    <button class="close" onclick={onclose}>✕</button>
    <h3>¿Cómo quieres iniciar «{options.title}»?</h3>
    <p class="muted small">
      {#if esSteam}
        Es un juego de <b>Steam</b>. Elige una cosa <b>o</b> la otra.
      {:else}
        Este elemento tiene un launcher configurado. Elige una cosa <b>o</b> la otra: lanzar las
        dos a la vez hace que el launcher actualice con el juego ya abierto.
      {/if}
    </p>

    {#if options.confirmaciones.length > 0}
      <div class="confirmaciones">
        {#each options.confirmaciones as [cat, pregunta] (cat)}
          <label class="check confirmacion">
            <input
              type="checkbox"
              checked={confirmadas.has(cat)}
              onchange={(e) => alternarConfirmacion(cat, e.currentTarget.checked)}
            />
            <span>
              <b>{pregunta}</b>
              <small class="hint">Lo pide la categoría «{cat}»</small>
            </span>
          </label>
        {/each}
      </div>
    {/if}

    {#if options.account_warning}
      <p class="warn-banner">
        Este juego requiere la cuenta de Steam <b>{options.account_warning.requerida}</b>, pero
        la activa es <b>{options.account_warning.activa ?? "desconocida"}</b>. Cambia de cuenta en
        Steam antes de jugar, o puede que no arranque.
      </p>
    {/if}

    <div class="launch-options">
      <!-- svelte-ignore a11y_autofocus -->
      <button class="launch-opt primary" autofocus disabled={faltaConfirmar}
        onclick={() => onchoose("game", recordar)}>
        <span class="launch-icon">▶</span>
        <span class="launch-text">
          <b>Lanzar el juego directamente</b>
          <em>
            {#if esSteam}steam://rungameid/{options.store_id}{:else}{nombre(options.exe_path)}{/if}
          </em>
          <small>
            {#if esSteam}
              Lo abre Steam: resuelve DRM y actualizaciones por su cuenta.
            {:else}
              Si el juego es de una tienda, ya abrirá su cliente por su cuenta.
            {/if}
          </small>
        </span>
      </button>

      <button
        class="launch-opt"
        disabled={faltaConfirmar}
        onclick={() => onchoose("client", recordar)}
      >
        <span class="launch-icon">⧉</span>
        <span class="launch-text">
          <b>{esSteam ? "Abrir su ficha en Steam" : "Abrir solo el launcher"}</b>
          <em>
            {#if esSteam}steam://nav/games/details/{options.store_id}{:else}{nombre(
                options.client_path,
              )}{/if}
          </em>
          <small>Para actualizar, elegir cuenta o entrar desde su propia interfaz.</small>
        </span>
      </button>
    </div>

    <label class="check">
      <input type="checkbox" bind:checked={recordar} />
      No volver a preguntar para este juego
      <small class="hint">Se puede deshacer en Editar → «Preguntar siempre al lanzar».</small>
    </label>

    <div class="actions">
      <button class="btn" onclick={onclose}>Cancelar</button>
    </div>
  </div>
</div>
