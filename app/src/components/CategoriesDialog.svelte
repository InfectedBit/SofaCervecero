<script lang="ts">
  /**
   * Editor del **árbol** de categorías (guión F.0_6).
   *
   * Antes los «grupos» eran una etiqueta de texto en cada categoría: no se podían ordenar ni
   * anidar, y renombrar uno obligaba a tocar todas sus categorías. Ahora un eje es una
   * categoría más, con `parent_id = null`.
   */
  import { dialogo, fondoModal } from "../lib/focus";
  import { confirmar } from "../lib/confirmar.svelte";
  import { porEje, raices, SUELTAS } from "../lib/categorias";
  import { api, type Category, type Recolocacion } from "../lib/api";

  let {
    categories,
    onclose,
    onchanged,
  }: {
    categories: Category[];
    onclose: () => void;
    /** La biblioteca ha cambiado → recargar sidebar y grid. */
    onchanged: () => void;
  } = $props();

  let error = $state<string | null>(null);
  let aviso = $state<string | null>(null);
  let editando = $state<number | null>(null);
  let nombreEdit = $state("");
  let nuevoNombre = $state("");
  let nuevoPadre = $state("");
  /** Categoría cuya confirmación al lanzar se está editando (C8). */
  let editandoPrompt = $state<number | null>(null);
  let textoPrompt = $state("");

  /** Recolocaciones propuestas. **Nunca se aplican solas**: el dato es del usuario. */
  let sugerencias = $state<Recolocacion[]>([]);
  let elegidas = $state(new Set<number>());
  let revisando = $state(false);

  let ejes = $derived(porEje(categories));
  let raiz = $derived(raices(categories));

  async function cargarSugerencias() {
    try {
      sugerencias = await api.categoriesSuggestMoves();
      elegidas = new Set(sugerencias.map((s) => s.id));
    } catch (e) {
      error = String(e);
    }
  }
  cargarSugerencias();

  async function hecho(msg?: string) {
    error = msg ?? null;
    onchanged();
    await cargarSugerencias();
  }

  async function crear() {
    const n = nuevoNombre.trim();
    if (!n) return;
    try {
      await api.categoryCreate(n, nuevoPadre ? Number(nuevoPadre) : null);
      nuevoNombre = "";
      await hecho();
    } catch (e) {
      error = String(e);
    }
  }

  function empezarEdicion(c: Category) {
    editando = c.id;
    nombreEdit = c.name;
  }

  async function guardarNombre(c: Category) {
    const n = nombreEdit.trim();
    editando = null;
    if (!n || n === c.name) return;
    try {
      await api.categoryRename(c.id, n);
      await hecho();
    } catch (e) {
      error = String(e);
    }
  }

  async function moverA(c: Category, padre: string) {
    try {
      await api.categorySetParent(c.id, padre ? Number(padre) : null);
      await hecho();
    } catch (e) {
      error = String(e);
    }
  }

  async function ordenar(c: Category, delta: number) {
    try {
      await api.categoryMove(c.id, delta);
      await hecho();
    } catch (e) {
      error = String(e);
    }
  }

  function empezarPrompt(c: Category) {
    editandoPrompt = c.id;
    textoPrompt = c.launch_prompt ?? "";
  }
  async function guardarPrompt(c: Category) {
    const t = textoPrompt.trim();
    editandoPrompt = null;
    if (t === (c.launch_prompt ?? "")) return;
    try {
      await api.categorySetPrompt(c.id, t || null);
      await hecho();
    } catch (e) {
      error = String(e);
    }
  }

  async function borrar(c: Category) {
    const hijas = categories.filter((x) => x.parent_id === c.id).length;
    const ok = await confirmar({
      titulo: `¿Eliminar la categoría «${c.name}»?`,
      mensaje: [
        c.count > 0
          ? `Sus ${c.count} juego(s) NO se borran: solo dejan de estar en esta categoría.`
          : "",
        hijas > 0 ? `Sus ${hijas} subcategoría(s) no se borran: pasan a ser ejes.` : "",
      ]
        .filter(Boolean)
        .join("\n")
        || undefined,
      aceptar: "Eliminar",
      peligro: true,
    });
    if (!ok) return;
    try {
      await api.categoryDelete(c.id);
      await hecho();
    } catch (e) {
      error = String(e);
    }
  }

  async function anadirPorDefecto() {
    try {
      const creadas = await api.categoriesAddDefaults();
      aviso =
        creadas.length === 0
          ? "Ya estaban todas: no se ha creado ninguna."
          : `Añadidas ${creadas.length}: ${creadas.join(", ")}.`;
      await hecho();
    } catch (e) {
      error = String(e);
    }
  }

  async function aplicarSugerencias() {
    try {
      const n = await api.categoriesApplyMoves([...elegidas]);
      aviso = `Movidas ${n} categoría(s).`;
      revisando = false;
      await hecho();
    } catch (e) {
      error = String(e);
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="overlay" role="presentation" use:fondoModal={onclose}>
  <div
    class="dialog edit"
    role="dialog"
    aria-modal="true"
    aria-label="Categorías"
    tabindex="-1"
    use:dialogo
  >
    <button class="close" onclick={onclose}>✕</button>
    <h3>🗂️ Categorías</h3>

    <p class="muted small">
      Un <b>eje</b> es una categoría sin padre: <i>Clientes</i>, <i>Nº Jugadores</i>, <i>TAGS</i>.
      Separarlos es lo que evita que un juego salga repetido — <i>Coop</i> dice con cuánta gente
      se juega, <i>LEGO</i> qué clase de juego es.
    </p>

    <!-- Propuesta de recolocación: se enseña, no se aplica (ADR-005 §8 #4). -->
    {#if sugerencias.length > 0}
      <div class="sugerencias">
        <div class="sug-cab">
          <b>{sugerencias.length} categoría(s) encajan mejor en otro eje.</b>
          <button class="btn sm" onclick={() => (revisando = !revisando)}>
            {revisando ? "Ocultar" : "Revisar"}
          </button>
        </div>
        {#if revisando}
          {#each sugerencias as s (s.id)}
            <label class="check sug-fila">
              <input
                type="checkbox"
                checked={elegidas.has(s.id)}
                onchange={(e) => {
                  const n = new Set(elegidas);
                  if (e.currentTarget.checked) n.add(s.id);
                  else n.delete(s.id);
                  elegidas = n;
                }}
              />
              <span><b>{s.name}</b> · {s.desde ?? SUELTAS} → {s.hacia}</span>
            </label>
          {/each}
          <button class="btn primary sm" onclick={aplicarSugerencias} disabled={elegidas.size === 0}>
            Aplicar {elegidas.size} cambio(s)
          </button>
        {/if}
      </div>
    {/if}

    {#if categories.length === 0}
      <p class="muted">Aún no hay categorías. Crea la primera abajo, o añade las de fábrica.</p>
    {/if}

    <div class="cat-admin">
      {#each ejes as { eje, hijas } (eje?.id ?? "sueltas")}
        <div class="cat-group">
          <div class="cat-group-head">
            {#if eje}
              {#if editando === eje.id}
                <!-- svelte-ignore a11y_autofocus -->
                <input
                  class="inline-input"
                  autofocus
                  bind:value={nombreEdit}
                  onblur={() => guardarNombre(eje)}
                  onkeydown={(e) => {
                    if (e.key === "Enter") guardarNombre(eje);
                    if (e.key === "Escape") editando = null;
                  }}
                />
              {:else}
                <button class="cat-group-name" onclick={() => empezarEdicion(eje)} title="Renombrar">
                  {eje.name}
                </button>
                <span class="src-meta">{eje.count}</span>
                <button class="btn sm" title="Subir" onclick={() => ordenar(eje, -1)}>▲</button>
                <button class="btn sm" title="Bajar" onclick={() => ordenar(eje, 1)}>▼</button>
                <button class="btn danger sm" title="Eliminar eje" onclick={() => borrar(eje)}>
                  🗑
                </button>
              {/if}
            {:else}
              <span class="cat-group-name">{SUELTAS}</span>
            {/if}
          </div>

          {#each hijas as c (c.id)}
            <div class="cat-row">
              {#if editando === c.id}
                <!-- svelte-ignore a11y_autofocus -->
                <input
                  class="inline-input"
                  autofocus
                  bind:value={nombreEdit}
                  onblur={() => guardarNombre(c)}
                  onkeydown={(e) => {
                    if (e.key === "Enter") guardarNombre(c);
                    if (e.key === "Escape") editando = null;
                  }}
                />
              {:else}
                <button class="cat-name" onclick={() => empezarEdicion(c)} title="Renombrar">
                  {c.name}
                </button>
              {/if}
              <span class="src-meta">{c.count} juego{c.count === 1 ? "" : "s"}</span>
              <button class="btn sm" title="Subir" onclick={() => ordenar(c, -1)}>▲</button>
              <button class="btn sm" title="Bajar" onclick={() => ordenar(c, 1)}>▼</button>
              <select
                value={c.parent_id === null ? "" : String(c.parent_id)}
                onchange={(e) => moverA(c, e.currentTarget.value)}
                title="Mover a otro eje"
              >
                <option value="">— sin eje —</option>
                {#each raiz.filter((r) => r.id !== c.id) as r (r.id)}
                  <option value={r.id}>{r.name}</option>
                {/each}
              </select>
              <button
                class="btn sm"
                class:primary={!!c.launch_prompt}
                title={c.launch_prompt
                  ? `Pregunta antes de lanzar: «${c.launch_prompt}»`
                  : "Pedir confirmación antes de lanzar los juegos de esta categoría"}
                onclick={() => empezarPrompt(c)}>⚠</button
              >
              <button class="btn danger sm" title="Eliminar categoría" onclick={() => borrar(c)}>
                🗑
              </button>
            </div>
            {#if editandoPrompt === c.id}
              <div class="cat-prompt">
                <!-- svelte-ignore a11y_autofocus -->
                <input
                  autofocus
                  bind:value={textoPrompt}
                  placeholder="¿Está abierta la cuenta de servicio correcta?"
                  onkeydown={(e) => {
                    if (e.key === "Enter") guardarPrompt(c);
                    if (e.key === "Escape") editandoPrompt = null;
                  }}
                />
                <button class="btn sm" onclick={() => guardarPrompt(c)}>Guardar</button>
                <small class="hint">
                  Con texto, los juegos de «{c.name}» piden esta confirmación <b>siempre</b> antes
                  de lanzarse. Vacío = sin confirmación.
                </small>
              </div>
            {/if}
          {/each}
        </div>
      {/each}
    </div>

    <div class="section">
      <div class="section-title">Nueva categoría</div>
      <div class="grid2">
        <label>Nombre<input bind:value={nuevoNombre} placeholder="Couch Co-Op" /></label>
        <label>
          Dentro del eje
          <select bind:value={nuevoPadre}>
            <option value="">— ninguno (será un eje) —</option>
            {#each raiz as r (r.id)}<option value={r.id}>{r.name}</option>{/each}
          </select>
        </label>
      </div>
      <div class="actions between">
        <button class="btn" onclick={anadirPorDefecto} title="No duplica ni pisa lo que ya tengas">
          ＋ Añadir las de fábrica que falten
        </button>
        <button class="btn" onclick={crear}>＋ Crear</button>
      </div>
    </div>

    {#if aviso}<p class="muted small">{aviso}</p>{/if}
    {#if error}<p class="err">{error}</p>{/if}

    <div class="actions">
      <button class="btn primary" onclick={onclose}>Cerrar</button>
    </div>
  </div>
</div>
