$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
cargo fmt --all --check
if ($LASTEXITCODE -ne 0) { throw 'Formatting failed' }
cargo test --locked --lib
if ($LASTEXITCODE -ne 0) { throw 'Core tests failed' }
cargo clippy --locked --all-targets -- -D warnings
if ($LASTEXITCODE -ne 0) { throw 'Static checks failed' }
cargo build --locked --release
if ($LASTEXITCODE -ne 0) { throw 'Build failed' }
Write-Host 'Build checks passed. Interactive Windows checks are documented in WINDOWS-QA.md.'
