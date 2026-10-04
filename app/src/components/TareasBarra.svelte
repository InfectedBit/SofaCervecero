<script lang="ts">
  import { tareas, cancelarTarea, quitarTarea } from "../lib/tareas.svelte";

  /** Indicador de tareas en segundo plano en la barra superior (guión J.0_1). */
  let abierto = $state(false);
  let raiz = $state<HTMLElement | null>(null);

  let corriendo = $derived(tareas.lista.filter((t) => t.estado === "corriendo"));
  let actual = $derived(corriendo[0] ?? null);

  function fuera(e: MouseEvent) {
    if (abierto && raiz && !raiz.contains(e.target as Node)) abierto = false;
  }

  function icono(estado: string): string {
    return { ok: "✓", error: "✕", cancelada: "⊘" }[estado] ?? "…";
  }
</script>

<svelte:window onmousedown={fuera} />

{#if tareas.lista.length > 0}
  <div class="tareas" bind:this={raiz}>
    <button
      class="tareas-chip"
      class:activa={corriendo.length > 0}
      aria-expanded={abierto}
      onclick={() => (abierto = !abierto)}
      title="Tareas en segundo plano"
    >
      {#if corriendo.length > 0}
        <span class="girando">◴</span>
        <span class="tareas-texto">
          {actual?.progreso?.current ?? actual?.etiqueta ?? "Trabajando…"}
        </span>
        {#if corriendo.length > 1}<span class="tareas-mas">+{corriendo.length - 1}</span>{/if}
      {:else}
        <span>✓ Listo</span>
      {/if}
    </button>

    {#if abierto}
      <div class="tareas-pop">
        {#each tareas.lista as t (t.id)}
          <div class="tarea-fila">
            <span class="tarea-estado" class:err={t.estado === "error"}>{icono(t.estado)}</span>
            <span class="tarea-texto">
              <b>{t.etiqueta}</b>
              {#if t.progreso}
                <em>
                  {t.progreso.current}
                  {#if t.progreso.source_total > 1}
                    · {t.progreso.source_index}/{t.progreso.source_total}
                  {/if}
                  · {t.progreso.found} encontrados
                </em>
              {:else if t.detalle}
                <em>{t.detalle}</em>
              {/if}
            </span>
            {#if t.estado === "corriendo"}
              <button class="btn danger sm" onclick={() => cancelarTarea(t.id)}>Cancelar</button>
            {:else}
              <button class="btn sm" onclick={() => quitarTarea(t.id)}>✕</button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  </div>
{/if}
