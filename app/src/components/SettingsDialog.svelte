<script lang="ts">
  import { dialogo, fondoModal } from "../lib/focus";
  import { elegirCarpeta } from "../lib/rutas";
  import { prefs, setPrefs, resetPrefs, resetMando, aplicarPrefsTema } from "../lib/prefs.svelte";
  import { confirmar } from "../lib/confirmar.svelte";
  import { abrirContextual, type OpcionContextual } from "../lib/contextual.svelte";
  import { THEMES, THEME_VARS, aplicarTema, buscarTema, escala, type Theme } from "../lib/themes";
  import { api, type ArtSummary, type MandoConfig, type Pantalla, type TimeTrackConfig, type TimeTrackSync } from "../lib/api";
  import SelectorPantalla from "./SelectorPantalla.svelte";
  import { ACCIONES, MAPA_POR_DEFECTO, comoLista, type AccionMando } from "../lib/gamepad";
  import Tecla from "./Tecla.svelte";

  let {
    onclose,
    onsynced,
    capturando,
    oncapturar,
  }: {
    onclose: () => void;
    onsynced: () => void;
    /** Acción que está esperando una pulsación de botón, o `null`. */
    capturando: AccionMando | null;
    /** `null` cancela la captura. */
    oncapturar: (a: AccionMando | null) => void;
  } = $props();

  /**
   * Ajustes por **pestañas** (guión F.0_6 §5). Con todo en una sola columna había que bajar
   * por cinco bloques para llegar al mando, y añadir una opción más lo empeoraba. Separados,
   * cada apartado puede crecer sin saturar la ventana.
   */
  const PESTANAS = [
    { id: "aspecto", texto: "Aspecto", icono: "🎨" },
    { id: "biblioteca", texto: "Biblioteca", icono: "▦" },
    { id: "caratulas", texto: "Carátulas", icono: "🖼" },
    { id: "mando", texto: "Mando", icono: "🎮" },
    { id: "sofa", texto: "Sofá", icono: "🛋" },
    { id: "timetrack", texto: "TimeTrack", icono: "⏱" },
  ] as const;
  let pestana = $state<(typeof PESTANAS)[number]["id"]>("aspecto");

  /**
   * Escanear al abrir el hub. Vive en el **core**, no en `localStorage`: cerrar el hub destruye
   * el WebView2 y Chromium escribe `localStorage` en diferido, así que desmarcar la casilla y
   * cerrar la ventana —que es justo lo que uno hace— podía perder el cambio y volver a recorrer
   * los discos en la siguiente apertura. Marcada de fábrica; ausente ⇒ sí.
   */
  let scanAlAbrir = $state(true);
  api.settingGet("hub.scan_on_start").then((v) => (scanAlAbrir = v !== "0"));

  async function alternarScanAlAbrir(activo: boolean) {
    scanAlAbrir = activo;
    await api.settingSet("hub.scan_on_start", activo ? "1" : "0");
  }

  // ── Modo Sofá (guión E.0_4, fase 1) ────────────────────────────────────────
  /**
   * De momento esta pestaña **solo enseña lo que el core ve**. Es el primer escalón del plan:
   * sin un inventario de pantallas fiable —en píxeles físicos y con nombres de verdad— no se
   * puede construir nada de lo que viene detrás.
   */
  let pantallas = $state<Pantalla[]>([]);
  let pantallaElegida = $state<string | null>(null);
  let pantallasError = $state<string | null>(null);

  /**
   * Botón que entra en Modo Sofá al **mantenerlo**. Por defecto **SELECT**.
   *
   * No se usa el botón Xbox para esto porque **mantenerlo apaga el mando** — ese se queda como
   * pulsación única. Y no va en `prefs.gamepadMap` sino en el core, porque quien lo lee es el
   * núcleo, que sondea el mando también con la ventana cerrada (guión E.0_4 §9.1).
   */
  const BOTONES_SOFA = [
    { i: 8, texto: "Select / Share" },
    { i: 9, texto: "Start / Options" },
    { i: 4, texto: "LB / L1" },
    { i: 5, texto: "RB / R1" },
    { i: 10, texto: "L3" },
    { i: 11, texto: "R3" },
  ];
  let mandoCfg = $state<MandoConfig>({
    escucha: true,
    boton: 8,
    gesto: "mantener",
    mantener_ms: 1200,
    xbox_trae: true,
    xbox_doble: false,
  });
  api.mandoConfig().then((c) => (mandoCfg = c));
  async function guardarMando(patch: Partial<MandoConfig>) {
    mandoCfg = { ...mandoCfg, ...patch };
    await api.mandoConfigSet(mandoCfg);
  }

  api
    .monitorsList()
    .then((p) => {
      pantallas = p;
      pantallaElegida = p.find((x) => x.primaria)?.id ?? p[0]?.id ?? null;
    })
    .catch((e) => (pantallasError = String(e)));

  // ── Carátulas ──────────────────────────────────────────────────────────────
  let arte = $state<ArtSummary | null>(null);
  let arteOcupado = $state(false);
  /** ¿Se acepta además el arte de TimeTrack? Desactivado por defecto (guión D.0_2 §6). */
  let arteTT = $state(false);
  api.settingGet("timetrack.arte").then((v) => (arteTT = v === "1"));

  async function alternarArteTT(activo: boolean) {
    arteTT = activo;
    await api.settingSet("timetrack.arte", activo ? "1" : "0");
  }

  /**
   * Fuentes remotas de arte (guión D.0_3). Interruptores y claves viven en el **core**, no en
   * `localStorage`: una clave de API no es «cómo se ve la sesión actual», y además el core es
   * quien las usa — la búsqueda entera pasa por Rust porque ninguna de estas APIs manda
   * cabeceras CORS, y porque el arte hay que **guardarlo en disco**.
   */
  let fuenteSteam = $state(true);
  let fuenteSgdb = $state(true);
  let fuenteRawg = $state(false);
  let claveSgdb = $state("");
  let claveRawg = $state("");
  let verSgdb = $state(false);
  let verRawg = $state(false);
  let importando = $state(false);
  let importeMsg = $state<string | null>(null);

  async function cargarFuentes() {
    const [s, g, r, ks, kr] = await Promise.all([
      api.settingGet("art.fuente.steam"),
      api.settingGet("art.fuente.sgdb"),
      api.settingGet("art.fuente.rawg"),
      api.settingGet("art.sgdb_key"),
      api.settingGet("art.rawg_key"),
    ]);
    fuenteSteam = s !== "0";
    fuenteSgdb = g !== "0";
    fuenteRawg = r === "1";
    claveSgdb = ks ?? "";
    claveRawg = kr ?? "";
  }
  cargarFuentes();

  async function alternarFuente(cual: "steam" | "sgdb" | "rawg", activo: boolean) {
    if (cual === "steam") fuenteSteam = activo;
    else if (cual === "sgdb") fuenteSgdb = activo;
    else fuenteRawg = activo;
    await api.settingSet(`art.fuente.${cual}`, activo ? "1" : "0");
  }

  async function guardarClave(cual: "sgdb" | "rawg", valor: string) {
    const v = valor.trim();
    if (cual === "sgdb") claveSgdb = v;
    else claveRawg = v;
    await api.settingSet(`art.${cual}_key`, v);
  }

  /**
   * Trae las claves que TimeTrack ya tenga puestas. **Se piden explícitamente**: leer la base
   * de datos de otra app por tu cuenta y copiar sus credenciales no se hace en silencio, igual
   * que la recolocación de categorías se propone en vez de aplicarse (ADR-005 §5).
   */
  async function importarClaves() {
    importando = true;
    importeMsg = null;
    try {
      const k = await api.timetrackArtKeys();
      const puestas: string[] = [];
      if (k.sgdb) {
        await guardarClave("sgdb", k.sgdb);
        puestas.push("SteamGridDB");
      }
      if (k.rawg) {
        await guardarClave("rawg", k.rawg);
        puestas.push("RAWG");
      }
      importeMsg = puestas.length
        ? `Importadas: ${puestas.join(" y ")}.`
        : "TimeTrack no tiene ninguna clave configurada.";
    } catch (e) {
      importeMsg = String(e);
    } finally {
      importando = false;
    }
  }

  /**
   * «Rehacer» pregunta antes: con una biblioteca grande son minutos de red y cambia de golpe el
   * aspecto de toda la cuadrícula. Lo que **no** cambia son las carátulas puestas a mano, y el
   * aviso lo dice — que es justo la duda que da miedo al pulsarlo.
   */
  async function rehacerArte() {
    const ok = await confirmar({
      titulo: "¿Rehacer todas las carátulas?",
      mensaje:
        "Se olvidan las automáticas y se recorre la cascada entera: carpeta del juego, caché de " +
        "Steam, fuentes de internet y, en último lugar, el icono del ejecutable. Las que hayas " +
        "puesto a mano no se tocan. Con muchos juegos puede tardar varios minutos.",
      aceptar: "Rehacer",
    });
    if (ok) await buscarArte(true);
  }

  async function buscarArte(rehacer: boolean) {
    arteOcupado = true;
    try {
      arte = await api.artFetch(rehacer);
      onsynced();
    } catch (e) {
      ttError = String(e);
    } finally {
      arteOcupado = false;
    }
  }

  /** Otra acción ya usa esa **misma** asignación: se avisa, pero no se impide. */
  function duplicado(a: AccionMando): string | null {
    const mia = comoLista(prefs.gamepadMap[a]).join(",");
    const otra = ACCIONES.find(
      (x) => x.id !== a && comoLista(prefs.gamepadMap[x.id]).join(",") === mia,
    );
    return otra ? otra.etiqueta : null;
  }

  /** Devuelve **una sola** acción a su asignación de fábrica. */
  function restablecerAccion(a: AccionMando) {
    setPrefs({ gamepadMap: { ...prefs.gamepadMap, [a]: MAPA_POR_DEFECTO[a] } });
  }
  const esDefecto = (a: AccionMando) =>
    comoLista(prefs.gamepadMap[a]).join(",") === comoLista(MAPA_POR_DEFECTO[a]).join(",");

  // ── TimeTrack ──────────────────────────────────────────────────────────────
  let tt = $state<TimeTrackConfig>({ base: "", dir: null });
  let ttSync = $state<TimeTrackSync | null>(null);
  let ttOcupado = $state(false);
  let ttError = $state<string | null>(null);

  async function cargarTT() {
    try {
      tt = await api.timetrackConfig();
    } catch (e) {
      ttError = String(e);
    }
  }

  async function elegirDirTT() {
    const sel = await elegirCarpeta(tt.dir);
    if (sel) {
      await api.timetrackSetConfig({ dir: sel });
      await cargarTT();
    }
  }

  async function sincronizarTT() {
    ttOcupado = true;
    ttError = null;
    try {
      await api.timetrackSetConfig({ base: tt.base });
      ttSync = await api.timetrackSync();
      onsynced();
    } catch (e) {
      ttError = String(e);
    } finally {
      ttOcupado = false;
    }
  }

  cargarTT();

  // ── Temas (guión H.0_3) ────────────────────────────────────────────────────

  /**
   * Qué está haciendo el editor. `null` = cerrado.
   *
   * Los dos modos usan el mismo formulario a propósito: lo único que cambia es si al guardar se
   * **sustituye** el tema de ese `id` o se **añade** uno nuevo. Antes solo existía el segundo, así
   * que corregir un color de un tema propio dejaba una copia más en la lista.
   */
  let editando = $state<{ modo: "crear" | "editar"; id: string } | null>(null);
  let borrador = $state<Theme>({ ...buscarTema(prefs.theme, prefs.customThemes) });
  let nombreNuevo = $state("");
  let nombreEl = $state<HTMLInputElement | null>(null);

  let todosLosTemas = $derived([...THEMES, ...prefs.customThemes]);
  /** Los de fábrica no se tocan: se duplican. `ct_` los marca desde que existen los propios. */
  const esPropio = (id: string) => id.startsWith("ct_");

  /**
   * **Vista previa en vivo**: mientras el editor está abierto, el borrador se aplica a toda la
   * app. Elegir nueve colores a ciegas y descubrir el resultado al guardar no es editar un tema,
   * es adivinarlo.
   *
   * Se aplica sin el acento personalizado (`null`) porque lo que se está viendo es **el tema**;
   * el acento vuelve solo al cerrar, que es cuando manda otra vez `prefs`.
   */
  $effect(() => {
    if (editando) aplicarTema(borrador, null);
  });

  /** Duplica cualquier tema —también los de fábrica— en uno propio nuevo. */
  function duplicarTema(t: Theme) {
    borrador = { ...t };
    nombreNuevo = `${t.name} (copia)`;
    editando = { modo: "crear", id: t.id };
  }

  /** Edita un tema propio **en su sitio**, sin dejar una copia. */
  function editarTema(t: Theme, soloNombre = false) {
    borrador = { ...t };
    nombreNuevo = t.name;
    editando = { modo: "editar", id: t.id };
    if (soloNombre) {
      // «Renombrar» aterriza en el mismo editor, pero con el nombre listo para escribir encima.
      requestAnimationFrame(() => nombreEl?.select());
    }
  }

  /**
   * Guarda y **activa** el tema. Activarlo no es un extra: con la vista previa llevas un rato
   * mirándolo a pantalla completa, así que salir de él al guardar sería lo raro.
   */
  function guardarTema() {
    const name = nombreNuevo.trim() || "Tema propio";
    if (editando?.modo === "editar") {
      const id = editando.id;
      setPrefs({
        customThemes: prefs.customThemes.map((t) => (t.id === id ? { ...borrador, id, name } : t)),
        theme: id,
        accent: null,
      });
    } else {
      const id = `ct_${Date.now()}`;
      setPrefs({
        customThemes: [...prefs.customThemes, { ...borrador, id, name }],
        theme: id,
        accent: null,
      });
    }
    editando = null;
  }

  /** Cierra sin guardar. Hay que deshacer la vista previa a mano: la hizo el editor. */
  function cancelarEdicion() {
    editando = null;
    aplicarPrefsTema();
  }

  /**
   * Cerrar la ventana con el editor abierto equivale a cancelar. Sin esto, la vista previa se
   * quedaba pegada a la app sin tema guardado detrás que la respaldara.
   */
  function cerrar() {
    if (editando) cancelarEdicion();
    onclose();
  }

  async function borrarTemaPropio(t: Theme) {
    const ok = await confirmar({
      titulo: `¿Eliminar el tema «${t.name}»?`,
      mensaje:
        prefs.theme === t.id
          ? "Está activo ahora mismo: la app volverá al tema de fábrica."
          : undefined,
      aceptar: "Eliminar",
      peligro: true,
    });
    if (!ok) return;
    setPrefs({
      customThemes: prefs.customThemes.filter((x) => x.id !== t.id),
      theme: prefs.theme === t.id ? "sofa" : prefs.theme,
    });
  }

  /**
   * Menú del clic derecho sobre un tema. Es donde viven las acciones que no son «aplicar»:
   * ponerlas todas como botones dentro de cada muestra la volvería ilegible, y son de uso raro.
   */
  function menuTema(e: MouseEvent, t: Theme) {
    const opciones: OpcionContextual[] = [];
    if (prefs.theme !== t.id) {
      opciones.push({
        etiqueta: "Aplicar",
        icono: "✔",
        accion: () => setPrefs({ theme: t.id, accent: null }),
      });
    }
    opciones.push({ etiqueta: "Duplicar y editar…", icono: "⧉", accion: () => duplicarTema(t) });
    if (esPropio(t.id)) {
      opciones.push({ etiqueta: "Editar…", icono: "✎", separar: true, accion: () => editarTema(t) });
      opciones.push({ etiqueta: "Renombrar…", icono: "🏷", accion: () => editarTema(t, true) });
      opciones.push({
        etiqueta: "Eliminar…",
        icono: "🗑",
        separar: true,
        peligro: true,
        accion: () => borrarTemaPropio(t),
      });
    } else {
      // Decirlo es mejor que ofrecer un «Editar» que no haría lo que promete.
      opciones.push({ etiqueta: "Los temas de fábrica no se editan", cabecera: true, separar: true });
    }
    abrirContextual(e, t.name, opciones);
  }

  /** Muestra de colores para el swatch de cada tema. */
  function muestras(t: Theme): string[] {
    const e = escala(t);
    return [e["--bg"], e["--panel2"], e["--accent"], e["--accent2"]];
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key !== "Escape") return;
    // Durante una captura, Escape **cancela la captura** y deja la asignación como estaba;
    // no cierra Ajustes. Cerrar la ventana a media captura sería perder el sitio.
    if (capturando) {
      e.stopPropagation();
      oncapturar(null);
      return;
    }
    cerrar();
  }}
/>

<div class="overlay" role="presentation" use:fondoModal={cerrar}>
  <div
    class="dialog edit"
    role="dialog"
    aria-modal="true"
    aria-label="Opciones"
    tabindex="-1"
    use:dialogo
  >
    <button class="close" onclick={cerrar}>✕</button>
    <h3>⚙ Opciones</h3>

    <div class="tabs" role="tablist">
      {#each PESTANAS as p}
        <button
          class="tab"
          class:active={pestana === p.id}
          role="tab"
          aria-selected={pestana === p.id}
          onclick={() => (pestana = p.id)}
        >
          <span aria-hidden="true">{p.icono}</span>{p.texto}
        </button>
      {/each}
    </div>

    <!-- ── Aspecto ─────────────────────────────────────────────────────────── -->
    {#if pestana === "aspecto"}
    <div class="section">
      <div class="swatches">
        {#each todosLosTemas as t (t.id)}
          <button
            class="swatch"
            class:active={prefs.theme === t.id}
            class:propio={esPropio(t.id)}
            title={esPropio(t.id) ? `${t.name} — clic derecho para editar, duplicar o eliminar` : t.name}
            onclick={() => setPrefs({ theme: t.id, accent: null })}
            oncontextmenu={(e) => menuTema(e, t)}
            onkeydown={(e) => {
              if (e.key !== "ContextMenu" && !(e.shiftKey && e.key === "F10")) return;
              e.preventDefault();
              const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
              menuTema(new MouseEvent("contextmenu", { clientX: r.left + 20, clientY: r.bottom }), t);
            }}
          >
            <span class="swatch-colors">
              {#each muestras(t) as c}<i style="background:{c}"></i>{/each}
            </span>
            <span class="swatch-name">{t.name}</span>
            {#if esPropio(t.id)}
              <!-- Atajo al menú, no un segundo menú: con el mando y con el teclado hay que poder
                   llegar a lo mismo sin depender del clic derecho. -->
              <span
                class="swatch-mas"
                title="Opciones del tema"
                role="button"
                tabindex="-1"
                onclick={(e) => {
                  e.stopPropagation();
                  menuTema(e, t);
                }}
                onkeydown={() => {}}>⋯</span
              >
            {/if}
          </button>
        {/each}
      </div>

      <div class="grid2">
        <label>
          Color de acento
          <div class="path-row">
            <input
              type="color"
              value={prefs.accent ?? buscarTema(prefs.theme, prefs.customThemes).accent}
              oninput={(e) => setPrefs({ accent: e.currentTarget.value })}
            />
            <button class="btn" onclick={() => setPrefs({ accent: null })}>Restablecer</button>
          </div>
        </label>
        <label>
          Tema propio
          {#if esPropio(prefs.theme)}
            <button class="btn" onclick={() => editarTema(buscarTema(prefs.theme, prefs.customThemes))}>
              ✎ Editar «{buscarTema(prefs.theme, prefs.customThemes).name}»
            </button>
          {:else}
            <button class="btn" onclick={() => duplicarTema(buscarTema(prefs.theme, prefs.customThemes))}>
              ⧉ Duplicar el actual y editarlo
            </button>
          {/if}
          <small class="hint">Clic derecho sobre cualquier tema para duplicarlo, editarlo o borrarlo.</small>
        </label>
      </div>

      {#if editando}
        <div class="theme-editor">
          <div class="theme-editor-cab">
            {editando.modo === "editar" ? "Editando este tema" : "Nuevo tema"}
            <span class="hint">Los cambios se ven en la app según los tocas.</span>
          </div>
          <label>Nombre<input bind:this={nombreEl} bind:value={nombreNuevo} /></label>
          <div class="theme-vars">
            {#each THEME_VARS as [clave, etiqueta]}
              <label class="theme-var">
                <input
                  type="color"
                  value={borrador[clave]}
                  oninput={(e) => (borrador = { ...borrador, [clave]: e.currentTarget.value })}
                />
                <span>{etiqueta}</span>
              </label>
            {/each}
          </div>
          <div class="actions">
            <button class="btn" onclick={cancelarEdicion}>Cancelar</button>
            <button class="btn primary" onclick={guardarTema}>
              {editando.modo === "editar" ? "Guardar cambios" : "Crear tema"}
            </button>
          </div>
        </div>
      {/if}
    </div>
    {/if}

    <!-- ── Biblioteca ──────────────────────────────────────────────────────── -->
    {#if pestana === "biblioteca"}
    <div class="section">
      <div class="grid2">
        <label>
          Formato
          <select
            value={prefs.view}
            onchange={(e) => setPrefs({ view: e.currentTarget.value as "grid" | "list" })}
          >
            <option value="grid">Cuadrícula</option>
            <option value="list">Lista</option>
          </select>
        </label>
        <label>
          Forma de la card
          <select
            value={prefs.cardShape}
            onchange={(e) => setPrefs({ cardShape: e.currentTarget.value as "rect" | "square" })}
          >
            <option value="rect">Rectangular (carátula)</option>
            <option value="square">Cuadrada (caben más)</option>
          </select>
        </label>
      </div>
      <label>
        «Añadidos hace poco»: últimos <b>{prefs.recientesDias}</b> días
        <input
          type="range"
          min="1"
          max="90"
          step="1"
          value={prefs.recientesDias}
          oninput={(e) => {
            setPrefs({ recientesDias: +e.currentTarget.value });
            onsynced();
          }}
        />
        <small class="hint">
          Los juegos que ya estaban antes de que la app guardara esta fecha no cuentan: de esos
          no consta cuándo entraron.
        </small>
      </label>
      <label>
        Tamaño de las cards: <b>{prefs.cardSize}px</b>
        <input
          type="range"
          min="110"
          max="260"
          step="10"
          value={prefs.cardSize}
          oninput={(e) => setPrefs({ cardSize: +e.currentTarget.value })}
        />
      </label>
      <label class="check">
        <input
          type="checkbox"
          checked={scanAlAbrir}
          onchange={(e) => alternarScanAlAbrir(e.currentTarget.checked)}
        />
        Escanear las fuentes al abrir el hub
      </label>
      <small class="hint">
        Desmarcado, la biblioteca se queda como está hasta que pulses <b>⟳ Escanear</b>. Evita
        recorrer los discos cada vez que se abre el hub.
      </small>
    </div>

    <!-- ── Carátulas ───────────────────────────────────────────────────────── -->
    {/if}
    {#if pestana === "caratulas"}
    <div class="section">
      <p class="muted small">
        La cascada va <b>de lo que ya está en disco a lo que hay que buscar</b>: imagen en la
        carpeta del juego → caché de Steam → <b>fuentes de internet</b> → icono del ejecutable.
        El icono va el último a propósito: acierta siempre, así que puesto antes tapaba a todo
        lo demás.
      </p>

      <!-- Dos acciones **distintas**, cada una con su explicación. Antes eran dos botones
           pegados en una fila y el segundo, sin `primary`, se perdía con los temas oscuros. -->
      <div class="accion-arte">
        <button class="btn primary" onclick={() => buscarArte(false)} disabled={arteOcupado}>
          {arteOcupado ? "Buscando…" : "🔍 Buscar las que faltan"}
        </button>
        <small class="hint">Solo mira los juegos que hoy no tienen ninguna carátula.</small>
      </div>

      <div class="accion-arte">
        <button class="btn danger" onclick={rehacerArte} disabled={arteOcupado}>
          {arteOcupado ? "Buscando…" : "↻ Rehacer todas"}
        </button>
        <small class="hint">
          Olvida <b>todas las carátulas automáticas</b> y vuelve a recorrer la cascada entera.
          Es lo que hace falta para cambiar las que hoy enseñan el icono del ejecutable.
          <b>Nunca toca las que has puesto tú.</b>
        </small>
      </div>

      {#if arte}
        <p class="muted small">
          {arte.pendientes} procesadas ·
          <b>{arte.de_carpeta}</b> de la carpeta · <b>{arte.de_steam}</b> de la caché de Steam ·
          <b>{arte.de_steam_web}</b> de la tienda de Steam · <b>{arte.de_sgdb}</b> de SteamGridDB ·
          <b>{arte.de_rawg}</b> de RAWG · <b>{arte.de_icono}</b> del icono del exe
          {#if arte.sin_resolver > 0}· {arte.sin_resolver} sin resolver{/if}
          {#if arte.cancelada}· <b>cancelada a medias</b>{/if}
        </p>
      {/if}

      <!-- Una tarjeta por fuente, como en TimeTrack: qué cubre, interruptor y, si la pide,
           su clave. Lo que no se ve no se entiende, y «buscar carátulas» a secas no decía
           de dónde salían. -->
      <div class="section-title">Fuentes de internet</div>

      <div class="fuente-arte">
        <label class="check">
          <input
            type="checkbox"
            checked={fuenteSteam}
            onchange={(e) => alternarFuente("steam", e.currentTarget.checked)}
          />
          <b>Tienda de Steam</b> — buscando por nombre
        </label>
        <small class="hint">
          🎯 Catálogo de Steam · 🖼 Póster vertical 600×900 · <b>sin clave</b><br />
          Es la que más rinde: la mayoría de los juegos de carpeta suelta están en Steam aunque
          no los hayas comprado ahí. Solo se acepta el juego que <b>se llama igual</b>, para no
          colar la carátula de una secuela.
        </small>
      </div>

      <div class="fuente-arte">
        <label class="check">
          <input
            type="checkbox"
            checked={fuenteSgdb}
            onchange={(e) => alternarFuente("sgdb", e.currentTarget.checked)}
          />
          <b>SteamGridDB</b>
        </label>
        <small class="hint">
          🎯 Todos: no-Steam, emulados, mods y recopilaciones · 🖼 Póster vertical ·
          <b>clave gratuita</b><br />
          Arte hecho por la comunidad. Es la que cubre lo que Steam no tiene.
        </small>
        <div class="path-row">
          <input
            type={verSgdb ? "text" : "password"}
            placeholder="clave de steamgriddb.com"
            autocomplete="off"
            value={claveSgdb}
            onchange={(e) => guardarClave("sgdb", e.currentTarget.value)}
          />
          <button class="btn" title="Mostrar la clave" onclick={() => (verSgdb = !verSgdb)}>
            {verSgdb ? "🙈" : "👁"}
          </button>
        </div>
        <small class="hint">
          Se saca en <b>steamgriddb.com</b> → cuenta gratuita → <i>Preferences › API</i>.
        </small>
      </div>

      <div class="fuente-arte">
        <label class="check">
          <input
            type="checkbox"
            checked={fuenteRawg}
            onchange={(e) => alternarFuente("rawg", e.currentTarget.checked)}
          />
          <b>RAWG</b>
        </label>
        <small class="hint">
          🎯 500 000+ fichas, también de consola · 🖼 <b>Imagen apaisada</b> ·
          <b>clave gratuita</b><br />
          Apagada de fábrica: lo que devuelve es una captura horizontal, que en una tarjeta
          vertical se recorta mal. Útil como último recurso, o con las tarjetas cuadradas.
        </small>
        <div class="path-row">
          <input
            type={verRawg ? "text" : "password"}
            placeholder="clave de rawg.io"
            autocomplete="off"
            value={claveRawg}
            onchange={(e) => guardarClave("rawg", e.currentTarget.value)}
          />
          <button class="btn" title="Mostrar la clave" onclick={() => (verRawg = !verRawg)}>
            {verRawg ? "🙈" : "👁"}
          </button>
        </div>
        <small class="hint">Se saca en <b>rawg.io</b> → registro → <i>API key</i>.</small>
      </div>

      <div class="path-row">
        <button class="btn" onclick={importarClaves} disabled={importando}>
          {importando ? "Leyendo…" : "⇣ Importar las claves de TimeTrack"}
        </button>
      </div>
      {#if importeMsg}<small class="hint">{importeMsg}</small>{/if}

      <div class="section-title">Otros</div>

      <label class="check">
        <input
          type="checkbox"
          checked={prefs.artSteamCdn}
          onchange={(e) => setPrefs({ artSteamCdn: e.currentTarget.checked })}
        />
        Completar con el CDN de Steam al pintar
        <small class="hint">
          Para los juegos que ya tienen appid. No descarga nada: la tarjeta apunta directamente
          a la imagen de Valve.
        </small>
      </label>

      <label class="check">
        <input
          type="checkbox"
          checked={arteTT}
          onchange={(e) => alternarArteTT(e.currentTarget.checked)}
        />
        Usar también el arte que haya descargado TimeTrack
        <small class="hint">
          Opcional y desactivado por defecto: la búsqueda de arriba no depende de TimeTrack.
          Solo rellena huecos, nunca pisa una carátula que ya tengas.
        </small>
      </label>
    </div>

    <!-- ── Mando ───────────────────────────────────────────────────────────── -->
    {/if}
    {#if pestana === "mando"}
    <div class="section">
      <p class="muted small">
        Pulsa <b>Cambiar</b> y después el botón que quieras asignar. No se remapean: la
        <b>cruceta</b> (salta de opción), el <b>stick izquierdo</b> (desplaza el panel) y el
        <b>stick derecho</b> (mueve el cursor).
      </p>

      <label>
        Distribución de botones
        <select
          value={prefs.gamepadLayout}
          onchange={(e) =>
            setPrefs({ gamepadLayout: e.currentTarget.value as "auto" | "xbox" | "playstation" })}
        >
          <option value="auto">Automática (según el mando conectado)</option>
          <option value="xbox">Xbox — A B X Y</option>
          <option value="playstation">PlayStation — ✕ ○ □ △</option>
        </select>
        <small class="hint">
          Solo cambia los nombres y los colores que se muestran; los botones físicos son los
          mismos.
        </small>
      </label>

      <div class="mapa">
        {#each ACCIONES as a (a.id)}
          <div class="mapa-fila" class:capturando={capturando === a.id}>
            <Tecla boton={prefs.gamepadMap[a.id]} />
            <span class="mapa-texto">
              <b>{a.etiqueta}</b>
              <em>
                {a.ayuda}
                {#if duplicado(a.id)}
                  <span class="dup">· comparte botón con «{duplicado(a.id)}»</span>
                {/if}
              </em>
            </span>
            {#if capturando === a.id}
              <button class="btn sm primary" onclick={() => oncapturar(null)}>
                Pulsa un botón o una combinación… (Esc cancela)
              </button>
            {:else}
              <button class="btn sm" onclick={() => oncapturar(a.id)}>Cambiar</button>
              <button
                class="btn sm"
                title="Volver a la asignación de fábrica"
                disabled={esDefecto(a.id)}
                onclick={() => restablecerAccion(a.id)}>↺</button
              >
            {/if}
          </div>
        {/each}
      </div>

      <label class="check">
        <input
          type="checkbox"
          checked={prefs.gamepadHints}
          onchange={(e) => setPrefs({ gamepadHints: e.currentTarget.checked })}
        />
        Mostrar la ayuda de botones abajo a la derecha
      </label>
      <button class="btn" onclick={resetMando}>Restablecer botones</button>
    </div>

    <!-- ── Modo Sofá ───────────────────────────────────────────────────────── -->
    {/if}
    {#if pestana === "sofa"}
    <div class="section">
      <p class="muted small">
        El <b>Modo Sofá</b> lleva el hub a pantalla completa en el televisor y hace que los juegos
        se abran ahí. Está <b>en construcción</b>: de momento esto solo enseña lo que el core ve.
      </p>

      {#if pantallasError}
        <p class="err">{pantallasError}</p>
      {:else if pantallas.length === 0}
        <p class="muted small">Leyendo las pantallas…</p>
      {:else}
        <div class="section-title">Pantallas conectadas</div>
        <SelectorPantalla {pantallas} bind:elegida={pantallaElegida} />
        <small class="hint">
          Colocadas como en el escritorio, con su resolución <b>física</b> y su escala. Las
          medidas son las que usará el modo para colocar el hub y los juegos.
          {#if pantallas.length === 1}
            Con una sola pantalla no habrá nada que elegir: se entra directo.
          {:else if pantallas.find((p) => p.id === pantallaElegida)?.primaria}
            Esta ya es la principal: elegirla <b>no</b> cambiará nada del sistema.
          {:else}
            Elegir esta supondrá <b>cambiar de pantalla principal</b> mientras dure la sesión, y
            devolverlo todo al salir.
          {/if}
        </small>
      {/if}

      <div class="section-title">Entrar con el mando</div>
      <label class="check">
        <input
          type="checkbox"
          checked={mandoCfg.escucha}
          onchange={(e) => guardarMando({ escucha: e.currentTarget.checked })}
        />
        Escuchar el mando desde el núcleo
        <small class="hint">
          Es lo que permite abrir el hub con el mando <b>estando cerrado</b>, mientras el icono
          siga en la bandeja. Con la app cerrada del todo no hay nada escuchando: no hay un
          vigilante permanente. Cuesta ~2 MB y una fracción de un núcleo.
        </small>
      </label>

      {#if mandoCfg.escucha}
        <div class="grid2">
          <label>
            Botón
            <select
              value={mandoCfg.boton}
              onchange={(e) => guardarMando({ boton: Number(e.currentTarget.value) })}
            >
              {#each BOTONES_SOFA as b}<option value={b.i}>{b.texto}</option>{/each}
            </select>
          </label>
          <label>
            Cómo activarlo
            <select
              value={mandoCfg.gesto}
              onchange={(e) =>
                guardarMando({ gesto: e.currentTarget.value as MandoConfig["gesto"] })}
            >
              <option value="mantener">Manteniéndolo pulsado</option>
              <option value="pulsar">Una pulsación</option>
              <option value="doble">Dos pulsaciones seguidas</option>
            </select>
          </label>
        </div>
        {#if mandoCfg.gesto === "mantener"}
          <label>
            Cuánto hay que mantenerlo: {(mandoCfg.mantener_ms / 1000).toFixed(1)} s
            <input
              type="range"
              min="300"
              max="3000"
              step="100"
              value={mandoCfg.mantener_ms}
              onchange={(e) => guardarMando({ mantener_ms: Number(e.currentTarget.value) })}
            />
          </label>
        {/if}
        <small class="hint">
          Este botón lo lleva <b>entero</b> el núcleo, que es quien distingue una pulsación corta
          de una larga. La corta sigue haciendo lo que tenga asignado —de fábrica, la guía de
          botones—, así que ya no se solapan.<br />
          El <b>botón Xbox</b> no vale para el gesto de mantener: mantenerlo <b>apaga el mando</b>.
        </small>

        <label class="check">
          <input
            type="checkbox"
            checked={mandoCfg.xbox_trae}
            onchange={(e) => guardarMando({ xbox_trae: e.currentTarget.checked })}
          />
          El botón Xbox trae el hub al frente
          <small class="hint">
            Funciona esté la ventana <b>cerrada, minimizada o detrás de un juego</b>. Si ya está
            delante, el botón pasa a hacer otra cosa según dónde estés.
          </small>
        </label>
        {#if mandoCfg.xbox_trae}
          <label class="check">
            <input
              type="checkbox"
              checked={mandoCfg.xbox_doble}
              onchange={(e) => guardarMando({ xbox_doble: e.currentTarget.checked })}
            />
            …pero exigiendo <b>dos pulsaciones</b> seguidas
            <small class="hint">
              Dentro de un segundo. Útil si se roza sin querer al coger el mando.
            </small>
          </label>
        {/if}
      {/if}
    </div>

    <!-- ── TimeTrack ───────────────────────────────────────────────────────── -->
    {/if}
    {#if pestana === "timetrack"}
    <div class="section">
      <p class="muted small">
        El hub <b>no</b> trackea: lee las horas de TimeTrack y reutiliza el arte que este ya
        descarga. Cerrar el hub nunca cierra TimeTrack.
      </p>

      <label>
        Dirección de su API
        <div class="path-row">
          <input bind:value={tt.base} placeholder="http://127.0.0.1:31337" />
          <button class="btn" onclick={sincronizarTT} disabled={ttOcupado}>
            {ttOcupado ? "Sincronizando…" : "Sincronizar ahora"}
          </button>
        </div>
      </label>

      <label>
        Carpeta de TimeTrack <small class="hint">(para leer las imágenes de disco)</small>
        <div class="path-row">
          <input value={tt.dir ?? ""} readonly placeholder="M:\!L\CODE\TimeTrack" />
          <button class="btn" onclick={elegirDirTT}>Examinar…</button>
        </div>
      </label>

      {#if ttSync}
        {#if ttSync.estado.kind === "conectado"}
          <p class="muted small">
            ✅ Conectado · perfil <b>{ttSync.estado.profile}</b> · {ttSync.apps} apps ·
            {ttSync.con_tiempo} con tiempo · {ttSync.con_arte} carátulas nuevas
          </p>
        {:else}
          <p class="warn-banner">
            TimeTrack no responde en <b>{tt.base}</b>. Ábrelo (queda en la bandeja) y vuelve a
            sincronizar.
          </p>
        {/if}
      {/if}
      {#if ttError}<p class="err">{ttError}</p>{/if}
    </div>
    {/if}

    <div class="actions between">
      <button class="btn" onclick={resetPrefs}>Restablecer opciones</button>
      <button class="btn primary" onclick={onclose}>Cerrar</button>
    </div>
  </div>
</div>
