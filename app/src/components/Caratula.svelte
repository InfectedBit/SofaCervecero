<script lang="ts">
  import { coverSrc, urlCaratulaSteam } from "../lib/api";
  import { prefs } from "../lib/prefs.svelte";

  /**
   * Carátula con **cadena de respaldo real** (guión D.0_2 §3).
   *
   * Antes era un `background-image` de CSS: si la imagen no cargaba, no había forma de
   * enterarse ni de probar otra cosa — se quedaba el hueco en gris y punto. Con un `<img>`
   * y su `onerror` se pasa al siguiente candidato, y solo al agotarlos aparece la inicial.
   */
  let {
    titulo,
    cover,
    storeId = null,
  }: {
    titulo: string;
    /** Ruta en disco (carátula del usuario o arte resuelta). */
    cover: string | null;
    /** Appid de Steam, para el último salto por CDN. */
    storeId?: string | null;
  } = $props();

  let candidatos = $derived(
    [
      coverSrc(cover),
      prefs.artSteamCdn && storeId ? urlCaratulaSteam(storeId) : null,
      // Si el póster vertical no existe en el CDN, el banner casi siempre sí.
      prefs.artSteamCdn && storeId
        ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${storeId}/header.jpg`
        : null,
    ].filter((x): x is string => !!x),
  );

  let i = $state(0);
  // Al cambiar de juego (o al activar/desactivar el CDN) se vuelve a empezar por el primero.
  $effect(() => {
    void candidatos;
    i = 0;
  });

  let src = $derived(candidatos[i] ?? null);
</script>

{#if src}
  <img
    class="cover-img"
    {src}
    alt=""
    loading="lazy"
    onerror={() => {
      if (i < candidatos.length - 1) i += 1;
      else i = candidatos.length; // agotados: se cae al marcador de posición
    }}
  />
{:else}
  <span class="ph">{titulo.charAt(0)}</span>
{/if}
