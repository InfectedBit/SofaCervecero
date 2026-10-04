<script lang="ts">
  import { untrack } from "svelte";
  import type { Category, LibraryFilter } from "../lib/api";
  import {
    JUGADORES,
    ORDEN,
    ORIGEN_LABEL,
    TIEMPO,
    buscarOrden,
    tiempoActivo,
    tiempoFiltro,
  } from "../lib/filtros";
  import { ejesDisponibles, INDICE, type ModoSeccion } from "../lib/secciones";
  import { prefs, setPrefs } from "../lib/prefs.svelte";
  import { pulsacionFuera } from "../lib/focus";
  import TareasBarra from "./TareasBarra.svelte";

  let {
    filter,
    platforms,
    origins,
    categories,
    scanning,
    onfilter,
    onclear,
    onscan,
    onadd,
    onsources,
    oncategories,
    onsettings,
    onsofa,
  }: {
    filter: LibraryFilter;
    platforms: string[];
    origins: string[];
    categories: Category[];
    scanning: boolean;
    onfilter: (patch: Partial<LibraryFilter>) => void;
    onclear: () => void;
    onscan: () => void;
    onadd: () => void;
    onsources: () => void;
    oncategories: () => void;
    onsettings: () => void;
    /** Abre la pregunta de entrada al Modo Sofá (guión E.0_4). */
    onsofa: () => void;
  } = $props();

  // Mientras se escribe manda el input; pero si el filtro de texto se limpia **desde fuera**
  // (el chip ✕ de la barra de filtros activos, o «Limpiar todo»), la caja tiene que vaciarse
  // con él. Si no, se queda enseñando un texto que ya no filtra nada.
  // svelte-ignore state_referenced_locally
  let value = $state(filter.query ?? "");
  $effect(() => {
    const q = filter.query ?? "";
    // `untrack` para depender solo de `filter.query`: leer `value` aquí lo convertiría en
    // dependencia y el efecto se reejecutaría con cada tecla, sin necesidad.
    untrack(() => {
      if (q !== value) value = q;
    });
  });
  let abierto = $state(false);

  let criterio = $derived(buscarOrden(filter.sort));
  /** Dirección efectiva: si no se ha tocado, la natural del criterio. */
  let desc = $derived(filter.desc ?? criterio.desc);

  /**
   * Al cambiar de criterio se adopta **su** dirección natural: nadie pide «tiempo jugado»
   * para ver primero el que menos ha jugado. Si luego quiere el inverso, está el botón.
   */
  function cambiarCriterio(id: string) {
    const o = buscarOrden(id);
    setPrefs({ orden: id, ordenDesc: o.desc });
    onfilter({ sort: id, desc: o.desc });
  }
  function invertir() {
    setPrefs({ ordenDesc: !desc });
    onfilter({ desc: !desc });
  }

  /** Ejes que se pueden elegir: los fijos más una entrada por categoría raíz. */
  let ejes = $derived(ejesDisponibles(categories));
  let ayudaEje = $derived(ejes.find((e) => e.id === prefs.seccion)?.ayuda ?? "");
  /**
   * Los ejes que se pueden anidar dentro de otro. El **índice** queda fuera: ya ocupa dos
   * niveles por sí solo (eje → sus categorías), así que meterlo dentro de otro no significa
   * nada.
   */
  let ejesAnidables = $derived(ejes.filter((e) => e.id !== INDICE));

  /**
   * Atajos de la barra: los dos que más se usan salen fuera del panel, sin dejar de estar
   * dentro. El de categorías **solo toca el primer eje**: si hay más anidados se respetan,
   * porque tirar la configuración del panel por pulsar un atajo sería una sorpresa fea.
   */
  let indiceActivo = $derived(prefs.seccion === INDICE);
  let favoritosActivo = $derived(filter.favorite === true);

  function alternarIndice() {
    setPrefs({ seccion: indiceActivo ? "none" : INDICE });
  }
  function alternarFavoritos() {
    onfilter({ favorite: favoritosActivo ? null : true });
  }

  /** Solo los filtros de contenido; el orden y las secciones no «filtran» nada. */
  let activos = $derived(
    [filter.origin, filter.platform, filter.players, filter.played, filter.min_hours, filter.max_hours].filter(
      (v) => v !== null && v !== undefined && v !== "",
    ).length,
  );
</script>


<header class="topbar">
  <div class="brand">🍺 SofaCervecero</div>
  <input
    class="search"
    placeholder="Buscar juegos…"
    bind:value
    oninput={() => onfilter({ query: value || null })}
  />

  <!-- Se cierra al pulsar fuera, pero solo si la pulsación **empezó** fuera: así seleccionar
       texto dentro y soltar en el grid no lo cierra (guión A.0_4 §3). -->
  <div class="filter-wrap" use:pulsacionFuera={() => (abierto = false)}>
    <button
      class="btn"
      class:primary={activos > 0}
      aria-expanded={abierto}
      onclick={() => (abierto = !abierto)}
      title="Orden, secciones y filtros de la cuadrícula"
    >
      ▦ Vista{activos > 0 ? ` · ${activos} filtro${activos === 1 ? "" : "s"}` : ""}
    </button>

    {#if abierto}
      <div class="filter-panel vista">
        <!-- ── Orden ─────────────────────────────────────────────────────── -->
        <div class="vista-bloque">
          <div class="vista-titulo">Orden</div>
          <label>
            Ordenar por
            <select
              value={filter.sort ?? "title"}
              onchange={(e) => cambiarCriterio(e.currentTarget.value)}
            >
              {#each ORDEN as o}<option value={o.id}>{o.texto}</option>{/each}
            </select>
          </label>
          <button class="btn dir" onclick={invertir} title="Invertir el orden">
            <span class="dir-flecha">{desc ? "▼" : "▲"}</span>
            {desc ? criterio.descTexto : criterio.asc}
          </button>
        </div>

        <!-- ── Secciones ─────────────────────────────────────────────────── -->
        <div class="vista-bloque">
          <div class="vista-titulo">Secciones</div>
          <label>
            Partir la cuadrícula por
            <select
              value={prefs.seccion}
              onchange={(e) => setPrefs({ seccion: e.currentTarget.value as ModoSeccion })}
            >
              {#each ejes as eje}<option value={eje.id}>{eje.texto}</option>{/each}
            </select>
            <small class="hint">{ayudaEje}</small>
          </label>

          <!-- Ejes anidados: cada uno añade un nivel. Es la cadena
               `[Steam] → [Couch Co-Op] → [LEGO] → juego`, donde cada juego sale una vez.
               Dejar uno en «Sin secciones» es el «mostrar Todos» de ese eje. -->
          {#if prefs.seccion !== "none"}
            <label>
              Y dentro, por
              <select
                value={prefs.seccion2}
                onchange={(e) =>
                  setPrefs({
                    seccion2: e.currentTarget.value as ModoSeccion,
                    // Sin segundo nivel no puede haber tercero.
                    ...(e.currentTarget.value === "none" ? { seccion3: "none" } : {}),
                  })}
              >
                {#each ejesAnidables.filter((e) => e.id !== prefs.seccion) as eje}
                  <option value={eje.id}>{eje.texto}</option>
                {/each}
              </select>
            </label>
          {/if}
          {#if prefs.seccion !== "none" && prefs.seccion2 !== "none"}
            <label>
              Y luego, por
              <select
                value={prefs.seccion3}
                onchange={(e) => setPrefs({ seccion3: e.currentTarget.value as ModoSeccion })}
              >
                {#each ejesAnidables.filter((e) => e.id !== prefs.seccion && e.id !== prefs.seccion2) as eje}
                  <option value={eje.id}>{eje.texto}</option>
                {/each}
              </select>
            </label>
          {/if}
          <small class="hint">
            Con un eje concreto, un juego en dos de sus categorías va a su propia sección
            («Couch Co-Op + Online Co-Op») y <b>nunca se repite</b>. Con
            <b>Categorías (todas)</b> sí sale una vez por cada eje en el que esté: ese modo es
            para navegar el catálogo entero.
          </small>
        </div>

        <!-- ── Filtrar ───────────────────────────────────────────────────── -->
        <div class="vista-bloque">
          <div class="vista-titulo">Filtrar</div>
          <label class="check">
            <input
              type="checkbox"
              checked={favoritosActivo}
              onchange={(e) => onfilter({ favorite: e.currentTarget.checked ? true : null })}
            />
            Solo favoritos
          </label>
          <label>
            Jugadores (al menos)
            <select
              value={filter.players ?? ""}
              onchange={(e) =>
                onfilter({ players: e.currentTarget.value ? +e.currentTarget.value : null })}
            >
              <option value="">Cualquiera</option>
              {#each JUGADORES as j}<option value={j.valor}>{j.texto}</option>{/each}
            </select>
          </label>

          <!-- Jugado/sin jugar y las horas, en **un** desplegable: son la misma dimensión, y
               separarlos permitiría pedir «sin jugar» y «más de 20 h» a la vez (guión T.0_2 §2). -->
          <label>
            Tiempo jugado
            <select
              value={tiempoActivo({
                played: filter.played ?? null,
                min_hours: filter.min_hours ?? null,
                max_hours: filter.max_hours ?? null,
              })?.id ?? ""}
              onchange={(e) => onfilter(tiempoFiltro(e.currentTarget.value))}
            >
              <option value="">Indiferente</option>
              {#each TIEMPO as t}<option value={t.id}>{t.texto}</option>{/each}
            </select>
            <small class="hint">Las horas son las que lleva registradas <b>TimeTrack</b>.</small>
          </label>

          <label>
            Origen
            <select
              value={filter.origin ?? ""}
              onchange={(e) => onfilter({ origin: e.currentTarget.value || null })}
            >
              <option value="">Todos</option>
              {#each origins as o}<option value={o}>{ORIGEN_LABEL[o] ?? o}</option>{/each}
            </select>
          </label>

          {#if platforms.length > 1}
            <label>
              Plataforma
              <select
                value={filter.platform ?? ""}
                onchange={(e) => onfilter({ platform: e.currentTarget.value || null })}
              >
                <option value="">Todas</option>
                {#each platforms as p}<option value={p}>{p}</option>{/each}
              </select>
            </label>
          {/if}
        </div>

        <div class="actions">
          <!-- Este botón **no** toca la búsqueda: `clearFilters` la conserva a propósito. Antes
               vaciaba la caja de texto igualmente, así que quedaba vacía mientras se seguía
               filtrando por el texto anterior — invisible y sin forma de quitarlo desde aquí. -->
          <button class="btn" onclick={onclear}>Limpiar filtros</button>
          <button class="btn primary" onclick={() => (abierto = false)}>Cerrar</button>
        </div>
      </div>
    {/if}
  </div>

  <!-- Atajos de un clic a lo que más se usa del panel. Siguen estando dentro de Vista. -->
  <button
    class="btn"
    class:primary={indiceActivo}
    aria-pressed={indiceActivo}
    title="Ver todos los ejes con sus categorías. Un juego sale una vez por cada eje"
    onclick={alternarIndice}>▦ Categorías</button
  >
  <button
    class="btn"
    class:primary={favoritosActivo}
    aria-pressed={favoritosActivo}
    title="Solo los marcados como favoritos"
    onclick={alternarFavoritos}>{favoritosActivo ? "★" : "☆"} Favoritos</button
  >

  <!-- Las tareas en segundo plano viven aquí: escaneo, carátulas… (guión J.0_1). -->
  <TareasBarra />
  <div class="spacer"></div>
  <!-- Extracto de Ajustes: el tamaño de card se toca constantemente y no merece abrir un
       menú cada vez (guión A.0_4 §5). En vista lista no pinta nada, así que no se enseña. -->
  {#if prefs.view === "grid"}
    <label class="card-size" title="Tamaño de las cards: {prefs.cardSize}px">
      <span aria-hidden="true">▫</span>
      <input
        type="range"
        min="110"
        max="260"
        step="10"
        aria-label="Tamaño de las cards"
        value={prefs.cardSize}
        oninput={(e) => setPrefs({ cardSize: +e.currentTarget.value })}
      />
      <span aria-hidden="true">◻</span>
    </label>
  {/if}
  <button class="btn" onclick={oncategories} title="Categorías y grupos">🗂️</button>
  <button class="btn" onclick={onsources} title="Gestionar bibliotecas y apps añadidas">
    ⛁ Fuentes
  </button>
  <button class="btn" onclick={onadd}>＋ Añadir</button>
  <button class="btn primary" onclick={onscan} disabled={scanning}>
    {scanning ? "Escaneando…" : "⟳ Escanear"}
  </button>
  <button
    class="btn"
    onclick={onsofa}
    title="Modo Sofá: el hub a pantalla completa en la pantalla que elijas"
  >🛋</button>
  <button class="btn" onclick={onsettings} title="Opciones generales">⚙</button>
</header>
