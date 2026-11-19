
function isadmin {
<#
.SYNOPSIS
检查当前 PowerShell 是否具有管理员权限。
.EXAMPLE
isadmin
#>
    [CmdletBinding()]
    param()
    if (Test-DstAdministrator) {
        Write-DstOk "当前 PowerShell 已具有管理员权限。"
        return $true
    }
    Write-DstWarn "当前 PowerShell 不是管理员会话。"
    return $false
}

function super {
<#
.SYNOPSIS
打开新的管理员 PowerShell；不会修改当前窗口权限。
.EXAMPLE
super
.EXAMPLE
super -CurrentDirectory
#>
    [CmdletBinding()]
    param([switch]$CurrentDirectory)
    if (-not (Test-DstWindows)) { throw "super 仅支持 Windows。" }
    if (Test-DstAdministrator) {
        Write-DstInfo "当前 PowerShell 已经是管理员会话。"
        return
    }
    $exe = (Get-Process -Id $PID).Path
    $argList = @()
    if ($CurrentDirectory) {
        $escaped = (Get-Location).Path.Replace("'","''")
        $argList += @("-NoExit","-Command","Set-Location -LiteralPath '$escaped'")
    }
    Start-Process -FilePath $exe -Verb RunAs -ArgumentList $argList
    Write-DstOk "已请求打开管理员 PowerShell。"
}

function psb {
<#
.SYNOPSIS
仅在当前 PowerShell 进程临时允许执行脚本。
.DESCRIPTION
等价于 Set-ExecutionPolicy -Scope Process Bypass -Force；这不是管理员提权。
.EXAMPLE
psb
#>
    [CmdletBinding()]
    param()
    Set-ExecutionPolicy -Scope Process Bypass -Force
    Write-DstOk "当前 PowerShell 进程的执行策略已设为 Bypass。"
    Write-DstWarn "这不代表获得管理员权限，关闭窗口后自动失效。"
}

function reload {
<#
.SYNOPSIS
重新加载当前 PowerShell Profile（含 Import-Module DevShellTools -Force）。
.DESCRIPTION
Profile 内的 Import-Module DevShellTools -Force 会卸载并重载本模块（进程级生效，
命令改动即刻可用）。重载期间本函数所属模块被替换，因此本函数只使用内置
Write-Host 输出，不得调用 Write-Dst* 等模块内部函数。Profile 中的自定义
函数/别名在函数作用域执行后不会保留（PowerShell 限制），如需完整生效请
直接执行：. $PROFILE
.EXAMPLE
reload
#>
    [CmdletBinding()]
    param()
    if (-not (Test-Path -LiteralPath $PROFILE)) {
        Write-Host "[警告] Profile 不存在：$PROFILE" -ForegroundColor Yellow
        return
    }
    Write-Host "[信息] 正在重新加载：$PROFILE" -ForegroundColor Cyan
    try {
        . $PROFILE
        # 此处模块可能已被 -Force 重载，严禁调用 Write-Dst* 等模块函数
        Write-Host "[成功] 已重新加载：$PROFILE" -ForegroundColor Green
    }
    catch {
        Write-Host "[失败] 重载出错：$_" -ForegroundColor Red
    }
}

function profile {
<#
.SYNOPSIS
打开当前 PowerShell Profile 文件。
.EXAMPLE
profile
#>
    [CmdletBinding()]
    param()
    $dir = Split-Path -Parent $PROFILE
    if (-not (Test-Path -LiteralPath $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
    if (-not (Test-Path -LiteralPath $PROFILE)) { New-Item -ItemType File -Path $PROFILE -Force | Out-Null }
    if (Test-DstWindows) { Start-Process notepad.exe -ArgumentList @($PROFILE) } else { Invoke-Item $PROFILE }
}

function which {
<#
.SYNOPSIS
查看命令的真实来源和所有匹配项。
.EXAMPLE
which codex
.EXAMPLE
which git
#>
    [CmdletBinding()]
    param([Parameter(Mandatory,Position=0)][string]$Name)
    Get-Command $Name -All | Select-Object CommandType, Name, Source, Definition
}
