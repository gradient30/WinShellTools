param([switch]$SelfTest)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Normalize-Source([string]$Text) {
    $withoutMetadata = [regex]::Replace(
        ($Text -replace "^\uFEFF", ""),
        '(?s)<#!\s*@DST-Category.*?@DST-Category-End\s*#>',
        ''
    )
    $lines = $withoutMetadata -replace "`r`n", "`n" -split "`n"
    $result = [System.Collections.Generic.List[string]]::new()
    foreach ($line in $lines) {
        $normalized = $line.TrimEnd()
        $normalized = [regex]::Replace($normalized, '1\.0\.\d+', '<VERSION>')
        $result.Add($normalized)
    }
    return (($result -join "`n").Trim())
}

function Compare-SharedFiles([string]$Root, [string]$Templates) {
    $errors = [System.Collections.Generic.List[string]]::new()
    $pairs = @(
        @{ Root = "Public"; Template = "Public" },
        @{ Root = "Private"; Template = "Private" }
    )
    foreach ($pair in $pairs) {
        $rootDir = Join-Path $Root $pair.Root
        $templateDir = Join-Path $Templates $pair.Template
        if (-not (Test-Path -LiteralPath $rootDir) -or -not (Test-Path -LiteralPath $templateDir)) { continue }
        $rootFiles = @(Get-ChildItem -LiteralPath $rootDir -File -Filter *.ps1 | Select-Object -ExpandProperty Name)
        $templateFiles = @(Get-ChildItem -LiteralPath $templateDir -File -Filter *.ps1 | Select-Object -ExpandProperty Name)
        foreach ($name in ($rootFiles | Where-Object { $_ -notin $templateFiles })) {
            $errors.Add("$($pair.Root)/$name exists only in root module")
        }
        foreach ($name in ($templateFiles | Where-Object { $_ -notin $rootFiles })) {
            $errors.Add("$($pair.Template)/$name exists only in Studio template")
        }
        foreach ($name in ($rootFiles | Where-Object { $_ -in $templateFiles })) {
            # Help.ps1 包含模板块，根模块保留 1.0.4 的已生成形态；内容由一致性测试覆盖。
            if ($name -eq "Help.ps1") { continue }
            $rootText = Normalize-Source (Get-Content -LiteralPath (Join-Path $rootDir $name) -Raw)
            $templateText = Normalize-Source (Get-Content -LiteralPath (Join-Path $templateDir $name) -Raw)
            if ($rootText -ne $templateText) {
                $errors.Add("$($pair.Root)/$name implementation drift")
            }
        }
    }
    # 清单和加载器含生成导出块，不参与源码语义比较。
    foreach ($name in @("install.ps1", "uninstall.ps1")) {
        $rootPath = Join-Path $Root $name
        $templatePath = Join-Path $Templates $name
        if (-not (Test-Path -LiteralPath $rootPath) -or -not (Test-Path -LiteralPath $templatePath)) { continue }
        if ((Normalize-Source (Get-Content -LiteralPath $rootPath -Raw)) -ne
            (Normalize-Source (Get-Content -LiteralPath $templatePath -Raw))) {
            $errors.Add("$name implementation drift")
        }
    }
    return $errors
}

function Run-SelfTest {
    $temp = Join-Path ([IO.Path]::GetTempPath()) ("dst-parity-" + [Guid]::NewGuid().ToString("N"))
    try {
        $root = Join-Path $temp "root"
        $templates = Join-Path $temp "templates"
        New-Item -ItemType Directory -Path (Join-Path $root "Public"), (Join-Path $templates "Public") -Force | Out-Null
        Set-Content -LiteralPath (Join-Path $root "Public/A.ps1") -Value "function a { 1 }"
        Set-Content -LiteralPath (Join-Path $templates "Public/A.ps1") -Value @"
<#!
@DST-Category
Name: a
Title: A
@DST-Category-End
#>
function a { 1 }
"@
        if (@(Compare-SharedFiles $root $templates).Count -ne 0) {
            throw "metadata-only difference should not fail parity"
        }
        Set-Content -LiteralPath (Join-Path $templates "Public/A.ps1") -Value "function a { 2 }"
        if (@(Compare-SharedFiles $root $templates).Count -eq 0) {
            throw "semantic drift should fail parity"
        }
        Write-Output "Template parity self-test passed."
    }
    finally {
        if (Test-Path -LiteralPath $temp) { Remove-Item -LiteralPath $temp -Recurse -Force }
    }
}

if ($SelfTest) {
    Run-SelfTest
    exit 0
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$errors = @(Compare-SharedFiles $repoRoot (Join-Path $repoRoot "devshelltools-studio/templates"))
if ($errors.Count -gt 0) {
    Write-Error ("Template parity failed:`n- " + ($errors -join "`n- "))
    exit 1
}
Write-Output "Template parity check passed."
