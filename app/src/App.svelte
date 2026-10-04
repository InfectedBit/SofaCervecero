<script lang="ts">
  import { onMount } from "svelte";
  import {
    api,
    events,
    type Category,
    type GameCard,
    type GameDetail,
    type GameState,
    type LaunchMode,
    type LaunchOptions,
    type LibraryFilter,
    type StateCounts,
  } from "./lib/api";
  import TopBar from "./components/TopBar.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import GameGrid from "./components/GameGrid.svelte";
  import DetailCard from "./components/DetailCard.svelte";
  import EditGameDialog from "./components/EditGameDialog.svelte";
  import AddSourceDialog from "./components/AddSourceDialog.svelte";
  import SourcesDialog from "./components/SourcesDialog.svelte";
  import LaunchDialog from "./components/LaunchDialog.svelte";
  import CategoriesDialog from "./components/CategoriesDialog.svelte";
  import SettingsDialog from "./components/SettingsDialog.svelte";
  import SofaDialog from "./components/SofaDialog.svelte";
  import SofaConfirmar from "./components/SofaConfirmar.svelte";
  import CirculoProgreso from "./components/CirculoProgreso.svelte";
  import {
    sofa,
    entrarSofa,
    salirSofa,
    confirmarSofa,
    revertirReparto,
    alternarPantallaCompleta,
    cargarMonitorSofa,
  } from "./lib/sofa.svelte";
  import GamepadGuide from "./components/GamepadGuide.svelte";
  import GamepadHints from "./components/GamepadHints.svelte";
  import ActiveFilters from "./components/ActiveFilters.svelte";
  import ConfirmDialog from "./components/ConfirmDialog.svelte";
  import MenuContextual from "./components/MenuContextual.svelte";
  import {
    abrirContextual,
    cerrarContextual,
    contextual,
    type OpcionContextual,
  } from "./lib/contextual.svelte";
  import { confirmar, confirmacion } from "./lib/confirmar.svelte";
  import { porEje } from "./lib/categorias";
  import { lanzarTarea, progresoTarea, hayTareas } from "./lib/tareas.svelte";
  import { prefs, setPrefs, aplicarPrefsTema, mando } from "./lib/prefs.svelte";
  import { iniciarMando, enviarTecla, ACCIONES, type AccionMando } from "./lib/gamepad";
  import {
    hayDialogo,
    marcarNavegacionMando,
    marcarCursorMando,
    escucharRaton,
    desplazar,
  } from "./lib/focus";

  let categories = $state<Category[]>([]);
  let games = $state<GameCard[]>([]);
  let counts = $state<StateCounts>({
    installed: 0,
    uninstalled: 0,
    excluded: 0,
    favorites: 0,
    recent: 0,
    needs_exe: 0,
  });
  let platforms = $state<string[]>([]);
  let origins = $state<string[]>([]);
  // El orden es una preferencia de vista, así que la sesión empieza con el último elegido.
  let filter = $state<LibraryFilter>({ sort: prefs.orden, desc: prefs.ordenDesc });
  let detail = $state<GameDetail | null>(null);
  let editing = $state<GameDetail | null>(null);
  let showAdd = $state(false);
  let showSources = $state(false);
  let showSettings = $state(false);
  let showCategories = $state(false);
  let showGuide = $state(false);
  /** La pregunta de entrada al Modo Sofá (guión E.0_4 §6.1). */
  let showSofa = $state(false);
  /** Acción del mando en espera de que se pulse un botón (remapeo en Ajustes). */
  let capturando = $state<AccionMando | null>(null);
  let mandoConectado = $state(false);
  /** Id de la tarea de escaneo en curso, para dirigirle los eventos de progreso. */
  let tareaEscaneo = $state<string | null>(null);
  let launching = $state<LaunchOptions & { id: number } | null>(null);
  let toast = $state<string | null>(null);
  let toastTimer: ReturnType<typeof setTimeout>;

  /** Escaneando = hay una tarea de escaneo viva. */
  let scanning = $derived(tareaEscaneo !== null);

  /** ¿Hay un menú emergente abierto? La ayuda de botones cambia según el contexto. */
  let enDialogo = $derived(
    !!detail ||
      !!editing ||
      !!launching ||
      showAdd ||
      showSources ||
      showSettings ||
      showCategories ||
      showGuide ||
      // El menú contextual y las confirmaciones también son capas: mientras estén abiertos, la
      // ayuda de botones debe hablar de ellos y no del grid.
      contextual.abierto ||
      !!confirmacion.actual ||
      sofa.esperandoConfirmar,
  );

  async function reloadGames() {
    games = await api.libraryList(filter);
  }
  async function refresh() {
    [categories, counts, platforms, origins] = await Promise.all([
      api.categoriesList(),
      api.stateCounts(prefs.recientesDias),
      api.platformsList(),
      api.originsList(),
    ]);
    await reloadGames();
  }
  /** Cambia uno o varios campos del filtro y recarga (los filtros son acumulativos). */
  async function setFilter(patch: Partial<LibraryFilter>) {
    filter = { ...filter, ...patch };
    await reloadGames();
  }
  /** Limpia solo los criterios del panel; lo de la barra lateral (estado, categoría,
   *  favoritos) y la búsqueda se quedan como están. */
  async function clearFilters() {
    filter = {
      category_id: filter.category_id,
      state: filter.state,
      query: filter.query,
      favorite: filter.favorite,
      added_days: filter.added_days,
      sort: filter.sort,
      desc: filter.desc,
    };
    await reloadGames();
  }
  /** Quita un único criterio desde la barra de filtros activos. */
  async function quitarFiltro(clave: keyof LibraryFilter) {
    // El tiempo jugado es **una** dimensión repartida en tres campos: su chip los quita todos.
    // Dejar `min_hours` puesto al quitar «Ya jugados» filtraría en silencio (guión T.0_2 §2).
    if (clave === "played") {
      await setFilter({ played: null, min_hours: null, max_hours: null });
      return;
    }
    await setFilter({ [clave]: clave === "sort" ? "title" : null });
  }
  /** Devuelve orden o secciones a su valor por defecto desde la barra de filtros activos. */
  async function resetVista(que: "orden" | "seccion") {
    if (que === "seccion") {
      setPrefs({ seccion: "none", seccion2: "none" });
      return;
    }
    setPrefs({ orden: "title", ordenDesc: false });
    await setFilter({ sort: "title", desc: false });
  }
  async function limpiarTodo() {
    filter = { sort: filter.sort, desc: filter.desc };
    await reloadGames();
  }
  /**
   * Estado y categoría son **dimensiones independientes**: elegir una no borra la otra, así
   * se pueden ver "los excluidos de Steam" y ambas quedan resaltadas (guión F.0_3 §1).
   */
  async function selectCategory(id: number | null) {
    await setFilter({ category_id: id });
  }
  async function selectState(s: GameState | null) {
    await setFilter({ state: s });
  }
  async function selectNeedsExe(activo: boolean) {
    await setFilter({ needs_exe: activo ? true : null });
  }
  async function selectFavoritos(activo: boolean) {
    await setFilter({ favorite: activo ? true : null });
  }
  async function selectRecientes(activo: boolean) {
    await setFilter({ added_days: activo ? prefs.recientesDias : null });
  }

  /**
   * Escanear ya no bloquea nada: va a la cola de tareas (guión J.0_1). El id de la tarea
   * viaja al core para que se pueda cancelar a media faena.
   */
  function scan(etiqueta = "Escaneando la biblioteca") {
    tareaEscaneo = lanzarTarea(
      "scan",
      etiqueta,
      async (id) => {
        const s = await api.scanRun(id);
        await refresh();
        const partes = [`${s.scanned} elementos`, `${s.added} nuevos`];
        if (s.uninstalled) partes.push(`${s.uninstalled} ya no están`);
        if (s.excluded) partes.push(`${s.excluded} excluidos`);
        if (s.skipped.length) partes.push(`${s.skipped.length} fuente(s) inaccesible(s)`);
        if (s.cancelada) partes.push("cancelado a medias");
        return partes.join(" · ");
      },
      () => (tareaEscaneo = null),
    );
  }

  async function openDetail(id: number) {
    detail = await api.gameGet(id);
  }

  /**
   * Punto único de lanzamiento. Si el juego tiene launcher y no hay modo recordado,
   * pregunta antes (guión C.0_3) en vez de lanzar las dos cosas a la vez.
   */
  async function launch(id: number, executableId?: number, preguntar = false) {
    try {
      const opts = await api.launchOptions(id, executableId);
      if (opts?.needs_prompt) {
        launching = { ...opts, id };
        return;
      }
      // Un juego sin popup propio se lanzaba al primer clic, y desde el grid el botón ▶ está a
      // un descuido de distancia. Se pregunta **solo aquí**: si el juego ya tenía su diálogo
      // —launcher, ejecutable elegido o confirmación de categoría— encadenar dos preguntas
      // seguidas para lo mismo es ruido, no seguridad.
      if (preguntar) {
        const ok = await confirmar({
          titulo: `¿Lanzar ${opts?.title ?? "el juego"}?`,
          aceptar: "LANZAR",
        });
        if (!ok) return;
      }
      // Al elegir un ejecutable adicional concreto, el usuario ya ha dicho qué quiere:
      // se lanza ese y solo ese, sin arrastrar el launcher ni el modo recordado.
      await api.gameLaunch(id, {
        executableId,
        mode: executableId !== undefined ? "game" : (opts?.remembered ?? undefined),
      });
      notify("Lanzando…");
    } catch (e) {
      notify("No se pudo lanzar: " + e);
    }
  }
  async function launchWith(mode: LaunchMode, remember: boolean) {
    const target = launching;
    launching = null;
    if (!target) return;
    try {
      await api.gameLaunch(target.id, { mode, remember });
      notify(mode === "client" ? "Abriendo el launcher…" : "Lanzando…");
    } catch (e) {
      notify("No se pudo lanzar: " + e);
    }
  }

  /**
   * Abre las gráficas de TimeTrack de este juego, en la propia ventana de TimeTrack.
   *
   * No se dibujan aquí: TimeTrack ya tiene ese panel, con zoom e histórico de sesiones. Si no
   * está arrancado se dice, y no se arranca — el hub nunca decide por él (plan §4.5).
   */
  async function verGraficas(id: number) {
    try {
      await api.timetrackOpenApp(id);
    } catch (e) {
      notify("" + e);
    }
  }

  function editGame(d: GameDetail) {
    editing = d;
    detail = null;
  }
  async function deleteGame(d: GameDetail) {
    await api.gameDelete(d.id);
    detail = null;
    await refresh();
    notify("Eliminado");
  }
  /** Excluir / restaurar / marcar como no instalado (guiones L.0_1 y L.0_2 §1). */
  async function setState(d: GameCard | GameDetail, s: GameState) {
    await api.gameSetState(d.id, s);
    detail = null;
    await refresh();
    notify(
      s === "excluded"
        ? "Excluido: no volverá a aparecer al escanear"
        : s === "uninstalled"
          ? "Marcado como no instalado"
          : "Restaurado a la biblioteca",
    );
  }

  /**
   * Favorito. Es **ortogonal al estado** (guión L.0_2 §2): no mueve el juego de sitio, solo
   * lo marca. Se refresca todo porque el contador de la barra lateral también cambia.
   */
  async function setFavorite(d: GameCard | GameDetail, favorito: boolean) {
    await api.gameSetFavorite(d.id, favorito);
    if (detail && detail.id === d.id) detail = { ...detail, favorite: favorito };
    await refresh();
    notify(favorito ? "Añadido a favoritos" : "Quitado de favoritos");
  }

  /**
   * Menú contextual de un juego (guión A.0_4 §4). Las opciones dependen de su estado: no
   * tiene sentido ofrecer «Lanzar» en algo que no está instalado.
   */
  /**
   * Juego que se está clasificando desde el menú contextual, con una copia **viva** de sus
   * categorías: el submenú se repinta tras cada clic y tiene que ver el cambio al instante,
   * sin esperar a que recargue la biblioteca.
   */
  let clasificando: { id: number; cats: Set<number> } | null = null;

  /** El árbol de categorías como páginas del submenú: un rótulo por eje y sus hijas debajo. */
  function opcionesClasificar(): OpcionContextual[] {
    const estado = clasificando;
    if (!estado) return [];
    const out: OpcionContextual[] = [];
    for (const { eje, hijas } of porEje(categories)) {
      out.push({ etiqueta: eje?.name ?? "Sin eje", cabecera: true });
      for (const c of hijas) {
        const puesta = estado.cats.has(c.id);
        out.push({
          etiqueta: c.name,
          icono: puesta ? "●" : "○",
          activa: puesta,
          // No cierra el menú: se marcan varias seguidas y el acento se actualiza.
          mantener: true,
          accion: async () => {
            // Se actualiza la copia local **antes** de la llamada para que el repintado sea
            // inmediato; si el core fallara, el `refresh` siguiente deja la verdad.
            if (puesta) {
              estado.cats.delete(c.id);
              await api.categoryUnassign(estado.id, c.id);
            } else {
              estado.cats.add(c.id);
              await api.categoryAssign(estado.id, c.id);
            }
            await refresh();
          },
        });
      }
    }
    return out;
  }

  /**
   * Abre la carpeta del juego en el explorador. El error se enseña tal cual porque nombra la
   * ruta que falla: «la ruta ya no existe: D:\Juegos\X» dice qué corregir en el editor.
   */
  async function abrirCarpeta(id: number) {
    try {
      await api.gameOpenDir(id);
    } catch (e) {
      notify("No se pudo abrir la carpeta: " + e);
    }
  }

  function menuJuego(e: MouseEvent, g: GameCard) {
    const opciones: OpcionContextual[] = [];
    if (g.state === "installed" && !g.needs_exe) {
      opciones.push({ etiqueta: "Lanzar", icono: "▶", accion: () => launch(g.id) });
    }
    opciones.push({
      etiqueta: "Editar…",
      icono: "✎",
      accion: async () => {
        const d = await api.gameGet(g.id);
        if (d) editGame(d);
      },
    });
    opciones.push({
      etiqueta: "Clasificar",
      icono: "🗂️",
      submenu: () => {
        clasificando = { id: g.id, cats: new Set(g.categories) };
        return opcionesClasificar();
      },
    });
    opciones.push({
      etiqueta: g.favorite ? "Quitar de favoritos" : "Añadir a favoritos",
      icono: g.favorite ? "★" : "☆",
      accion: () => setFavorite(g, !g.favorite),
    });
    // Solo si hay alguna ruta registrada: en un juego de tienda sin instalar no hay nada que
    // abrir, y una opción que siempre falla es peor que no tenerla.
    if (g.has_dir) {
      opciones.push({ etiqueta: "Abrir carpeta del juego", icono: "📂", accion: () => abrirCarpeta(g.id) });
    }
    if (g.state === "uninstalled") {
      opciones.push({
        etiqueta: "Quitar de «No instalados»",
        icono: "↩",
        separar: true,
        accion: () => setState(g, "installed"),
      });
    } else if (g.state === "installed") {
      opciones.push({
        etiqueta: "Marcar como no instalado",
        icono: "📦",
        separar: true,
        accion: () => setState(g, "uninstalled"),
      });
    }
    opciones.push({
      etiqueta: g.state === "excluded" ? "Restaurar a la biblioteca" : "Excluir del escaneo",
      icono: g.state === "excluded" ? "↩" : "🚫",
      accion: () => setState(g, g.state === "excluded" ? "installed" : "excluded"),
    });
    opciones.push({
      etiqueta: "Eliminar…",
      icono: "🗑",
      separar: true,
      peligro: true,
      accion: async () => {
        const ok = await confirmar({
          titulo: `¿Eliminar "${g.title}" de la biblioteca?`,
          mensaje: "Se pierden sus ediciones y categorías.",
          aceptar: "Eliminar",
          peligro: true,
        });
        if (!ok) return;
        await api.gameDelete(g.id);
        await refresh();
        notify("Eliminado");
      },
    });
    abrirContextual(e, g.title, opciones);
  }
  async function afterEdit() {
    editing = null;
    await refresh();
    notify("Guardado");
  }
  async function afterDeleted() {
    editing = null;
    detail = null;
    await refresh();
    notify("Eliminado");
  }
  /**
   * Salta a la categoría anterior/siguiente de la barra lateral (LB/RB). Se hace sobre el
   * DOM para reutilizar exactamente el mismo orden que ve el usuario, incluidos los grupos
   * por estado y los que están ocultos por estar vacíos.
   */
  function moverCategoria(paso: number) {
    if (hayDialogo()) return;
    const items = Array.from(document.querySelectorAll<HTMLElement>(".sidebar .cat"));
    if (items.length === 0) return;
    const i = items.findIndex((el) => el.classList.contains("active"));
    const destino = Math.min(Math.max((i < 0 ? 0 : i) + paso, 0), items.length - 1);
    items[destino].click();
  }

  /**
   * Menú contextual de una categoría de la barra lateral (guión A.0_4 §4).
   * Se apoya en el diálogo de categorías para renombrar y mover, que es donde vive esa lógica.
   */
  function menuCategoria(e: MouseEvent, c: Category) {
    abrirContextual(e, c.name, [
      { etiqueta: "Ver solo esta categoría", icono: "🔎", accion: () => selectCategory(c.id) },
      { etiqueta: "Gestionar categorías…", icono: "🗂", accion: () => (showCategories = true) },
      {
        etiqueta: "Eliminar categoría…",
        icono: "🗑",
        separar: true,
        peligro: true,
        accion: async () => {
          const ok = await confirmar({
            titulo: `¿Eliminar la categoría «${c.name}»?`,
            mensaje:
              c.count > 0
                ? `Sus ${c.count} juego(s) NO se borran: solo dejan de estar en esta categoría.`
                : undefined,
            aceptar: "Eliminar",
            peligro: true,
          });
          if (!ok) return;
          await api.categoryDelete(c.id);
          if (filter.category_id === c.id) filter = { ...filter, category_id: null };
          await refresh();
          notify("Categoría eliminada");
        },
      },
    ]);
  }

  /**
   * El menú del navegador no aparece en ningún sitio: o sale el nuestro, o no sale nada
   * (guión A.0_4 §4). La excepción son los campos de texto, donde cortar/pegar del sistema
   * sí es útil.
   */
  function contextMenuGlobal(e: MouseEvent) {
    const t = e.target as HTMLElement | null;
    const editable =
      !!t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable);
    if (!editable) e.preventDefault();
  }

  /**
   * Aviso efímero abajo del todo. Se va **solo a los 3 s**, o antes si se pulsa su ✕
   * (guión A.0_6 §3): hay mensajes que uno quiere quitarse de en medio ya, y otros que conviene
   * poder leer dos veces.
   */
  /**
   * **F11** alterna la pantalla completa, como en cualquier aplicación — y **no** sale del Modo
   * Sofá: el modo es la tipografía grande y la interfaz podada, que siguen teniendo sentido en
   * ventana. Para salir del modo está su propio botón.
   */
  /** Progreso del botón mantenido (0–1), o `null` si no se está manteniendo nada. */
  let sofaProgreso = $state<number | null>(null);
  /** Botón que el **núcleo** se reserva para el gesto del Modo Sofá. De fábrica, Select. */
  let botonSofa = $state(8);

  /**
   * El Modo Sofá se marca en el **elemento raíz**, no en `.app`: los diálogos se montan fuera
   * de `.app`, así que una clase ahí los dejaba sin escalar. Mismo recurso que `data-nav`.
   */
  $effect(() => {
    document.documentElement.dataset.sofa = sofa.activo ? "1" : "";
  });

  /**
   * Lo que hace cada acción del mando. Está aparte porque la llaman **dos** sitios: el sondeo
   * de la interfaz y el núcleo, cuando detecta una pulsación corta del botón que tiene
   * reservado (guión E.0_4 §9.1).
   */
  function ejecutarAccion(a: AccionMando) {
    switch (a) {
      case "detalle":
        enviarTecla("Enter");
        break;
      case "lanzar":
        // Dentro de un menú, Ctrl+Enter no significa nada: vale como "aceptar".
        enviarTecla("Enter", !hayDialogo());
        break;
      case "atras":
        enviarTecla("Escape");
        break;
      case "buscar":
        if (!hayDialogo()) document.querySelector<HTMLElement>(".search")?.focus();
        break;
      case "categoria_anterior":
        moverCategoria(-1);
        break;
      case "categoria_siguiente":
        moverCategoria(1);
        break;
      case "menu":
        // La card ya sabe abrir su menú con esta tecla (la del teclado, junto a Ctrl).
        if (contextual.abierto) cerrarContextual();
        else enviarTecla("ContextMenu");
        break;
      case "guia":
        showGuide = !showGuide;
        break;
      case "ajustes":
        showSettings = !showSettings;
        break;
      case "escanear":
        if (!hayTareas() && !hayDialogo()) scan();
        break;
    }
  }

  /**
   * El botón Xbox, **una sola pulsación**, con una acción distinta según dónde estemos
   * (guión E.0_4 §9). Lo que pasa con la ventana cerrada lo resuelve el núcleo; aquí solo
   * llegan los casos en los que hay interfaz.
   */
  function botonXbox() {
    if (!sofa.activo) {
      // Modo normal: traerla al frente es cosa del núcleo, así que aquí no hay nada que hacer.
      return;
    }
    // En Modo Sofá y sin juego delante, preguntar si salir. (Con un juego delante este botón
    // tabulará al menú superpuesto: fase 6.)
    confirmarSalirSofa();
  }

  async function confirmarSalirSofa() {
    const ok = await confirmar({
      titulo: "¿Salir del Modo Sofá?",
      mensaje:
        sofa.repartoAplicado === "ninguna"
          ? undefined
          : "Las pantallas volverán a como estaban antes de entrar.",
      aceptar: "Salir",
    });
    if (ok) await salirSofa();
  }

  function teclaGlobal(e: KeyboardEvent) {
    if (e.key !== "F11" || !sofa.activo) return;
    e.preventDefault();
    alternarPantallaCompleta();
  }

  function notify(msg: string) {
    toast = msg;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = null), 3000);
  }
  function cerrarToast() {
    clearTimeout(toastTimer);
    toast = null;
  }

  onMount(() => {
    aplicarPrefsTema();
    cargarMonitorSofa();
    api.mandoConfig().then((c) => (botonSofa = c.escucha ? c.boton : -1));
    refresh().then(async () => {
      // Esta preferencia vive en el **core**, no en `localStorage` (guión H.0_3 §2). Cerrar el
      // hub destruye el WebView2, y Chromium escribe `localStorage` en diferido: desmarcar la
      // casilla y cerrar la ventana podía perder el cambio, y a la siguiente apertura se volvía
      // a recorrer el disco entero. SQLite confirma la escritura en el acto.
      // Ausente = sí: el escaneo al abrir viene marcado de fábrica.
      if ((await api.settingGet("hub.scan_on_start")) !== "0") scan();
    });
    // Eventos del core: progreso real del escaneo en lugar de un spinner ciego (C.0_2 §3).
    const subs = [
      // El progreso va a la tarea correspondiente, que es quien lo pinta (guión J.0_1).
      events.onScanProgress((p) => {
        if (tareaEscaneo) progresoTarea(tareaEscaneo, p);
      }),
      // Venció la cuenta atrás del Modo Sofá y el núcleo deshizo el cambio de pantallas. Hay
      // que decirlo: si no, la interfaz seguiría creyendo que el televisor es el principal.
      events.onSofaRevertido(() => {
        sofa.repartoAplicado = "ninguna";
        sofa.esperandoConfirmar = false;
        notify("No se confirmó a tiempo: las pantallas vuelven a como estaban.");
      }),
      // ── Gestos del mando leídos por el núcleo (guión E.0_4 §9) ──────────────────
      events.onMandoSofaProgreso((p) => (sofaProgreso = p < 0 ? null : p)),
      events.onMandoSofa(() => {
        sofaProgreso = null;
        // Mantener SELECT alterna el modo. Con el modo puesto se pregunta antes de salir:
        // salir de golpe devolvería las pantallas sin avisar.
        if (sofa.activo) confirmarSalirSofa();
        else showSofa = true;
      }),
      events.onMandoXbox(() => botonXbox()),
      // El botón del gesto lo lleva entero el núcleo, incluida su pulsación corta: es quien
      // distingue corta de larga. Aquí solo se ejecuta la acción que tenga asignada, que de
      // fábrica es la guía — así dejan de solaparse (guión E.0_4 §9.1).
      events.onMandoSofaCorto(() => {
        const boton = botonSofa;
        const accion = ACCIONES.find((a) => prefs.gamepadMap[a.id] === boton);
        if (accion) ejecutarAccion(accion.id);
      }),
    ];

    // El mando reutiliza la navegación por foco de A.0_2: solo traduce a teclas, y las
    // dirige a la capa activa (el menú abierto, o el grid) — guión E.0_2.
    const pararMando = iniciarMando({
      mapa: () => prefs.gamepadMap,
      captura: () => {
        const accion = capturando;
        if (!accion) return null;
        // Llega **la combinación** completa, ya soltada. Un solo botón se guarda como número
        // para no ensuciar lo que ya hubiera guardado.
        return (botones: number[]) => {
          if (botones.length === 0) return;
          const valor = botones.length === 1 ? botones[0] : botones;
          setPrefs({ gamepadMap: { ...prefs.gamepadMap, [accion]: valor } });
          capturando = null;
        };
      },
      tecla: (key, ctrl) => {
        // Sin esta marca el foco se mueve pero no se ve: `:focus-visible` no se activa
        // con eventos sintéticos (ver guión E.0_3 §1).
        marcarNavegacionMando();
        enviarTecla(key, ctrl);
      },
      // Stick izquierdo: recorre el panel como la rueda del ratón. La cruceta es la que
      // salta de opción en opción.
      desplazar: (dy) => {
        marcarNavegacionMando();
        desplazar(dy);
      },
      // El stick derecho mueve el cursor real del sistema; el gatillo hace clic. Así se
      // llega con el mando a lo que aún no es navegable por foco.
      cursor: (dx, dy) => {
        marcarCursorMando();
        api.cursorMove(dx, dy).catch(() => {});
      },
      clic: (pulsado) => {
        marcarCursorMando();
        api.cursorButton(true, pulsado).catch(() => {});
      },
      clicDerecho: (pulsado) => {
        marcarCursorMando();
        api.cursorButton(false, pulsado).catch(() => {});
      },
      // El botón que lleva el núcleo no lo toca el sondeo de la interfaz: si lo tocara, su
      // acción se dispararía al **pulsar** y se solaparía con el gesto de mantener.
      reservado: () => botonSofa,
      accion: ejecutarAccion,
      conexion: (conectado, nombre, layout) => {
        mandoConectado = conectado;
        if (conectado) mando.layout = layout;
        notify(conectado ? `Mando conectado: ${nombre}` : "Mando desconectado");
      },
    });

    const pararRaton = escucharRaton();

    return () => {
      subs.forEach((s) => s.then((off) => off()));
      pararMando();
      pararRaton();
    };
  });
</script>

<svelte:window oncontextmenu={contextMenuGlobal} onkeydown={teclaGlobal} />

<div class="app">
  <TopBar
    {filter}
    {platforms}
    {origins}
    {categories}
    {scanning}
    onfilter={setFilter}
    onclear={clearFilters}
    onscan={scan}
    onadd={() => (showAdd = true)}
    onsources={() => (showSources = true)}
    oncategories={() => (showCategories = true)}
    onsettings={() => (showSettings = true)}
    onsofa={() => (sofa.activo ? salirSofa() : (showSofa = true))}
  />
  <div class="body">
    <Sidebar
      {categories}
      {counts}
      selectedCategory={filter.category_id ?? null}
      selectedState={filter.state ?? null}
      needsExe={filter.needs_exe === true}
      soloFavoritos={filter.favorite === true}
      soloRecientes={(filter.added_days ?? 0) > 0}
      diasRecientes={prefs.recientesDias}
      onselect={selectCategory}
      onselectstate={selectState}
      onneedsexe={selectNeedsExe}
      onfavoritos={selectFavoritos}
      onrecientes={selectRecientes}
      onmenucategoria={menuCategoria}
    />
    <div class="contenido">
      <ActiveFilters
        {filter}
        {categories}
        total={games.length}
        onquitar={quitarFiltro}
        onlimpiar={limpiarTodo}
        onvista={resetVista}
      />
      <GameGrid
        {games}
        {categories}
        {scanning}
        onopen={openDetail}
        onlaunch={(id) => launch(id, undefined, true)}
        onadd={() => (showAdd = true)}
        onmenu={menuJuego}
      />
    </div>
  </div>
</div>

{#if detail}
  <DetailCard
    {detail}
    onclose={() => (detail = null)}
    onlaunch={launch}
    onedit={editGame}
    ondelete={deleteGame}
    onsetstate={setState}
    onfavorite={setFavorite}
    ongraficas={verGraficas}
  />
{/if}
{#if editing}
  <EditGameDialog
    detail={editing}
    {categories}
    onclose={() => (editing = null)}
    onsaved={afterEdit}
    ondeleted={afterDeleted}
  />
{/if}
{#if launching}
  <LaunchDialog
    options={launching}
    onclose={() => (launching = null)}
    onchoose={launchWith}
  />
{/if}
{#if showCategories}
  <CategoriesDialog
    {categories}
    onclose={() => (showCategories = false)}
    onchanged={refresh}
  />
{/if}
{#if showSources}
  <SourcesDialog
    {categories}
    onclose={() => (showSources = false)}
    onadd={() => {
      showSources = false;
      showAdd = true;
    }}
    onchanged={refresh}
  />
{/if}
{#if showAdd}
  <AddSourceDialog
    {categories}
    onclose={() => (showAdd = false)}
    onadded={(etiqueta, yaExistia) => {
      // Se cierra al instante y el escaneo sigue en segundo plano: antes el diálogo se
      // quedaba esperando y, al acabar, cerraba el menú que hubiera abierto (J3).
      showAdd = false;
      refresh();
      // Se escanea igual aunque la carpeta ya fuese fuente —puede haber juegos nuevos—, pero
      // se avisa de que no se ha añadido nada: si no, parece que se ha hecho algo y no (A.0_6).
      if (yaExistia) notify("Esa carpeta ya era una fuente. Se vuelve a escanear, sin duplicarla.");
      scan(etiqueta);
    }}
  />
{/if}
{#if showSofa}
  <SofaDialog
    onclose={() => (showSofa = false)}
    onentrar={async (m, reparto) => {
      showSofa = false;
      const err = await entrarSofa(m, reparto);
      if (err) notify("No se pudo entrar en Modo Sofá: " + err);
    }}
  />
{/if}
{#if sofaProgreso !== null}
  <CirculoProgreso
    progreso={sofaProgreso}
    texto={sofa.activo ? "Salir del Modo Sofá…" : "Iniciar Modo Sofá…"}
  />
{/if}
<!-- Ajustes y guía van los últimos: si se abren con el mando teniendo otro menú delante,
     deben quedar encima (la "capa activa" es el último diálogo del DOM). -->
{#if showSettings}
  <SettingsDialog
    onclose={() => {
      showSettings = false;
      capturando = null;
    }}
    onsynced={refresh}
    {capturando}
    oncapturar={(a) => (capturando = a)}
  />
{/if}
{#if showGuide}
  <GamepadGuide
    onclose={() => (showGuide = false)}
    onajustes={() => {
      showGuide = false;
      showSettings = true;
    }}
  />
{/if}

<!-- Capas propias que van por encima de todo: el menú contextual y las confirmaciones. -->
<MenuContextual />
<ConfirmDialog />

<!-- Y la última de todas, por detrás de nadie: la cuenta atrás de la pantalla. Se puede entrar
     en Modo Sofá con el mando teniendo Ajustes abierto, y ahí quedaba debajo. Es justo la capa
     que no puede quedar tapada — si no se ve, no se contesta, y a los 20 s se deshace sola. -->
{#if sofa.esperandoConfirmar}
  <SofaConfirmar onconfirmar={confirmarSofa} onrevertir={revertirReparto} />
{/if}

{#if mandoConectado && prefs.gamepadHints}
  <GamepadHints {enDialogo} />
{/if}
{#if toast}
  <div class="toast" role="status">
    <span>{toast}</span>
    <button class="toast-x" title="Cerrar" onclick={cerrarToast}>✕</button>
  </div>
{/if}
