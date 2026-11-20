param(
    [switch]$AllowNetwork,
    [switch]$RequireTools,
    [string]$ReportPath = "devshelltools-studio/src-tauri/target/dependency-audit.md"
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"
$repoRoot = Split-Path -Parent $PSScriptRoot
$lines = [System.Collections.Generic.List[string]]::new()
$failed = $false

$lines.Add("# Dependency Audit")
$lines.Add("")
$lines.Add("Generated: $((Get-Date).ToString('yyyy-MM-dd HH:mm:ss zzz'))")
$lines.Add("")

$cargoAudit = Get-Command cargo-audit -ErrorAction SilentlyContinue
if ($cargoAudit) {
    $version = (& cargo audit --version) 2>&1 | Out-String
    $lines.Add("## cargo-audit")
    $lines.Add("")
    $lines.Add("Tool: $($version.Trim())")
    if ($AllowNetwork) {
        Push-Location (Join-Path $repoRoot "devshelltools-studio/src-tauri")
        try {
            $output = (& cargo audit) 2>&1 | Out-String
            if ($LASTEXITCODE -ne 0) { $failed = $true }
            $lines.Add("")
            $lines.Add('```text')
            $lines.Add($output.Trim())
            $lines.Add('```')
        }
        finally { Pop-Location }
    }
    else {
        $lines.Add("")
        $lines.Add("Network disabled; run again with ``-AllowNetwork`` for advisory lookup.")
    }
}
else {
    $lines.Add("## cargo-audit")
    $lines.Add("")
    $lines.Add("Unavailable: cargo-audit is not installed.")
    if ($RequireTools) { $failed = $true }
}

$pnpm = Get-Command pnpm -ErrorAction SilentlyContinue
if ($pnpm -and $AllowNetwork) {
    Push-Location (Join-Path $repoRoot "devshelltools-studio")
    try {
        $output = (& pnpm audit --audit-level high) 2>&1 | Out-String
        if ($LASTEXITCODE -ne 0) { $failed = $true }
        $lines.Add("")
        $lines.Add("## pnpm audit")
        $lines.Add("")
        $lines.Add('```text')
        $lines.Add($output.Trim())
        $lines.Add('```')
    }
    finally { Pop-Location }
}
else {
    $lines.Add("")
    $lines.Add("## pnpm audit")
    $lines.Add("")
    $lines.Add("Skipped: network disabled or pnpm unavailable.")
}

$lines.Add("")
$lines.Add("- Lockfiles present: cargo and pnpm")
$lines.Add("- Offline build remains the default")
if ($failed) { $lines.Add("- Result: FAILED or required audit tool unavailable") }
else { $lines.Add("- Result: no blocking result; see tool limitations above") }

$report = Join-Path $repoRoot $ReportPath
New-Item -ItemType Directory -Path (Split-Path -Parent $report) -Force | Out-Null
$content = ($lines -join "`n") + "`n"
[System.IO.File]::WriteAllText(
    $report,
    $content,
    [System.Text.UTF8Encoding]::new($false)
)
Write-Output "Dependency audit written to $report"
if ($failed) { exit 1 }
