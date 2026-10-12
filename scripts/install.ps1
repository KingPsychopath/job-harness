[CmdletBinding()]
param([string]$Prefix = (Join-Path $HOME ".local"))

$ErrorActionPreference = "Stop"
$HarnessRepo = Split-Path -Parent $PSScriptRoot
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "Install Rust from https://rustup.rs, then run this script again."
}
if (-not (Test-Path (Join-Path $HarnessRepo "Cargo.lock"))) {
    throw "Use a complete Job Harness checkout with Cargo.lock."
}
& cargo install --locked --path (Join-Path $HarnessRepo "crates/harness-cli") --root $Prefix
if ($LASTEXITCODE -ne 0) { throw "Native installation failed; existing workspaces were not modified." }
Write-Host ("Installed: " + (Join-Path $Prefix "bin/job-harness.exe"))
Write-Host "Add its bin directory to PATH, then run job-harness --help."
