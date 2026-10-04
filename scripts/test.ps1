# test.ps1 — Verificacion: tests del nucleo Rust + build del frontend.
$ErrorActionPreference = "Stop"
$app = Join-Path $PSScriptRoot "..\app"
$env:Path = "$env:USERPROFILE\.cargo\bin;$env:Path"

Push-Location (Join-Path $app "src-tauri")
try { cargo test; if ($LASTEXITCODE -ne 0) { throw "cargo test fallo" } } finally { Pop-Location }

Push-Location $app
try { npm run build; if ($LASTEXITCODE -ne 0) { throw "build frontend fallo" } } finally { Pop-Location }

Write-Host "OK: tests del nucleo + build del frontend" -ForegroundColor Green
