<script lang="ts">
  import { dialogo, fondoModal } from "../lib/focus";
  import { confirmar } from "../lib/confirmar.svelte";
  import MenuAcciones from "./MenuAcciones.svelte";
  import { fmtTiempo, STATE_LABEL, type GameDetail, type GameState } from "../lib/api";
  import Caratula from "./Caratula.svelte";

  let {
    detail,
    onclose,
    onlaunch,
    onedit,
    ondelete,
    onsetstate,
    onfavorite,
    ongraficas,
  }: {
    detail: GameDetail;
    onclose: () => void;
    onlaunch: (id: number, executableId?: number) => void;
    onedit: (detail: GameDetail) => void;
    ondelete: (detail: GameDetail) => void;
    onsetstate: (detail: GameDetail, state: GameState) => void;
    onfavorite: (detail: GameDetail, favorito: boolean) => void;
    /** Abre las gráficas de TimeTrack de este juego (guión T.0_2 §3). */
    ongraficas: (id: number) => void;
  } = $props();

  // Un juego de tienda no necesita `exe_path`: se lanza por su cliente (`steam://`).
  let lanzable = $derived(!!detail.exe_path || !!detail.store_id);
  let puedeLanzar = $derived(lanzable && detail.state === "installed");
  let jugadores = $derived(
    detail.players_min || detail.players_max
      ? detail.players_min === detail.players_max
        ? `${detail.players_max}`
        : `${detail.players_min ?? "?"}–${detail.players_max ?? "?"}`
      : null,
  );

  /** `launched_at` llega en UTC (`datetime('now')` de SQLite). */
  function fecha(s: string | null): string {
    if (!s) return "—";
    const d = new Date(s.replace(" ", "T") + "Z");
    return isNaN(d.getTime()) ? s : d.toLocaleString();
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && onclose()} />

<!-- Cerrar al pulsar fuera. El equivalente de teclado es Escape (svelte:window). -->
<div class="overlay" role="presentation" use:fondoModal={onclose}>
  <div
    class="detail"
    role="dialog"
    aria-modal="true"
    aria-label={detail.title}
    tabindex="-1"
    use:dialogo
  >
    <button class="close" onclick={onclose}>✕</button>
    <div class="detail-left">
      <div class="detail-cover">
        <!-- La propia manda sobre la automática. Desde `D.0_4` viajan **separadas**, y aquí se
             quedó leyendo solo la primera: los juegos sin carátula propia —casi todos— dejaron
             de enseñar arte en el detalle, aunque sí se veía en la cuadrícula. -->
        <Caratula
          titulo={detail.title}
          cover={detail.cover_path || detail.art_path}
          storeId={detail.store_id}
        />
      </div>
      <!-- svelte-ignore a11y_autofocus -->
      <button
        class="btn primary big"
        autofocus
        onclick={() => onlaunch(detail.id)}
        disabled={!puedeLanzar}
      >
        ▶ LANZAR
      </button>
      {#if !lanzable}
        <p class="err small">Sin ejecutable asignado: usa <b>Editar</b> para indicar la ruta.</p>
      {:else if detail.state !== "installed"}
        <p class="muted small">No se puede lanzar: está en «{STATE_LABEL[detail.state]}».</p>
      {/if}
      {#each detail.executables as ex (ex.id)}
        <button class="btn" onclick={() => onlaunch(detail.id, ex.id)} disabled={detail.state !== "installed"}>
          ▶ {ex.label}
        </button>
      {/each}
      <!-- Excluir y Eliminar viven en el desplegable: son irreversibles (o casi) y no deben
           estar al mismo nivel que Lanzar o Editar (guión A.0_3, A11). -->
      <div class="detail-actions">
        <button class="btn" onclick={() => onedit(detail)}>✎ Editar</button>
        <button
          class="btn fav-btn"
          class:primary={detail.favorite}
          title={detail.favorite ? "Quitar de favoritos" : "Añadir a favoritos"}
          onclick={() => onfavorite(detail, !detail.favorite)}
        >
          {detail.favorite ? "★" : "☆"}
        </button>
        <MenuAcciones>
          {#snippet children(cerrar)}
            {#if detail.state === "excluded"}
              <button
                class="menu-item"
                onclick={() => {
                  cerrar();
                  onsetstate(detail, "installed");
                }}>↩ Restaurar a la biblioteca</button
              >
            {:else}
              <button
                class="menu-item"
                title="No volverá a aparecer al escanear; se puede deshacer desde «Excluidos»"
                onclick={() => {
                  cerrar();
                  onsetstate(detail, "excluded");
                }}>🚫 Excluir del escaneo</button
              >
            {/if}
            <!-- Marcar/desmarcar «No instalado» a mano: el escaneo lo hace solo, pero hay que
                 poder deshacerlo si se marcó por error (guión L.0_2 §1). -->
            {#if detail.state === "uninstalled"}
              <button
                class="menu-item"
                title="Volver a darlo por instalado"
                onclick={() => {
                  cerrar();
                  onsetstate(detail, "installed");
                }}>↩ Quitar de «No instalados»</button
              >
            {:else if detail.state !== "excluded"}
              <button
                class="menu-item"
                title="Marcarlo como que no lo tienes en disco"
                onclick={() => {
                  cerrar();
                  onsetstate(detail, "uninstalled");
                }}>📦 Marcar como no instalado</button
              >
            {/if}
            <div class="menu-sep"></div>
            <button
              class="menu-item danger"
              onclick={async () => {
                cerrar();
                const ok = await confirmar({
                  titulo: `¿Eliminar "${detail.title}" de la biblioteca?`,
                  mensaje: "Se pierden sus ediciones y categorías.",
                  aceptar: "Eliminar",
                  peligro: true,
                });
                if (ok) ondelete(detail);
              }}>🗑 Eliminar</button
            >
          {/snippet}
        </MenuAcciones>
      </div>
    </div>
    <div class="detail-right">
      <h2>{detail.title}</h2>
      {#if detail.state === "uninstalled"}
        <p class="warn-banner">
          <b>No instalado:</b> no está en disco ahora mismo. Se conserva todo (categorías,
          ejecutables, argumentos y lanzamientos); si lo instalas, el siguiente escaneo lo
          recupera solo.
        </p>
      {:else if detail.state === "excluded"}
        <p class="warn-banner">
          <b>Excluido:</b> el escáner no volverá a añadirlo. Usa <b>Restaurar</b> para deshacerlo.
        </p>
      {/if}
      <div class="meta"><b>PLATFORM</b><span>{detail.platform}</span></div>
      {#if jugadores}
        <div class="meta"><b>Jugadores</b><span>{jugadores}</span></div>
      {/if}
      {#if detail.store !== "steam" || detail.exe_path}
        <div class="meta">
          <b>Ejecutable</b>
          <span>
            {detail.exe_path ?? "—"}
            {#if detail.exe_path && !detail.exe_exists}
              <em class="err small"> (no existe ahora mismo)</em>
            {/if}
          </span>
        </div>
      {/if}
      {#if detail.exe_args}
        <div class="meta"><b>Argumentos</b><span>{detail.exe_args}</span></div>
      {/if}
      {#if detail.store === "steam"}
        <div class="meta">
          <b>Steam</b>
          <span>
            appid {detail.store_id}
            {#if detail.steam_account}
              <em class="muted">· requiere la cuenta {detail.steam_account}</em>
            {/if}
          </span>
        </div>
      {/if}
      <div class="meta"><b>Instalación</b><span>{detail.install_dir ?? "—"}</span></div>
      <div class="meta"><b>Categorías</b><span>{detail.categories.join(", ") || "—"}</span></div>
      <div class="meta">
        <b>Procedimiento</b>
        <span>
          {#if detail.store === "steam"}
            Vía Steam: <code>steam://rungameid/{detail.store_id}</code>
            <em class="muted">
              ·
              {#if detail.launch_mode === "game"}
                al lanzar abre el juego
              {:else if detail.launch_mode === "client"}
                al lanzar abre su ficha en Steam
              {:else}
                pregunta al lanzar
              {/if}
            </em>
          {:else if detail.client_path}
            Launcher: {detail.client_path}
            {#if detail.client_args}<em class="muted"> {detail.client_args}</em>{/if}
            <em class="muted">
              ·
              {#if detail.launch_mode === "game"}
                al lanzar abre solo el juego
              {:else if detail.launch_mode === "client"}
                al lanzar abre solo el launcher
              {:else}
                pregunta al lanzar
              {/if}
            </em>
          {:else}
            Ejecutable directo
          {/if}
        </span>
      </div>
      <div class="meta">
        <b>Lanzamientos</b>
        <span>
          {detail.launch_count}
          {#if detail.last_launched}<em class="muted">· último: {fecha(detail.last_launched)}</em
            >{/if}
        </span>
      </div>
      <div class="meta">
        <b>Tiempo de juego</b>
        <span>
          {fmtTiempo(detail.playtime_secs)}
          {#if detail.playtime_sync}
            <em class="muted">· según TimeTrack, {fecha(detail.playtime_sync)}</em>
          {:else}
            <em class="muted">· sin sincronizar con TimeTrack</em>
          {/if}
          <!-- Las gráficas se piden **aquí**, en la línea que da el dato: es donde surge la
               pregunta de «¿y cómo se reparten esas horas?» (guión T.0_2 §3). -->
          {#if detail.tt_app_id != null}
            <button class="btn graficas" onclick={() => ongraficas(detail.id)}>
              📈 Gráficas
            </button>
          {/if}
        </span>
      </div>
    </div>
  </div>
</div>
