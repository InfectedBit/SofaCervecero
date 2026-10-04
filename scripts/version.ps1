<# version.ps1 — Actualiza la version en tauri.conf.json, package.json y Cargo.toml.
   Uso:  .\scripts\version.ps1 0.2.0
#>
param([Parameter(Mandatory)][string]$Version)
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw "Version debe ser X.Y.Z (p.ej. 0.2.0)" }
$app = (Resolve-Path (Join-Path $PSScriptRoot "..\app")).Path

function Bump($file, $pattern) {
  $c = [System.IO.File]::ReadAllText($file)
  $c = [regex]::Replace($c, $pattern, ('${1}' + $Version + '${2}'))
  [System.IO.File]::WriteAllText($file, $c)   # UTF-8 sin BOM
}

Bump (Join-Path $app "src-tauri\tauri.conf.json") '("version":\s*")[^"]+(")'
Bump (Join-Path $app "package.json")              '("version":\s*")[^"]+(")'
Bump (Join-Path $app "src-tauri\Cargo.toml")      '(?m)^(version\s*=\s*")[^"]+(")'

Write-Host "Version -> $Version (tauri.conf.json, package.json, Cargo.toml)" -ForegroundColor Green
Write-Host "Recuerda: recompilar para regenerar Cargo.lock con la nueva version."
