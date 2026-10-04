# SofaCervecero — app (Tauri v2)

Hub launcher para PC (Windows). Codename **SofaCervecero**. Stack: **Rust (core) + WebView2 (UI)** vía
Tauri v2. Descripción general, capturas y guía de uso en el [`README`](../README.md) de la raíz del
repositorio.

## Estructura
- `src/` — frontend (TypeScript vanilla + Vite; framework por decidir, ADR-002).
- `src-tauri/` — core Rust. Módulos de arquitectura (§5 del plan):
  `service` · `library` · `sources` · `launcher` · `timetrack` · `metadata`.

## Requisitos
- Rust (`stable-x86_64-pc-windows-msvc`) · Node 18+ · WebView2 Runtime (ya en Win11).

## Comandos
```bash
npm install            # dependencias frontend + Tauri CLI
npm run tauri dev      # desarrollo (hot reload)
npm run tauri build    # release + instalador
npm run tauri build -- --no-bundle   # release sin instalador (usado para medir RAM en M0)
```

## Estado
En desarrollo activo. Ver el [`README`](../README.md) de la raíz para el estado funcional actual.
