param([switch]$SkipDependencyAudit)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot

function Invoke-Gate([string]$Name, [scriptblock]$Command) {
    Write-Host ""
    Write-Host "== $Name ==" -ForegroundColor Cyan
    & $Command
    if ($LASTEXITCODE -ne 0) {
        throw "Gate failed: $Name (exit $LASTEXITCODE)"
    }
}

Push-Location (Join-Path $repoRoot "devshelltools-studio/src-tauri")
try {
    $env:CARGO_NET_OFFLINE = "true"
    Invoke-Gate "cargo fmt" { cargo fmt --all -- --check }
    Invoke-Gate "cargo clippy" { cargo clippy --offline --all-targets -- -D warnings }
    Invoke-Gate "rust tests" { cargo test --offline --tests -- --test-threads=1 }
}
finally {
    Pop-Location
}

Push-Location (Join-Path $repoRoot "devshelltools-studio")
try {
    Invoke-Gate "frontend tests" { pnpm exec vitest run }
    Invoke-Gate "frontend build" { pnpm build }
    Invoke-Gate "security configuration self-test" { node scripts/check-security-config.mjs --self-test }
    Invoke-Gate "security configuration" { node scripts/check-security-config.mjs }
}
finally {
    Pop-Location
}

Invoke-Gate "template parity" {
    & pwsh -NoProfile -File (Join-Path $repoRoot "scripts/check-template-parity.ps1")
}
Invoke-Gate "template parity self-test" {
    & pwsh -NoProfile -File (Join-Path $repoRoot "scripts/check-template-parity.ps1") -SelfTest
}
if (-not $SkipDependencyAudit) {
    Invoke-Gate "dependency audit record" {
        & pwsh -NoProfile -File (Join-Path $repoRoot "scripts/check-dependencies.ps1")
    }
}
Invoke-Gate "git whitespace" {
    Push-Location $repoRoot
    try { git diff --check }
    finally { Pop-Location }
}

Write-Host ""
Write-Host "All verification gates passed." -ForegroundColor Green
