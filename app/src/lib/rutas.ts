/**
 * Rutas y selectores de fichero (guión A.0_3, punto A12).
 *
 * El explorador debe abrirse **donde estás**: al buscar el ejecutable de un juego instalado en
 * `C:\Games\X`, tiene que aparecer en `C:\Games\X`, no en la última carpeta que se usó.
 */
import { open } from "@tauri-apps/plugin-dialog";

/** Separadores de ruta: Windows acepta los dos. */
const SEPARADOR = /[\\/]/;

/** Carpeta que contiene a esa ruta. Si ya parece una carpeta, se devuelve tal cual. */
export function carpetaDe(ruta: string | null | undefined): string | undefined {
  if (!ruta) return undefined;
  const limpia = ruta.replace(/[\\/]+$/, "");
  if (!limpia) return undefined;
  // Si la última parte tiene extensión, es un fichero: nos quedamos con su carpeta.
  const hoja = limpia.split(SEPARADOR).pop() ?? "";
  if (!hoja.includes(".")) return limpia;
  const corte = Math.max(limpia.lastIndexOf("\\"), limpia.lastIndexOf("/"));
  return corte > 0 ? limpia.slice(0, corte) : undefined;
}

/**
 * Primer candidato utilizable de una lista, en orden de preferencia: normalmente la ruta del
 * propio campo y, detrás, el directorio de instalación del juego.
 */
export function contexto(...candidatos: (string | null | undefined)[]): string | undefined {
  for (const c of candidatos) {
    const d = carpetaDe(c);
    if (d) return d;
  }
  return undefined;
}

/** Selector de ejecutable, abierto en el directorio del contexto. */
export async function elegirExe(...desde: (string | null | undefined)[]): Promise<string | null> {
  const sel = await open({
    multiple: false,
    defaultPath: contexto(...desde),
    filters: [{ name: "Ejecutable", extensions: ["exe"] }],
  });
  return typeof sel === "string" ? sel : null;
}

/** Selector de imagen, abierto en el directorio del contexto. */
export async function elegirImagen(
  ...desde: (string | null | undefined)[]
): Promise<string | null> {
  const sel = await open({
    multiple: false,
    defaultPath: contexto(...desde),
    filters: [{ name: "Imagen", extensions: ["png", "jpg", "jpeg", "webp", "bmp", "ico"] }],
  });
  return typeof sel === "string" ? sel : null;
}

/** Selector de carpeta, abierto en el directorio del contexto. */
export async function elegirCarpeta(
  ...desde: (string | null | undefined)[]
): Promise<string | null> {
  const sel = await open({ directory: true, multiple: false, defaultPath: contexto(...desde) });
  return typeof sel === "string" ? sel : null;
}
