$env:FLIT_EPHEMERAL = "1"
if (-not $env:FLIT_IDLE_SECS) { $env:FLIT_IDLE_SECS = "1800" }
if (-not $env:FLIT_ADDR) { $env:FLIT_ADDR = "127.0.0.1:7777" }
$root = Split-Path -Parent $PSScriptRoot
$bin = Join-Path $root "target/release/flit-server.exe"
if (Test-Path $bin) { & $bin } else { cargo run --release --manifest-path (Join-Path $root "Cargo.toml") }