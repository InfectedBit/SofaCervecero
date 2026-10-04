<#
.SYNOPSIS  Compila SofaCervecero en release.
.DESCRIPTION
  Sin parametros: build completo + instalador NSIS (setup.exe) y lo copia a <raiz>\dist\.
  -NoBundle: solo el .exe portable (mas rapido, para pruebas).
#>
param([switch]$NoBundle)
$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$app  = Join-Path $root "app"
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

# SofaCervecero es un servicio residente: al cerrar la ventana el core sigue vivo en la
# bandeja, **sin nada visible**. Y un proceso vivo mantiene su .exe bloqueado, asi que el
# enlazador fallaba con "Acceso denegado (os error 5)" y la build no llegaba a rehacerse:
# despues parecia que la app "habia dejado de funcionar", cuando lo que corria era el
# binario viejo. Antes de compilar, se cierra.
$vivos = Get-Process -Name SofaCervecero -EA SilentlyContinue
if ($vivos) {
  Write-Host "Cerrando $($vivos.Count) instancia(s) residente(s) para poder reemplazar el .exe..." -ForegroundColor Yellow
  foreach ($p in $vivos) {
    if (-not $p.CloseMainWindow()) { }
    if (-not $p.WaitForExit(3000)) { Stop-Process -Id $p.Id -Force -EA SilentlyContinue }
  }
  Start-Sleep -Milliseconds 400
}

Push-Location $app
try {
  if ($NoBundle) {
    # Se llama al CLI directamente: con `npm run tauri build -- --no-bundle` el `--` no
    # llegaba a tauri y acababa generando el instalador NSIS igualmente (2 min de mas).
    npx --no-install tauri build --no-bundle
    if ($LASTEXITCODE -ne 0) { throw "Build fallo" }
    Write-Host "`nEXE portable:" -ForegroundColor Green
    Write-Host "  $app\src-tauri\target\release\SofaCervecero.exe"
  } else {
    npm run tauri build
    if ($LASTEXITCODE -ne 0) { throw "Build fallo" }
    $dist = Join-Path $root "dist"
    New-Item -ItemType Directory -Force $dist | Out-Null
    $arts = Get-ChildItem "$app\src-tauri\target\release\bundle" -Recurse -Include *-setup.exe, *.msi -ErrorAction SilentlyContinue
    foreach ($f in $arts) {
      Copy-Item $f.FullName $dist -Force
      Write-Host "Instalador -> $($f.Name)" -ForegroundColor Green
    }
    Write-Host "`nArtefactos de distribucion en: $dist"
  }
} finally { Pop-Location }
