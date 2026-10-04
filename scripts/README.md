# scripts/ — Gestión de SofaCervecero

Scripts PowerShell para el ciclo de vida de la app (Windows). Guía completa de build/despliegue en el
[`README`](../README.md) de la raíz del repositorio.

| Script | Qué hace |
|--------|----------|
| `dev.ps1` | Modo desarrollo (hot-reload de UI). Para iterar. |
| `run.ps1` | Lanza el `.exe` release ya compilado (lo compila si falta). Pruebas rápidas. |
| `build.ps1` | Build release + **instalador NSIS** → `dist\`. Con `-NoBundle`, solo el `.exe`. |
| `test.ps1` | Tests del núcleo Rust + build del frontend. |
| `clean.ps1` | Borra artefactos (`-Deep` incluye `node_modules`). |
| `version.ps1 X.Y.Z` | Sincroniza la versión en los 3 manifiestos. |

**Uso** (desde la raíz del repo):
```powershell
.\scripts\dev.ps1
.\scripts\run.ps1
.\scripts\build.ps1            # instalador
.\scripts\build.ps1 -NoBundle  # solo exe
```
Si PowerShell bloquea por política de ejecución:
`powershell -ExecutionPolicy Bypass -File .\scripts\build.ps1`
