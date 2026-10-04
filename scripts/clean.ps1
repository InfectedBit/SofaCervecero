# clean.ps1 — Limpia artefactos de build. -Deep tambien borra node_modules.
param([switch]$Deep)
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
# Igual que en build.ps1: una instancia residente en la bandeja bloquea el .exe y borrar
# `target` fallaria a medias, dejando un arbol de build corrupto.
Get-Process -Name SofaCervecero -EA SilentlyContinue | ForEach-Object {
  Write-Host "Cerrando instancia residente (PID $($_.Id))..." -ForegroundColor Yellow
  Stop-Process -Id $_.Id -Force -EA SilentlyContinue
}
Remove-Item (Join-Path $root "app\src-tauri\target") -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $root "app\dist") -Recurse -Force -ErrorAction SilentlyContinue
Remove-Item (Join-Path $root "dist") -Recurse -Force -ErrorAction SilentlyContinue
if ($Deep) { Remove-Item (Join-Path $root "app\node_modules") -Recurse -Force -ErrorAction SilentlyContinue }
Write-Host "Limpieza completada." -ForegroundColor Green
