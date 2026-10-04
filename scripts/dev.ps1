# dev.ps1 — Lanza SofaCervecero en modo desarrollo (hot-reload de UI; recompila Rust al cambiar).
$ErrorActionPreference = "Stop"
$app = Join-Path $PSScriptRoot "..\app"
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"
Push-Location $app
try { npm run tauri dev } finally { Pop-Location }
