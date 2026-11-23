<#
.SYNOPSIS
安装或升级 DevShellTools。
.DESCRIPTION
把模块复制到当前用户的 Windows PowerShell 5.1 和 PowerShell 7 模块目录，
并向对应 Profile 写入 Import-Module DevShellTools。
若脚本已位于 PS5.1 模块目录（Studio 工作区），则跳过对该目录的删除/自复制，避免自毁。
.EXAMPLE
Set-ExecutionPolicy -Scope Process Bypass
.\install.ps1
#>

[CmdletBinding()]
param([switch]$SkipProfile)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

function Get-DstDocuments {
    if (-not [string]::IsNullOrWhiteSpace($env:DST_MY_DOCUMENTS)) {
        return $env:DST_MY_DOCUMENTS.TrimEnd('\', '/')
    }
    return [Environment]::GetFolderPath("MyDocuments")
}

function Get-DstFullPath([string]$Path) {
    return [System.IO.Path]::GetFullPath($Path)
}

function Copy-DstModuleFiles([string]$Source, [string]$Destination) {
    New-Item -ItemType Directory -Path $Destination -Force | Out-Null
    foreach ($name in @("DevShellTools.psd1", "DevShellTools.psm1", "Private", "Public")) {
        $srcPath = Join-Path $Source $name
        if (-not (Test-Path -LiteralPath $srcPath)) { continue }
        $dstPath = Join-Path $Destination $name
        if (Test-Path -LiteralPath $dstPath) {
            Remove-Item -LiteralPath $dstPath -Recurse -Force
        }
        Copy-Item -LiteralPath $srcPath -Destination $dstPath -Recurse -Force
    }
}

function Enable-DstShellModule([string]$ModuleDir) {
    foreach ($pair in @(
        @{ Active = "DevShellTools.psd1"; Disabled = "DevShellTools.psd1.dst-disabled" },
        @{ Active = "DevShellTools.psm1"; Disabled = "DevShellTools.psm1.dst-disabled" }
    )) {
        $active = Join-Path $ModuleDir $pair.Active
        $disabled = Join-Path $ModuleDir $pair.Disabled
        if ((-not (Test-Path -LiteralPath $active)) -and (Test-Path -LiteralPath $disabled)) {
            Rename-Item -LiteralPath $disabled -NewName $pair.Active
            Write-Host "[成功] 已重新启用：$active" -ForegroundColor Green
        }
    }
}

$moduleName = "DevShellTools"
$source = Get-DstFullPath $PSScriptRoot
$documents = Get-DstDocuments

# 若此前软卸载禁用了清单，先恢复（尤其是工作区 == PS5.1 目录时）
Enable-DstShellModule -ModuleDir $source

$targets = @(
    (Get-DstFullPath (Join-Path $documents "WindowsPowerShell\Modules\$moduleName"))
    (Get-DstFullPath (Join-Path $documents "PowerShell\Modules\$moduleName"))
)

foreach ($target in $targets) {
    if ($source -eq $target) {
        Enable-DstShellModule -ModuleDir $target
        Write-Host "[成功] 源目录即模块目录，跳过自复制：$target" -ForegroundColor Green
        continue
    }

    if (Test-Path -LiteralPath $target) {
        # 备份必须放在 Modules 目录外，否则 PS 会递归扫描到其中的 DevShellTools 并继续自动加载
        $backupRoot = Join-Path $documents "DevShellTools-backups"
        New-Item -ItemType Directory -Path $backupRoot -Force | Out-Null
        $backup = Join-Path $backupRoot ("DevShellTools.backup." + (Get-Date -Format 'yyyyMMddHHmmss') + "." + [Guid]::NewGuid().ToString('N').Substring(0, 8))
        Copy-Item -LiteralPath $target -Destination $backup -Recurse -Force
        Write-Host "[备份] $backup" -ForegroundColor Yellow
        Remove-Item -LiteralPath $target -Recurse -Force
    }

    Copy-DstModuleFiles -Source $source -Destination $target
    Write-Host "[成功] 已安装：$target" -ForegroundColor Green
}

if (-not $SkipProfile) {
    $docsRoots = @($documents)
    $userDocs = Join-Path $env:USERPROFILE "Documents"
    if ($userDocs -and ((Get-DstFullPath $userDocs) -ne (Get-DstFullPath $documents))) {
        $docsRoots += $userDocs
    }
    $profiles = foreach ($docsRoot in $docsRoots) {
        (Join-Path $docsRoot "WindowsPowerShell\Microsoft.PowerShell_profile.ps1")
        (Join-Path $docsRoot "PowerShell\Microsoft.PowerShell_profile.ps1")
    }

    foreach ($profilePath in ($profiles | Select-Object -Unique)) {
        $dir = Split-Path -Parent $profilePath
        New-Item -ItemType Directory -Path $dir -Force | Out-Null
        if (-not (Test-Path -LiteralPath $profilePath)) {
            New-Item -ItemType File -Path $profilePath -Force | Out-Null
        }

        $content = Get-Content -LiteralPath $profilePath -Raw -ErrorAction SilentlyContinue
        if ($null -eq $content) { $content = "" }

        if ($content -notmatch "(?m)^\s*Import-Module\s+DevShellTools\b") {
            Add-Content -LiteralPath $profilePath -Value ""
            Add-Content -LiteralPath $profilePath -Value "# DevShellTools"
            Add-Content -LiteralPath $profilePath -Value "Import-Module DevShellTools -Force -ErrorAction SilentlyContinue"
            Write-Host "[成功] 已更新 Profile：$profilePath" -ForegroundColor Green
        }
    }
}

$manifest = Join-Path $source "DevShellTools.psd1"
if (-not (Test-Path -LiteralPath $manifest)) {
    foreach ($target in $targets) {
        $candidate = Join-Path $target "DevShellTools.psd1"
        if (Test-Path -LiteralPath $candidate) {
            $manifest = $candidate
            break
        }
    }
}

Remove-Module DevShellTools -Force -ErrorAction SilentlyContinue
if (-not (Test-Path -LiteralPath $manifest)) {
    throw "未找到 DevShellTools.psd1，无法完成 Import-Module。"
}
Import-Module $manifest -Force

Write-Host ""
Write-Host "[成功] DevShellTools 1.0.5 安装完成。" -ForegroundColor Green
Write-Host "首次使用：dsh"
Write-Host "直接分类：dsh files | powershell | proxy | git | network"
Write-Host ""
Write-Host "[兼容提醒] 如果已安装旧 ProxyCtl，lpr 可能存在同名命令。" -ForegroundColor Yellow
Write-Host "当前模块已导出统一版 lpr；可使用 which lpr 查看实际来源。" -ForegroundColor Yellow
