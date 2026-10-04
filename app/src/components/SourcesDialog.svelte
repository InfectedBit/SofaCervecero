<script lang="ts">
  import { dialogo, fondoModal } from "../lib/focus";
  import { confirmar } from "../lib/confirmar.svelte";
  import { api, type Category, type Source } from "../lib/api";

  let {
    categories,
    onclose,
    onadd,
    onchanged,
  }: {
    categories: Category[];
    onclose: () => void;
    onadd: () => void;
    /** La biblioteca ha cambiado (fuente eliminada o desactivada) → refrescar grid. */
    onchanged: () => void;
  } = $props();

  let sources = $state<Source[]>([]);
  /** Categorías asignadas a cada fuente (F10): se aplican a todo lo que venga de ella. */
  let catsPorFuente = $state<Record<number, number[]>>({});
  let expandida = $state<number | null>(null);
  let cargando = $state(true);
  let error = $state<string | null>(null);

  async function cargar() {
    cargando = true;
    try {
      sources = await api.sourcesList();
      const pares = await Promise.all(
        sources.map(async (s) => [s.id, await api.sourceCategories(s.id)] as const),
      );
      catsPorFuente = Object.fromEntries(pares);
      error = null;
    } catch (e) {
      error = String(e);
    } finally {
      cargando = false;
    }
  }

  function tipo(kind: string): string {
    return (
      { folder_library: "Carpeta-biblioteca", app_entry: "App / .exe", steam: "Steam" }[kind] ??
      kind
    );
  }

  async function alternar(s: Source) {
    try {
      await api.sourcesSetEnabled(s.id, !s.enabled);
      await cargar();
      onchanged();
    } catch (e) {
      error = String(e);
    }
  }

  /**
   * Asigna una categoría a la fuente entera. Se pregunta si aplicarla también a lo que ya
   * está — incluido el histórico, que es justo el caso *"o hayan estado instalados"* (F10).
   */
  async function asignarCategoria(s: Source, categoryId: number) {
    const c = categories.find((x) => x.id === categoryId);
    const retroactivo = await confirmar({
      titulo: `¿Aplicar «${c?.name}» a los ${s.game_count} juego(s) que ya hay?`,
      mensaje:
        "Incluye los del histórico (desinstalados). Si no, la categoría solo se aplicará a lo que se añada a partir de ahora.",
      aceptar: "Aplicar a todos",
      cancelar: "Solo a lo nuevo",
    });
    try {
      const n = await api.sourceCategoryAssign(s.id, categoryId, retroactivo);
      await cargar();
      onchanged();
      if (retroactivo) error = null;
      if (retroactivo && n === 0) error = "No había juegos que etiquetar en esa fuente.";
    } catch (e) {
      error = String(e);
    }
  }

  async function quitarCategoria(s: Source, categoryId: number) {
    const c = categories.find((x) => x.id === categoryId);
    const limpiar = await confirmar({
      titulo: `¿Quitar «${c?.name}» también de los juegos de esta fuente?`,
      mensaje: "Si no, la categoría solo deja de aplicarse a lo nuevo.",
      aceptar: "Quitar de todos",
      cancelar: "Solo de lo nuevo",
    });
    try {
      await api.sourceCategoryUnassign(s.id, categoryId, limpiar);
      await cargar();
      onchanged();
    } catch (e) {
      error = String(e);
    }
  }

  async function eliminar(s: Source) {
    const ok = await confirmar({
      titulo: "¿Eliminar la fuente?",
      mensaje: s.path,
      aceptar: "Eliminar",
      peligro: true,
    });
    if (!ok) return;
    let conJuegos = false;
    if (s.game_count > 0) {
      conJuegos = await confirmar({
        titulo: `¿Eliminar también sus ${s.game_count} juego(s) de la biblioteca?`,
        mensaje: "Se pierden sus ediciones y categorías. Si no, se quedan en la biblioteca sin fuente.",
        aceptar: "Borrarlos también",
        cancelar: "Conservarlos",
        peligro: true,
      });
    }
    try {
      await api.sourcesRemove(s.id, conJuegos);
      await cargar();
      onchanged();
    } catch (e) {
      error = String(e);
    }
  }

  cargar();
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="overlay" role="presentation" use:fondoModal={onclose}>
  <div
    class="dialog edit"
    role="dialog"
    aria-modal="true"
    aria-label="Fuentes de la biblioteca"
    tabindex="-1"
    use:dialogo
  >
    <button class="close" onclick={onclose}>✕</button>
    <h3>Fuentes de la biblioteca</h3>

    {#if cargando}
      <p class="muted">Cargando…</p>
    {:else if sources.length === 0}
      <p class="muted">Todavía no hay ninguna fuente. Añade una carpeta-biblioteca o una app.</p>
    {:else}
      <div class="src-list">
        {#each sources as s (s.id)}
          <div class="src-row" class:off={!s.enabled}>
            <label class="check" title="Las fuentes desactivadas se saltan al escanear">
              <input type="checkbox" checked={s.enabled} onchange={() => alternar(s)} />
              <span class="src-kind">{tipo(s.kind)}</span>
            </label>
            <span class="src-path" title={s.path}>{s.path}</span>
            <span class="src-meta">
              {s.game_count} juego{s.game_count === 1 ? "" : "s"}
              {#if (catsPorFuente[s.id] ?? []).length > 0}
                · {(catsPorFuente[s.id] ?? []).length} categoría(s)
              {/if}
            </span>
            <button
              class="btn sm"
              class:primary={(catsPorFuente[s.id] ?? []).length > 0}
              title="Categorías que se aplican a toda la fuente"
              onclick={() => (expandida = expandida === s.id ? null : s.id)}>🗂️</button
            >
            <button class="btn danger sm" title="Eliminar fuente" onclick={() => eliminar(s)}>
              🗑
            </button>
          </div>
          {#if expandida === s.id}
            <div class="src-cats">
              <div class="cats">
                {#each catsPorFuente[s.id] ?? [] as cid (cid)}
                  <span class="chip">
                    {categories.find((c) => c.id === cid)?.name ?? "?"}
                    <button class="chip-x" onclick={() => quitarCategoria(s, cid)}>✕</button>
                  </span>
                {:else}
                  <span class="muted small">Sin categorías propias.</span>
                {/each}
              </div>
              <select
                value=""
                onchange={(e) => {
                  const v = e.currentTarget.value;
                  e.currentTarget.value = "";
                  if (v) asignarCategoria(s, Number(v));
                }}
              >
                <option value="">Añadir categoría a toda la fuente…</option>
                {#each categories.filter((c) => !(catsPorFuente[s.id] ?? []).includes(c.id)) as c (c.id)}
                  <option value={c.id}>{c.name}</option>
                {/each}
              </select>
              <small class="hint">
                Todo lo que venga de esta fuente entrará en estas categorías.
              </small>
            </div>
          {/if}
        {/each}
      </div>
    {/if}

    {#if error}<p class="err">{error}</p>{/if}

    <div class="actions between">
      <button class="btn" onclick={onadd}>＋ Añadir fuente</button>
      <button class="btn primary" onclick={onclose}>Cerrar</button>
    </div>
  </div>
</div>
