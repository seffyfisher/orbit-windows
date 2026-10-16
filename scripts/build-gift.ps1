$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
cargo build --locked --release
if ($LASTEXITCODE -ne 0) { throw 'App build failed' }
$env:ORBIT_PAYLOAD = (Resolve-Path 'target/release/orbit.exe').Path
cargo test --manifest-path gift-installer/Cargo.toml --locked
if ($LASTEXITCODE -ne 0) { throw 'Gift gesture tests failed' }
cargo build --manifest-path gift-installer/Cargo.toml --locked --release
if ($LASTEXITCODE -ne 0) { throw 'Gift build failed' }
New-Item -ItemType Directory -Force dist | Out-Null
Copy-Item gift-installer/target/release/orbit-gift.exe dist/Orbit-Gift-0.1.0-x64.exe
Write-Host 'Built dist/Orbit-Gift-0.1.0-x64.exe. Interactive Windows acceptance remains necessary.'
