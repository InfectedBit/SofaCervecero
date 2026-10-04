import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/** Convierte una ruta de disco en URL cargable por el WebView (asset protocol). */
export function coverSrc(p: string | null): string | null {
  return p ? convertFileSrc(p) : null;
}

export type Category = {
  id: number;
  name: string;
  /** `null` = es un **eje** raíz (Clientes, Nº Jugadores, TAGS…). Guión F.0_6. */
  parent_id: number | null;
  /** Posición entre sus hermanas. Arranca alfabética y se cambia a mano. */
  sort_order: number;
  /** Juegos suyos **y de sus descendientes**, sin repetir y sin contar los excluidos. */
  count: number;
  /** Si está, se pide esta confirmación antes de lanzar sus juegos (C8). */
  launch_prompt: string | null;
};

/** Una categoría que encaja mejor en otro eje. Se propone; no se aplica sola. */
export type Recolocacion = {
  id: number;
  name: string;
  /** Nombre del padre actual; `null` si hoy es un eje raíz. */
  desde: string | null;
  hacia: string;
};

/**
 * Estados de un elemento de la biblioteca (guiones L.0_1 y L.0_2).
 *
 * «Deseados» ya no existe: se llamaba así a lo que en realidad es **No instalado**, y
 * confundía con Favoritos, que es otra cosa (y ortogonal: se puede ser las dos).
 */
export type GameState = "installed" | "uninstalled" | "excluded";
export const STATE_LABEL: Record<GameState, string> = {
  installed: "Instalados",
  uninstalled: "No instalados",
  excluded: "Excluidos",
};

export type GameCard = {
  id: number;
  title: string;
  platform: string;
  cover_path: string | null;
  state: GameState;
  /** No tiene ejecutable asignado: hay que editarlo antes de poder lanzarlo. */
  needs_exe: boolean;
  players_min: number | null;
  players_max: number | null;
  /** Tiempo jugado (segundos) según TimeTrack; 0 si no hay dato. */
  playtime_secs: number;
  /** Appid de tienda: permite tirar del CDN de Steam si no hay carátula local (D7). */
  store_id: string | null;
  favorite: boolean;
  /** Ids de sus categorías: la cuadrícula se secciona en el cliente (guión F.0_5). */
  categories: number[];
  /** `kind` de su fuente: `steam` · `folder_library` · `app_entry`. */
  origin: string | null;
  /** Tiene carpeta o ejecutable registrados: se le puede ofrecer «Abrir carpeta» (A.0_5). */
  has_dir: boolean;
};

/**
 * CDN público de Valve. **No pide clave ni registro**, pero sí internet: por eso es el último
 * salto de la cascada de carátulas y se puede desactivar en Ajustes (guión D.0_1).
 */
export function urlCaratulaSteam(appid: string): string {
  return `https://cdn.cloudflare.steamstatic.com/steam/apps/${appid}/library_600x900.jpg`;
}

/** Resultado de buscar carátulas sin APIs (guión D.0_1). */
export type ArtSummary = {
  pendientes: number;
  /** De una imagen que ya estaba en la carpeta del juego. */
  de_carpeta: number;
  /** De la caché local de Steam. */
  de_steam: number;
  /** Buscando el nombre en la tienda de Steam (guión D.0_3). */
  de_steam_web: number;
  de_sgdb: number;
  de_rawg: number;
  /** Del icono del ejecutable, que es el **último** recurso. */
  de_icono: number;
  sin_resolver: number;
  cancelada: boolean;
};

/** Filtros combinables del grid (guión F.0_1). Todo opcional y acumulativo. */
export type LibraryFilter = {
  category_id?: number | null;
  query?: string | null;
  /** Ausente = todos menos los excluidos. */
  state?: GameState | null;
  platform?: string | null;
  /** Admite al menos N jugadores. */
  players?: number | null;
  needs_exe?: boolean | null;
  /** Origen: `steam` · `folder_library` · `app_entry`. */
  origin?: string | null;
  /** `true` = solo jugados; `false` = solo sin jugar. */
  played?: boolean | null;
  /** Horas totales como mínimo (inclusive). Se combina con `played` (guión T.0_2 §2). */
  min_hours?: number | null;
  /** Horas totales como máximo, **exclusivo**: bandas contiguas no se solapan. */
  max_hours?: number | null;
  /** `true` = solo favoritos. */
  favorite?: boolean | null;
  /** Solo los añadidos en los últimos N días. Los que no tienen fecha quedan fuera. */
  added_days?: number | null;
  /** Criterio: `title` · `recent` · `playtime` · `players` · `added`. */
  sort?: string | null;
  /** Dirección. Ausente = la natural del criterio (guión F.0_5 §2). */
  desc?: boolean | null;
};

export type StateCounts = {
  installed: number;
  uninstalled: number;
  excluded: number;
  /** Marcados como favoritos, en cualquier estado. */
  favorites: number;
  /** Añadidos dentro de la ventana de «recientes». */
  recent: number;
  /** Sin ejecutable ni id de tienda: no se pueden lanzar. */
  needs_exe: number;
};

/**
 * Una pantalla conectada (guión E.0_4 fase 1). Todo en píxeles **físicos**: con un televisor al
 * 250 % de escala, las coordenadas lógicas no sirven para colocar nada.
 */
export type Pantalla = {
  /** `\\.\DISPLAY1` — el nombre que entiende Windows para reconfigurarla. */
  id: string;
  /** «LG TV SSCR2» — el que entiende una persona. */
  nombre: string;
  x: number;
  y: number;
  ancho: number;
  alto: number;
  /** 1 = 100 %, 2.5 = 250 %. */
  escala: number;
  primaria: boolean;
};

/**
 * Qué se le hace al escritorio al entrar en Modo Sofá (guión E.0_4 §5):
 * - `ninguna` — no se toca nada; el hub se abre ahí y punto.
 * - `primaria` — la elegida pasa a ser la principal, que es donde abre casi todo.
 * - `solo` — se apagan las demás. La que funciona seguro, y la que más molesta.
 */
export type Reparto = "ninguna" | "primaria" | "solo";

/**
 * Cómo escucha el **núcleo** al mando (guión E.0_4 §9.1). Vive en el core y no en `prefs`
 * porque quien lo lee es el núcleo, que sondea también con la ventana cerrada.
 */
export type MandoConfig = {
  /** Escuchar el mando desde el núcleo. Apagado, no hay sondeo ninguno. */
  escucha: boolean;
  /** Botón del gesto, por índice del *Standard Gamepad*. 8 = Select. */
  boton: number;
  /** Cómo se activa. */
  gesto: "mantener" | "pulsar" | "doble";
  /** Milisegundos del mantenido. */
  mantener_ms: number;
  /** El botón Xbox trae el hub al frente, esté minimizado o detrás de un juego. */
  xbox_trae: boolean;
  /** Exigir **dos pulsaciones seguidas** (dentro de 1 s), por si se roza sin querer. */
  xbox_doble: boolean;
};

/** Qué hacer al pulsar LANZAR cuando hay launcher configurado (guión C.0_3). */
export type LaunchMode = "game" | "client";
export type LaunchOptions = {
  title: string;
  needs_prompt: boolean;
  client_path: string | null;
  exe_path: string | null;
  remembered: LaunchMode | null;
  store: string | null;
  store_id: string | null;
  /** El juego pide una cuenta de Steam distinta de la activa (guión B.0_1 §3). */
  account_warning: { requerida: string; activa: string | null } | null;
  /** Confirmaciones que exigen sus categorías: `[categoría, pregunta]` (C8). */
  confirmaciones: [string, string][];
};
export type Executable = { id: number; label: string; path: string; args: string | null };
export type GameDetail = {
  id: number;
  title: string;
  install_dir: string | null;
  exe_path: string | null;
  exe_args: string | null;
  platform: string;
  /** **Solo la puesta a mano.** Vacía = manda la automática (guión D.0_4 §1). */
  cover_path: string | null;
  /** La que resolvió la cascada. Se enseña, pero no se guarda al pulsar Guardar. */
  art_path: string | null;
  client_path: string | null;
  client_args: string | null;
  client_first: boolean;
  state: GameState;
  launch_mode: LaunchMode | null;
  players_min: number | null;
  players_max: number | null;
  /** Tienda de origen (`steam`…), su id (appid) y la cuenta que requiere. */
  store: string | null;
  store_id: string | null;
  steam_account: string | null;
  exe_exists: boolean;
  launch_count: number;
  last_launched: string | null;
  playtime_secs: number;
  playtime_sync: string | null;
  /** Id de este juego en TimeTrack. `null` = no hay gráficas que abrir (guión T.0_2 §3). */
  tt_app_id: number | null;
  categories: string[];
  executables: Executable[];
  favorite: boolean;
};
export type Source = {
  id: number;
  kind: string;
  path: string;
  default_category_id: number | null;
  enabled: boolean;
  /** Juegos que provienen de esta fuente. */
  game_count: number;
};

/** Progreso del escaneo (evento `scan_progress`). */
export type ScanProgress = {
  source_index: number;
  source_total: number;
  source_path: string;
  current: string;
  found: number;
};
/** Resumen del escaneo (evento `scan_finished` y retorno de `scan_run`). */
export type ScanSummary = {
  scanned: number;
  added: number;
  updated: number;
  /** Encontrados pero excluidos por el usuario: intactos. */
  excluded: number;
  /** Ya no están en disco → pasan al histórico. */
  uninstalled: number;
  /** Fuentes cuya ruta no era accesible (disco desconectado); se dejan intactas. */
  skipped: string[];
  /** El usuario la paró a medias; lo escaneado hasta ahí se conserva. */
  cancelada: boolean;
};

/** Integración con TimeTrack (guión T.0_1 / ADR-003). */
export type TimeTrackEstado =
  | { kind: "no_responde" }
  | { kind: "conectado"; profile_id: number; profile: string };
export type TimeTrackConfig = { base: string; dir: string | null };
export type TimeTrackSync = {
  estado: TimeTrackEstado;
  apps: number;
  con_tiempo: number;
  con_arte: number;
};

/** Formatea segundos como "12h 30m" / "45m" / "—". */
export function fmtTiempo(secs: number): string {
  if (!secs || secs <= 0) return "—";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  if (h === 0) return `${m}m`;
  return m > 0 ? `${h}h ${m}m` : `${h}h`;
}

/** Eventos core → UI (guión A §8). Devuelven la función para dejar de escuchar. */
export const events = {
  onScanProgress: (cb: (p: ScanProgress) => void): Promise<UnlistenFn> =>
    listen<ScanProgress>("scan_progress", (e) => cb(e.payload)),
  onScanFinished: (cb: (s: ScanSummary) => void): Promise<UnlistenFn> =>
    listen<ScanSummary>("scan_finished", (e) => cb(e.payload)),
  /**
   * Venció la cuenta atrás del Modo Sofá sin confirmar y el núcleo **ya ha deshecho** el cambio
   * de pantallas. La interfaz solo tiene que enterarse (guión E.0_4 §5.5).
   */
  onSofaRevertido: (cb: () => void): Promise<UnlistenFn> => listen("sofa_revertido", () => cb()),
  /**
   * Gestos del mando leídos por el **núcleo** (guión E.0_4 fase 4). Llegan también con un juego
   * a pantalla completa delante, que es justo cuando la Web Gamepad API no ve nada.
   */
  onMandoXbox: (cb: () => void): Promise<UnlistenFn> => listen("mando_xbox", () => cb()),
  onMandoSofa: (cb: () => void): Promise<UnlistenFn> => listen("mando_sofa", () => cb()),
  /** Progreso del mantenido, `0`–`1`. Un **−1** significa «soltó antes de tiempo». */
  onMandoSofaProgreso: (cb: (p: number) => void): Promise<UnlistenFn> =>
    listen<number>("mando_sofa_progreso", (e) => cb(e.payload)),
  /**
   * **Pulsación corta** del botón reservado al Modo Sofá. El núcleo se queda ese botón entero
   * —porque es quien distingue corta de larga— y avisa al soltar, para que la interfaz dispare
   * la acción que tenga asignada (de fábrica, la guía de botones).
   */
  onMandoSofaCorto: (cb: () => void): Promise<UnlistenFn> =>
    listen("mando_sofa_corto", () => cb()),
  onArtProgress: (cb: (p: ScanProgress) => void): Promise<UnlistenFn> =>
    listen<ScanProgress>("art_progress", (e) => cb(e.payload)),
  onArtFinished: (cb: (s: ArtSummary) => void): Promise<UnlistenFn> =>
    listen<ArtSummary>("art_finished", (e) => cb(e.payload)),
};

/** Envoltorios tipados sobre los comandos Tauri (core ↔ UI). Ver guión A §8. */
export const api = {
  appInfo: () =>
    invoke<{ name: string; version: string; codename: string }>("app_info"),
  /**
   * Añade una fuente **o devuelve la que ya cubría esa carpeta** (guión A.0_6). `ya_existia`
   * distingue los dos casos: la misma ruta nunca entra dos veces, venga escrita como venga.
   */
  sourcesAdd: (kind: string, path: string, defaultCategoryId: number | null) =>
    invoke<{ id: number; ya_existia: boolean }>("sources_add", {
      kind,
      path,
      defaultCategoryId,
    }),
  sourcesList: () => invoke<Source[]>("sources_list"),
  /** Devuelve cuántos juegos se han borrado. */
  sourcesRemove: (id: number, deleteGames: boolean) =>
    invoke<number>("sources_remove", { id, deleteGames }),
  sourcesSetEnabled: (id: number, enabled: boolean) =>
    invoke<void>("sources_set_enabled", { id, enabled }),
  scanRun: (taskId?: string) => invoke<ScanSummary>("scan_run", { taskId: taskId ?? null }),
  /** Pide parar una tarea en curso; termina ordenadamente y conserva lo hecho. */
  taskCancel: (taskId: string) => invoke<void>("task_cancel", { taskId }),
  libraryList: (filter: LibraryFilter) => invoke<GameCard[]>("library_list", { filter }),
  stateCounts: (recentDays: number) => invoke<StateCounts>("state_counts", { recentDays }),
  platformsList: () => invoke<string[]>("platforms_list"),
  originsList: () => invoke<string[]>("origins_list"),
  /** Busca carátulas sin APIs: caché de Steam → icono del exe. */
  /** Fija la carátula a mano; `null` la quita y vuelve a mandar la automática. */
  gameSetCover: (id: number, path: string | null) =>
    invoke<void>("game_set_cover", { id, path }),
  /** Vuelve a buscar la carátula de un solo juego. Devuelve la ruta, o `null`. */
  gameArtRefresh: (id: number) => invoke<string | null>("game_art_refresh", { id }),
  artFetch: (rehacer = false, taskId?: string) =>
    invoke<ArtSummary>("art_fetch", { rehacer, taskId: taskId ?? null }),
  gameSetState: (id: number, newState: GameState) =>
    invoke<void>("game_set_state", { id, newState }),
  /** Marca o desmarca favorito. Es ortogonal al estado. */
  gameSetFavorite: (id: number, favorite: boolean) =>
    invoke<void>("game_set_favorite", { id, favorite }),
  gameGet: (id: number) => invoke<GameDetail | null>("game_get", { id }),
  /**
   * Abre una ventana del explorador en la carpeta del juego, con su ejecutable seleccionado
   * si sigue ahí. Falla con un mensaje que nombra la ruta cuando ya no existe.
   */
  gameOpenDir: (id: number) => invoke<void>("game_open_dir", { id }),
  /** ¿Hay que preguntar "launcher o juego directo"? */
  launchOptions: (id: number, executableId?: number) =>
    invoke<LaunchOptions | null>("launch_options", {
      id,
      executableId: executableId ?? null,
    }),
  gameLaunch: (
    id: number,
    opts: { executableId?: number; mode?: LaunchMode; remember?: boolean } = {},
  ) =>
    invoke<void>("game_launch", {
      id,
      executableId: opts.executableId ?? null,
      mode: opts.mode ?? null,
      remember: opts.remember ?? false,
    }),
  gameUpdate: (
    id: number,
    data: {
      title: string;
      platform: string;
      exePath: string | null;
      exeArgs: string | null;
      clientPath: string | null;
      clientArgs: string | null;
      clientFirst: boolean;
      playersMin: number | null;
      playersMax: number | null;
      /** `null` = preguntar siempre al lanzar. */
      launchMode: LaunchMode | null;
      steamAccount: string | null;
    },
  ) => invoke<void>("game_update", { id, ...data }),
  gameDelete: (id: number) => invoke<void>("game_delete", { id }),
  executableAdd: (gameId: number, label: string, path: string, args: string | null) =>
    invoke<number>("executable_add", { gameId, label, path, args }),
  executableDelete: (id: number) => invoke<void>("executable_delete", { id }),
  categoriesList: () => invoke<Category[]>("categories_list"),
  categoryCreate: (name: string, parentId: number | null) =>
    invoke<number>("category_create", { name, parentId }),
  categoryRename: (id: number, name: string) => invoke<void>("category_rename", { id, name }),
  /** Cuelga una categoría de otra; `null` la deja como eje raíz. */
  categorySetParent: (id: number, parentId: number | null) =>
    invoke<void>("category_set_parent", { id, parentId }),
  /** Sube (-1) o baja (+1) una categoría entre sus hermanas. `false` = ya estaba al borde. */
  categoryMove: (id: number, delta: number) =>
    invoke<boolean>("category_move", { id, delta }),
  /** Añade las categorías por defecto que falten. Devuelve los nombres creados. */
  categoriesAddDefaults: () => invoke<string[]>("categories_add_defaults"),
  categoriesSuggestMoves: () => invoke<Recolocacion[]>("categories_suggest_moves"),
  categoriesApplyMoves: (ids: number[]) =>
    invoke<number>("categories_apply_moves", { ids }),
  categoryDelete: (id: number) => invoke<void>("category_delete", { id }),
  /** Renombra un grupo entero; devuelve cuántas categorías se han movido. */
  /** Categorías asignadas a una fuente entera (F10). */
  sourceCategories: (sourceId: number) => invoke<number[]>("source_categories", { sourceId }),
  /** `retroactivo` la aplica también a lo que ya estaba, incluido el histórico. */
  sourceCategoryAssign: (sourceId: number, categoryId: number, retroactivo: boolean) =>
    invoke<number>("source_category_assign", { sourceId, categoryId, retroactivo }),
  sourceCategoryUnassign: (sourceId: number, categoryId: number, limpiar: boolean) =>
    invoke<number>("source_category_unassign", { sourceId, categoryId, limpiar }),
  /** Confirmación previa al lanzar para los juegos de una categoría (C8). */
  categorySetPrompt: (id: number, prompt: string | null) =>
    invoke<void>("category_set_prompt", { id, prompt }),
  /** Detecta Steam y lo añade como fuente. Devuelve la ruta, o `null` si no lo encuentra. */
  steamDetect: () => invoke<string | null>("steam_detect"),
  /** Mueve el cursor del sistema en relativo (stick derecho del mando). */
  cursorMove: (dx: number, dy: number) => invoke<void>("cursor_move", { dx, dy }),
  /** Pulsa/suelta un botón del ratón; los dos flancos, para poder arrastrar. */
  cursorButton: (left: boolean, pressed: boolean) =>
    invoke<void>("cursor_button", { left, pressed }),
  /**
   * Pantallas conectadas, en píxeles físicos y con su nombre real. Lo resuelve el core con
   * Win32 porque hace falta también con la ventana cerrada (guión E.0_4 §4.1).
   */
  monitorsList: () => invoke<Pantalla[]>("monitors_list"),
  /**
   * Lleva la ventana a pantalla completa en el monitor indicado, o la devuelve. **No toca la
   * configuración de pantallas**: eso es la fase 3 y se preguntará antes (guión E.0_4 fase 2).
   */
  sofaSet: (activo: boolean, monitor: string | null) =>
    invoke<void>("sofa_set", { activo, monitor }),
  /**
   * Aplica el reparto de pantallas: `ninguna` no toca nada, `primaria` hace principal la
   * elegida, `solo` apaga las demás. Arranca una **cuenta atrás de 20 s en el núcleo**: sin
   * `sofaConfirmar()` se deshace sola (guión E.0_4 §5.5).
   */
  sofaAplicar: (reparto: Reparto, monitor: string) =>
    invoke<Pantalla[]>("sofa_aplicar", { reparto, monitor }),
  /** «Sí, se ve bien»: para la cuenta atrás. */
  sofaConfirmar: () => invoke<void>("sofa_confirmar"),
  /** Devuelve las pantallas a como estaban. Idempotente. */
  sofaRestaurar: () => invoke<Pantalla[]>("sofa_restaurar"),
  /**
   * Botón (índice del *Standard Gamepad*) que entra en Modo Sofá al **mantenerlo**. Vive en el
   * core porque lo lee el núcleo, que es quien sondea el mando con la ventana cerrada.
   */
  mandoConfig: () => invoke<MandoConfig>("mando_config"),
  mandoConfigSet: (config: MandoConfig) => invoke<void>("mando_config_set", { config }),
  /** Ajustes que viven en el core (no en localStorage). */
  settingGet: (key: string) => invoke<string | null>("setting_get", { key }),
  settingSet: (key: string, value: string) => invoke<void>("setting_set", { key, value }),
  /**
   * Claves de arte que TimeTrack ya tenga puestas. Solo **lee** su base de datos; guardarlas
   * es decisión del usuario desde Ajustes (guión D.0_3 §4).
   */
  timetrackArtKeys: () =>
    invoke<{ sgdb: string | null; rawg: string | null; origen: string }>("timetrack_art_keys"),
  timetrackConfig: () => invoke<TimeTrackConfig>("timetrack_config"),
  timetrackSetConfig: (cfg: { base?: string; dir?: string }) =>
    invoke<void>("timetrack_set_config", { base: cfg.base ?? null, dir: cfg.dir ?? null }),
  timetrackSync: () => invoke<TimeTrackSync>("timetrack_sync"),
  /**
   * Abre las gráficas de TimeTrack de este juego en una ventana aparte, reutilizando la que ya
   * esté abierta. Falla con un mensaje legible si TimeTrack no responde: el hub **no lo arranca**.
   */
  timetrackOpenApp: (id: number) => invoke<void>("timetrack_open_app", { id }),
  categoryAssign: (gameId: number, categoryId: number) =>
    invoke<void>("category_assign", { gameId, categoryId }),
  categoryUnassign: (gameId: number, categoryId: number) =>
    invoke<void>("category_unassign", { gameId, categoryId }),
};
