<script lang="ts">
  import { STATE_LABEL, type Category, type LibraryFilter } from "../lib/api";
  import { ORIGEN_LABEL, buscarOrden, textoOrden, tiempoActivo } from "../lib/filtros";
  import { ejesDisponibles } from "../lib/secciones";
  import { prefs } from "../lib/prefs.svelte";

  let {
    filter,
    categories,
    total,
    onquitar,
    onlimpiar,
    onvista,
  }: {
    filter: LibraryFilter;
    categories: Category[];
    total: number;
    /** Quita un solo criterio. */
    onquitar: (clave: keyof LibraryFilter) => void;
    onlimpiar: () => void;
    /** Devuelve orden o secciones a su valor por defecto. */
    onvista: (que: "orden" | "seccion") => void;
  } = $props();

  /**
   * Un chip por criterio activo. Es la respuesta al fallo reportado: combinar estado y
   * categoría funcionaba, pero no había forma de **ver** que los dos estaban puestos.
   */
  let chips = $derived.by(() => {
    const out: { clave: keyof LibraryFilter; texto: string }[] = [];
    if (filter.state) out.push({ clave: "state", texto: STATE_LABEL[filter.state] });
    if (filter.category_id != null) {
      const c = categories.find((x) => x.id === filter.category_id);
      out.push({ clave: "category_id", texto: c ? c.name : "Categoría" });
    }
    if (filter.query) out.push({ clave: "query", texto: `«${filter.query}»` });
    if (filter.origin) out.push({ clave: "origin", texto: ORIGEN_LABEL[filter.origin] ?? filter.origin });
    if (filter.platform) out.push({ clave: "platform", texto: filter.platform });
    if (filter.players) out.push({ clave: "players", texto: `${filter.players}+ jugadores` });
    // Un solo chip para toda la dimensión del tiempo: quitarlo limpia los tres campos
    // (lo hace `quitarFiltro`), porque media banda no significa nada.
    const t = tiempoActivo({
      played: filter.played ?? null,
      min_hours: filter.min_hours ?? null,
      max_hours: filter.max_hours ?? null,
    });
    if (t) out.push({ clave: "played", texto: `⏱ ${t.chip}` });
    if (filter.favorite) out.push({ clave: "favorite", texto: "★ Favoritos" });
    if (filter.added_days) {
      out.push({ clave: "added_days", texto: `🆕 Últimos ${filter.added_days} días` });
    }
    if (filter.needs_exe) out.push({ clave: "needs_exe", texto: "Sin ejecutable" });
    return out;
  });

  /**
   * Orden y secciones no filtran nada, pero cambian lo que ves: si no se enseñan, «¿por qué
   * está esto así?» no tiene respuesta a la vista. Van aparte y solo cuando no son los de por
   * defecto (guión F.0_5 §4).
   */
  /** Dirección que se está aplicando de verdad: la pedida, o la natural del criterio. */
  let dirEfectiva = $derived(filter.desc ?? buscarOrden(filter.sort).desc);
  let ordenActivo = $derived((filter.sort ?? "title") !== "title" || dirEfectiva);
  let seccionActiva = $derived(prefs.seccion !== "none");
  /** «Nº Jugadores › TAGS» cuando hay dos ejes anidados. */
  let textoSeccion = $derived.by(() => {
    const ejes = ejesDisponibles(categories);
    const n = (id: string) => ejes.find((e) => e.id === id)?.texto ?? "";
    const a = n(prefs.seccion);
    const b = prefs.seccion2 !== "none" ? n(prefs.seccion2) : "";
    return b ? `${a} › ${b}` : a;
  });

  let hayAlgo = $derived(chips.length > 0 || ordenActivo || seccionActiva);
</script>

{#if hayAlgo}
  <div class="filtros-activos">
    <span class="fa-total">{total} {total === 1 ? "resultado" : "resultados"}</span>
    {#each chips as c (c.clave)}
      <button class="chip" title="Quitar este filtro" onclick={() => onquitar(c.clave)}>
        {c.texto}<span class="chip-x">✕</span>
      </button>
    {/each}
    {#if ordenActivo}
      <button class="chip vista" title="Volver al orden por título" onclick={() => onvista("orden")}>
        ⇅ {textoOrden(filter.sort, dirEfectiva)}<span class="chip-x">✕</span>
      </button>
    {/if}
    {#if seccionActiva}
      <button class="chip vista" title="Quitar las secciones" onclick={() => onvista("seccion")}>
        ▦ {textoSeccion}<span class="chip-x">✕</span>
      </button>
    {/if}
    {#if chips.length > 0}
      <button class="btn sm" onclick={onlimpiar}>Limpiar todo</button>
    {/if}
  </div>
{/if}
