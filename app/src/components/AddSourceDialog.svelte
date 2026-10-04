<script lang="ts">
  import { dialogo, fondoModal } from "../lib/focus";
  import { elegirCarpeta, elegirExe } from "../lib/rutas";
  import { api, type Category } from "../lib/api";

  let {
    categories,
    onclose,
    onadded,
  }: {
    categories: Category[];
    onclose: () => void;
    /**
     * Recibe la etiqueta para la tarea de escaneo que se lanzará por detrás, y si la carpeta
     * **ya era una fuente** — en cuyo caso no se ha añadido nada y hay que decirlo (A.0_6 §3).
     */
    onadded: (etiqueta: string, yaExistia?: boolean) => void;
  } = $props();

  let kind = $state("folder_library");
  let path = $state("");
  let categoryId = $state<number | null>(null);
  let newCategory = $state("");
  let busy = $state(false);
  let error = $state<string | null>(null);

  async function browse() {
    // Se parte de lo que ya haya escrito, para no volver siempre al mismo sitio (A12).
    const sel =
      kind === "folder_library" ? await elegirCarpeta(path) : await elegirExe(path);
    if (sel) path = sel;
  }

  /** Steam se detecta solo: no hay que pedir la ruta (guión B.0_1 §1). */
  async function detectarSteam() {
    busy = true;
    error = null;
    try {
      const ruta = await api.steamDetect();
      if (!ruta) {
        error = "No se ha encontrado Steam instalado en este equipo.";
        return;
      }
      onadded("Leyendo la biblioteca de Steam");
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function submit() {
    if (kind === "steam") {
      await detectarSteam();
      return;
    }
    if (!path.trim()) {
      error = "Indica una ruta";
      return;
    }
    busy = true;
    error = null;
    try {
      let catId = categoryId;
      if (newCategory.trim()) {
        catId = await api.categoryCreate(newCategory.trim(), null);
      }
      const r = await api.sourcesAdd(kind, path.trim(), catId);
      // El escaneo NO se espera aquí: se lanza como tarea y el diálogo se cierra ya (J3).
      // Si ya existía, se escanea igual —puede haber juegos nuevos— pero sin duplicar la fuente.
      onadded(`Escaneando ${path.trim()}`, r.ya_existia);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<div class="overlay" role="presentation" use:fondoModal={onclose}>
  <div
    class="dialog"
    role="dialog"
    aria-modal="true"
    aria-label="Añadir fuente local"
    tabindex="-1"
    use:dialogo
  >
    <button class="close" onclick={onclose}>✕</button>
    <h3>Añadir fuente local</h3>

    <label>
      Tipo de fuente
      <select bind:value={kind}>
        <option value="folder_library">Carpeta-biblioteca (varios juegos)</option>
        <option value="app_entry">App / .exe suelto</option>
        <option value="steam">Steam (detectar automáticamente)</option>
      </select>
    </label>

    {#if kind === "steam"}
      <p class="muted small">
        Se localiza Steam solo y se leen sus <b>bibliotecas</b> y lo que tienes instalado
        (<code>libraryfolders.vdf</code> + <code>appmanifest</code>). Los juegos se lanzarán
        por <code>steam://</code>, que es más fiable que el <code>.exe</code> directo.
      </p>
    {:else}
      <label>
        Ruta
        <div class="path-row">
          <input
            bind:value={path}
            placeholder={kind === "app_entry" ? "M:\\EMUL4\\Dolphin\\Dolphin.exe" : "M:\\Juegos"}
          />
          <button type="button" class="btn" onclick={browse}>Examinar…</button>
        </div>
      </label>
    {/if}

    <label>
      Categoría existente
      <select bind:value={categoryId}>
        <option value={null}>—</option>
        {#each categories as c (c.id)}
          <option value={c.id}>{c.name}</option>
        {/each}
      </select>
    </label>

    <label>
      … o crear una nueva
      <input bind:value={newCategory} placeholder="Coop Emu Games" />
    </label>

    {#if error}<p class="err">{error}</p>{/if}

    <div class="actions">
      <button class="btn" onclick={onclose}>Cancelar</button>
      <button class="btn primary" onclick={submit} disabled={busy}>
        {busy ? "Añadiendo…" : kind === "steam" ? "Detectar y escanear" : "Añadir y escanear"}
      </button>
    </div>
  </div>
</div>
