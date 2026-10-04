<script lang="ts">
  import { dialogo, fondoModal } from "../lib/focus";
  import { confirmar } from "../lib/confirmar.svelte";
  import { elegirExe, elegirImagen } from "../lib/rutas";
  import Caratula from "./Caratula.svelte";
  import { api, type Category, type GameDetail, type Executable } from "../lib/api";
  import { porEje, raices, SUELTAS } from "../lib/categorias";

  let {
    detail,
    categories,
    onclose,
    onsaved,
    ondeleted,
  }: {
    detail: GameDetail;
    categories: Category[];
    onclose: () => void;
    onsaved: () => void;
    ondeleted: () => void;
  } = $props();

  // Copia editable de los datos del juego. Es intencionado que capture el valor inicial
  // (svelte avisa con `state_referenced_locally`): el diálogo vive dentro de un `{#if}` y se
  // recrea en cada apertura, así que no debe re-sincronizarse mientras se está editando.
  // svelte-ignore state_referenced_locally
  let title = $state(detail.title);
  // svelte-ignore state_referenced_locally
  let platform = $state(detail.platform);
  // svelte-ignore state_referenced_locally
  let exePath = $state(detail.exe_path ?? "");
  // svelte-ignore state_referenced_locally
  let exeArgs = $state(detail.exe_args ?? "");
  // svelte-ignore state_referenced_locally
  let clientFirst = $state(detail.client_first);
  // svelte-ignore state_referenced_locally
  let clientPath = $state(detail.client_path ?? "");
  // svelte-ignore state_referenced_locally
  let clientArgs = $state(detail.client_args ?? "");
  // Ojo: en un `<input type="number">` Svelte **convierte el valor a número** (y a `null` si
  // se vacía). Guardarlos como texto y llamar a `.trim()` al guardar reventaba con
  // *"n(...).trim is not a function"* en cuanto se tocaba el campo.
  // svelte-ignore state_referenced_locally
  let playersMin = $state<number | null>(detail.players_min);
  // svelte-ignore state_referenced_locally
  let playersMax = $state<number | null>(detail.players_max);
  // `launch_mode = null` ⇒ preguntar siempre (guión C.0_3 §3).
  // svelte-ignore state_referenced_locally
  let preguntarSiempre = $state(detail.launch_mode === null);
  // svelte-ignore state_referenced_locally
  let steamAccount = $state(detail.steam_account ?? "");
  /**
   * Carátula puesta a mano. Vacío = manda la automática (`art_path`).
   *
   * Ojo con esto, que fue un fallo real (D.0_4 §1): el core devolvía aquí
   * `COALESCE(cover_path, art_path)`, así que este campo se rellenaba con la carátula
   * **automática** y al pulsar *Guardar* la ascendía a manual. A partir de ese momento el juego
   * quedaba fuera del alcance de «Rehacer» — 27 juegos acabaron con el icono de su `.exe`
   * congelado. Ahora `detail.cover_path` es solo la del usuario, y la automática va aparte.
   */
  // svelte-ignore state_referenced_locally
  let coverPath = $state(detail.cover_path ?? "");
  let buscandoArte = $state(false);
  let arteMsg = $state<string | null>(null);
  /** Error al abrir la carpeta; se enseña junto al botón, no en el aviso general. */
  let carpetaMsg = $state<string | null>(null);
  /** Arte resuelta por la cascada; se refresca al pulsar «Buscar». Nunca se guarda. */
  // svelte-ignore state_referenced_locally
  let arteAuto = $state<string | null>(detail.art_path);
  // svelte-ignore state_referenced_locally
  let execs = $state<Executable[]>([...detail.executables]);
  // svelte-ignore state_referenced_locally
  let cats = $state<Category[]>([...categories]);
  // svelte-ignore state_referenced_locally
  let assigned = $state(new Set(detail.categories));

  let newExecLabel = $state("");
  let newExecPath = $state("");
  let newExecArgs = $state("");
  let newCategory = $state("");
  let newCategoryGroup = $state("");
  let creandoCategoria = $state(false);

  /** Categorías en las que ya está, para pintarlas como chips. */
  let asignadas = $derived(cats.filter((c) => assigned.has(c.name)));
  /** Las demás, agrupadas por eje, para el desplegable. */
  let porGrupo = $derived(
    porEje(cats).map((x) => [x.eje?.name ?? SUELTAS, x.hijas] as const),
  );
  /** Ejes existentes, para elegir dónde cae una categoría nueva. */
  let grupos = $derived(raices(cats));
  let busy = $state(false);
  let error = $state<string | null>(null);

  // El explorador se abre donde estás: el campo que se edita manda, y si está vacío se usa
  // la carpeta de instalación del juego (guión A.0_3, A12).
  async function browseMain() {
    const p = await elegirExe(exePath, detail.install_dir);
    if (p) exePath = p;
  }
  async function browseClient() {
    const p = await elegirExe(clientPath, detail.install_dir);
    if (p) clientPath = p;
  }
  async function browseNewExec() {
    const p = await elegirExe(newExecPath, exePath, detail.install_dir);
    if (p) newExecPath = p;
  }

  async function examinarCaratula() {
    const p = await elegirImagen(coverPath, detail.install_dir);
    if (p) {
      coverPath = p;
      arteMsg = null;
    }
  }

  /** Vuelve a pasar la cascada sin APIs para este juego concreto. */
  async function buscarCaratula() {
    buscandoArte = true;
    arteMsg = null;
    try {
      const p = await api.gameArtRefresh(detail.id);
      arteMsg = p
        ? `Encontrada: ${p.split(/[\\/]/).pop()}`
        : "No se ha encontrado nada en local. Puedes indicarla a mano arriba.";
      // La previa pasa a mostrar la recién encontrada (si no hay una puesta a mano).
      arteAuto = p;
    } catch (e) {
      arteMsg = String(e);
    } finally {
      buscandoArte = false;
    }
  }

  function quitarCaratula() {
    coverPath = "";
    arteMsg = null;
  }

  async function addExec() {
    if (!newExecLabel.trim() || !newExecPath.trim()) {
      error = "La etiqueta y la ruta del ejecutable son obligatorias";
      return;
    }
    error = null;
    const args = newExecArgs.trim() || null;
    const id = await api.executableAdd(detail.id, newExecLabel.trim(), newExecPath.trim(), args);
    execs = [...execs, { id, label: newExecLabel.trim(), path: newExecPath.trim(), args }];
    newExecLabel = "";
    newExecPath = "";
    newExecArgs = "";
  }
  async function removeExec(id: number) {
    await api.executableDelete(id);
    execs = execs.filter((e) => e.id !== id);
  }

  async function toggleCategory(c: Category, checked: boolean) {
    if (checked) {
      await api.categoryAssign(detail.id, c.id);
      assigned.add(c.name);
    } else {
      await api.categoryUnassign(detail.id, c.id);
      assigned.delete(c.name);
    }
    assigned = new Set(assigned);
  }
  async function createCategory() {
    const name = newCategory.trim();
    if (!name) return;
    const padre = newCategoryGroup ? Number(newCategoryGroup) : null;
    const id = await api.categoryCreate(name, padre);
    await api.categoryAssign(detail.id, id);
    if (!cats.some((c) => c.id === id)) {
      cats = [...cats, { id, name, parent_id: padre, sort_order: 999, count: 1, launch_prompt: null }];
    }
    assigned.add(name);
    assigned = new Set(assigned);
    newCategory = "";
    newCategoryGroup = "";
    creandoCategoria = false;
  }

  /** Vacío, cero o basura ⇒ «sin dato»; el resto, entero. */
  function jugadores(v: number | null): number | null {
    return v === null || !Number.isFinite(v) || v < 1 ? null : Math.trunc(v);
  }

  async function save() {
    busy = true;
    error = null;
    try {
      await api.gameSetCover(detail.id, coverPath.trim() || null);
      await api.gameUpdate(detail.id, {
        title: title.trim(),
        platform: platform.trim() || "PC",
        exePath: exePath.trim() || null,
        exeArgs: exeArgs.trim() || null,
        // Si se desmarca "tiene launcher", se limpia la ruta: si no, el popup de
        // lanzamiento seguiría apareciendo por un launcher que ya no se quiere.
        clientPath: clientFirst ? clientPath.trim() || null : null,
        clientArgs: clientFirst ? clientArgs.trim() || null : null,
        clientFirst,
        playersMin: jugadores(playersMin),
        playersMax: jugadores(playersMax),
        launchMode: preguntarSiempre ? null : detail.launch_mode,
        steamAccount: steamAccount.trim() || null,
      });
      onsaved();
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  /**
   * Abre la carpeta del juego en el explorador. El fallo se enseña **aquí** en vez de en el
   * aviso general: el editor es justo donde se corrige la ruta que el mensaje nombra.
   */
  async function abrirCarpeta() {
    carpetaMsg = null;
    try {
      await api.gameOpenDir(detail.id);
    } catch (e) {
      carpetaMsg = String(e);
    }
  }

  async function del() {
    const ok = await confirmar({
      titulo: `¿Eliminar "${detail.title}" por completo?`,
      mensaje: "Se pierden sus ediciones, categorías y tiempo jugado. Volverá a aparecer si lo encuentra un escaneo.",
      aceptar: "Eliminar",
      peligro: true,
    });
    if (!ok) return;
    busy = true;
    try {
      await api.gameDelete(detail.id);
      ondeleted();
    } catch (e) {
      error = String(e);
      busy = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="overlay" role="presentation" use:fondoModal={onclose}>
  <div
    class="dialog edit"
    role="dialog"
    aria-modal="true"
    aria-label={`Editar ${detail.title}`}
    tabindex="-1"
    use:dialogo
  >
    <button class="close" onclick={onclose}>✕</button>
    <h3>Editar: {detail.title}</h3>

    <div class="grid2">
      <label>Título<input bind:value={title} /></label>
      <label>Plataforma<input bind:value={platform} /></label>
    </div>

    <div class="grid2">
      <label>
        Jugadores (mín.)
        <input type="number" min="1" bind:value={playersMin} placeholder="1" />
      </label>
      <label>
        Jugadores (máx.)
        <input type="number" min="1" bind:value={playersMax} placeholder="4" />
        <small class="hint">Permite filtrar por coop / nº de jugadores.</small>
      </label>
    </div>

    <!-- Carátula del juego: definirla a mano o volver a buscarla (guión D.0_2 §5). -->
    <div class="section">
      <div class="section-title">Carátula</div>
      <div class="caratula-edit">
        <div class="caratula-previa">
          <Caratula
            titulo={detail.title}
            cover={coverPath || arteAuto}
            storeId={detail.store_id}
          />
        </div>
        <div class="caratula-acciones">
          <div class="path-row">
            <input bind:value={coverPath} placeholder="ruta a una imagen (opcional)" />
            <button type="button" class="btn" onclick={examinarCaratula}>Examinar…</button>
          </div>
          <div class="path-row">
            <button class="btn" onclick={buscarCaratula} disabled={buscandoArte}>
              {buscandoArte ? "Buscando…" : "🔍 Buscar automáticamente"}
            </button>
            <button class="btn" onclick={quitarCaratula} disabled={!coverPath}>Quitar la mía</button>
          </div>
          {#if arteMsg}<small class="hint">{arteMsg}</small>{/if}
          <small class="hint">
            {#if coverPath}
              <b>Carátula propia</b>: ni los re-escaneos ni «Rehacer» la tocan. Quítala para que
              vuelva a mandar la automática.
            {:else if arteAuto}
              Automática, de la cascada: <code>{arteAuto.split(/[\\/]/).pop()}</code>. Pon una
              ruta arriba para fijar la tuya.
            {:else}
              Sin carátula. Pon una ruta arriba, o pulsa <b>Buscar automáticamente</b>.
            {/if}
          </small>
        </div>
      </div>
    </div>

    <label>
      Ejecutable principal
      <div class="path-row">
        <input bind:value={exePath} placeholder="ruta al .exe principal" />
        <button type="button" class="btn" onclick={browseMain}>Examinar…</button>
      </div>
    </label>

    <!-- Dónde vive el juego. No es editable: lo pone el escaneo, y enseñarlo aquí es lo que
         permite entender un «no existe» al lanzar sin salir del editor (A.0_5 §2). -->
    {#if detail.install_dir || detail.exe_path}
      <label>
        Carpeta del juego
        <div class="path-row">
          <input value={detail.install_dir ?? detail.exe_path} readonly />
          <button type="button" class="btn" onclick={abrirCarpeta}>📂 Abrir carpeta</button>
        </div>
        <small class="hint">
          {carpetaMsg ?? "La pone el escaneo. Se abre el explorador con el ejecutable seleccionado."}
        </small>
      </label>
    {/if}

    <label>
      Argumentos del ejecutable principal (opcional)
      <input bind:value={exeArgs} placeholder={'-windowed  ·  -e "M:\\Mis Juegos\\rom.iso"'} />
      <small class="hint">Las rutas con espacios van entre comillas dobles.</small>
    </label>

    <div class="section">
      <div class="section-title">Ejecutables adicionales</div>
      {#each execs as ex (ex.id)}
        <div class="exec-row">
          <span class="exec-label">{ex.label}</span>
          <span class="exec-path" title={ex.path}>{ex.path}</span>
          <button class="btn danger sm" onclick={() => removeExec(ex.id)}>✕</button>
        </div>
      {/each}
      <div class="exec-add">
        <input placeholder="Etiqueta (Launcher, BattlEye…)" bind:value={newExecLabel} />
        <div class="path-row">
          <input placeholder="ruta al .exe" bind:value={newExecPath} />
          <button type="button" class="btn" onclick={browseNewExec}>…</button>
        </div>
        <input placeholder="args (opcional)" bind:value={newExecArgs} />
        <button class="btn" onclick={addExec}>Añadir ejecutable</button>
      </div>
    </div>

    <div class="section">
      <label class="check">
        <input type="checkbox" bind:checked={clientFirst} />
        Este juego tiene launcher/cliente (Steam, Epic, launcher propio…)
        <small class="hint">
          Al pulsar LANZAR se preguntará si abrir el juego o el launcher, nunca los dos a la vez.
        </small>
      </label>
      {#if clientFirst}
        <label>
          Ruta del cliente/launcher
          <div class="path-row">
            <input bind:value={clientPath} placeholder="p.ej. …\Steam\steam.exe" />
            <button type="button" class="btn" onclick={browseClient}>Examinar…</button>
          </div>
        </label>
        <label>Args del cliente (opcional)<input bind:value={clientArgs} /></label>
        <label class="check">
          <input type="checkbox" bind:checked={preguntarSiempre} />
          Preguntar siempre al lanzar («¿launcher o juego directo?»)
          <small class="hint">
            Desmarcado, se conserva la elección recordada
            {#if detail.launch_mode}(ahora: {detail.launch_mode === "game"
                ? "solo el juego"
                : "solo el launcher"}){/if}.
          </small>
        </label>
      {/if}
    </div>

    {#if detail.store === "steam"}
      <div class="section">
        <div class="section-title">Steam · appid {detail.store_id}</div>
        <label>
          Cuenta necesaria para jugarlo (opcional)
          <input bind:value={steamAccount} placeholder="nombre de inicio de sesión" />
          <small class="hint">
            Si la cuenta activa es otra, se avisa antes de lanzar. Útil para juegos que están en
            una cuenta concreta.
          </small>
        </label>
      </div>
    {/if}

    <!-- Lista desplegable en vez de una casilla por categoría (guión F.0_4, F9): con
         veinte categorías, veinte casillas no se pueden mirar. -->
    <div class="section">
      <div class="section-title">Categorías</div>

      {#if asignadas.length > 0}
        <div class="cats">
          {#each asignadas as c (c.id)}
            <span class="chip" role="group">
              {c.name}
              <button
                class="chip-x"
                title="Quitar de esta categoría"
                onclick={() => toggleCategory(c, false)}>✕</button
              >
            </span>
          {/each}
        </div>
      {:else}
        <p class="muted small">Todavía no está en ninguna categoría.</p>
      {/if}

      <label>
        Añadir a una categoría
        <select
          value=""
          onchange={(e) => {
            const v = e.currentTarget.value;
            e.currentTarget.value = "";
            if (v === "__nueva__") creandoCategoria = true;
            else if (v) {
              const c = cats.find((x) => String(x.id) === v);
              if (c) toggleCategory(c, true);
            }
          }}
        >
          <option value="">Elegir…</option>
          {#each porGrupo as [grupo, lista]}
            <optgroup label={grupo}>
              {#each lista as c (c.id)}
                <option value={c.id} disabled={assigned.has(c.name)}>{c.name}</option>
              {/each}
            </optgroup>
          {/each}
          <option value="__nueva__">＋ Crear nueva categoría…</option>
        </select>
      </label>

      {#if creandoCategoria}
        <div class="path-row">
          <!-- svelte-ignore a11y_autofocus -->
          <input
            autofocus
            placeholder="nombre de la categoría"
            bind:value={newCategory}
            onkeydown={(e) => {
              if (e.key === "Enter") createCategory();
              if (e.key === "Escape") creandoCategoria = false;
            }}
          />
          <input placeholder="grupo (opcional)" bind:value={newCategoryGroup} list="grupos-edit" />
          <datalist id="grupos-edit">
            {#each grupos as g}<option value={g}></option>{/each}
          </datalist>
          <button class="btn" onclick={createCategory}>Crear y añadir</button>
        </div>
      {/if}
    </div>

    {#if error}<p class="err">{error}</p>{/if}

    <div class="actions between">
      <button class="btn danger" onclick={del} disabled={busy}>🗑 Eliminar</button>
      <div class="right-actions">
        <button class="btn" onclick={onclose}>Cancelar</button>
        <button class="btn primary" onclick={save} disabled={busy}>
          {busy ? "Guardando…" : "Guardar"}
        </button>
      </div>
    </div>
  </div>
</div>
