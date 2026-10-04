<#
.SYNOPSIS  Ejecuta SofaCervecero, recompilando si el .exe se ha quedado atras.
.DESCRIPTION
  Antes solo miraba si el .exe existia, asi que tras tocar codigo lanzaba **la build vieja**
  sin avisar: parecia que los cambios no se aplicaban, o que la app "habia dejado de
  funcionar". Ahora compara la fecha del .exe con la del codigo y recompila si hace falta.
  -Force  : recompila siempre.
  -NoBuild: lanza lo que haya, sin comprobar nada (arranque rapido).
#>
param([switch]$Force, [switch]$NoBuild)
$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$app  = Join-Path $root "app"
$exe  = Join-Path $app "src-tauri\target\release\SofaCervecero.exe"

# Todo lo que, al cambiar, obliga a recompilar. Ojo con `src-tauri\src`: el frontend se
# **incrusta** en el binario, asi que tocar la UI tambien exige rehacer el .exe.
$fuentes = @(
  (Join-Path $app "src"),
  (Join-Path $app "src-tauri\src"),
  (Join-Path $app "src-tauri\Cargo.toml"),
  (Join-Path $app "src-tauri\tauri.conf.json"),
  (Join-Path $app "package.json")
)

function Necesita-Build {
  if (-not (Test-Path $exe)) { return "no hay build release" }
  $fechaExe = (Get-Item $exe).LastWriteTime
  foreach ($f in $fuentes) {
    if (-not (Test-Path $f)) { continue }
    $ultimo = Get-ChildItem $f -Recurse -File -EA SilentlyContinue |
              Sort-Object LastWriteTime -Descending | Select-Object -First 1
    if ($null -eq $ultimo) { $ultimo = Get-Item $f }
    if ($ultimo.LastWriteTime -gt $fechaExe) { return "cambio en $($ultimo.Name)" }
  }
  return $null
}

if (-not $NoBuild) {
  $motivo = if ($Force) { "-Force" } else { Necesita-Build }
  if ($motivo) {
    Write-Host "Recompilando ($motivo)..." -ForegroundColor Yellow
    & (Join-Path $PSScriptRoot "build.ps1") -NoBundle
  }
}
if (-not (Test-Path $exe)) { throw "No existe $exe" }

# Con la guardia de instancia unica, volver a lanzarlo NO abre otro core: le dice al que ya
# esta en la bandeja que muestre el hub.
$vivo = Get-Process -Name SofaCervecero -EA SilentlyContinue
if ($vivo) { Write-Host "Ya habia una instancia; se le pide que abra la ventana." -ForegroundColor Cyan }
else        { Write-Host "Lanzando SofaCervecero..." -ForegroundColor Green }
Start-Process $exe
