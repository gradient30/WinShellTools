
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
重新加载 DevShellTools 模块（命令改动即刻生效）。
.DESCRIPTION
本函数不能在函数体内直接执行 . $PROFILE 或 Import-Module -Force，原因有二：
1. Profile 内的 Import-Module DevShellTools -Force 会卸载正在执行的本模块；
   实测（PS 5.1 / PS 7）从模块函数内发起该重载会损坏会话状态——公共命令
   仍可调用，但同文件辅助函数（如 Assert-Git）失联报错。
2. 函数作用域内 dot-source Profile，其中定义的自定义函数/别名不会保留。
PowerShell 7+：本函数只登记请求并注入一次性全局 prompt 钩子（用
[scriptblock]::Create() 动态创建以绑定全局会话状态、自包含、不依赖本
模块）；函数返回、回到提示符时由该钩子在无模块调用栈的全局上下文执行
Import-Module -Force，完成干净重载。
PowerShell 5.1：引擎限制（动态创建的 scriptblock 仍绑定模块状态），请
直接执行：. $PROFILE
Profile 本身有改动（自定义函数/别名）时同样请直接执行：. $PROFILE
.EXAMPLE
reload
#>
    [CmdletBinding()]
    param()
    # 全部使用内置命令：本函数与钩子都不得依赖本模块的任何函数。
    if (-not (Test-Path -LiteralPath $PROFILE)) {
        Write-Host "[警告] Profile 不存在：$PROFILE" -ForegroundColor Yellow
    }
    if ($PSVersionTable.PSVersion.Major -lt 6) {
        Write-Host "[提示] PowerShell 5.1 下请直接执行：. $PROFILE" -ForegroundColor Cyan
        Write-Host "[提示] 该命令会同时重载 DevShellTools 模块与 Profile。" -ForegroundColor DarkGray
        return
    }
    # 幂等注入全局 prompt 钩子；若 prompt 已被其它工具接管则重新包装在其外层。
    # 钩子必须用 [scriptblock]::Create() 动态创建：字面量 { } 与字符串 Set-Item
    # 都会绑定/继承本模块的会话状态，导致钩子内的 Import -Force 仍从模块状态
    # 发起（会话损坏）；动态创建的 scriptblock 绑定当前 runspace 的全局会话状态。
    $currentPrompt = "${Function:global:prompt}"
    if ($currentPrompt -notmatch '__DST_RELOAD_HOOK__') {
        $global:__DstOrigPrompt = ${Function:global:prompt}
        $hookCode = @'
            # __DST_RELOAD_HOOK__（标记用于防止重复包装）
            if ($global:__DstReloadRequested) {
                $global:__DstReloadRequested = $false
                Write-Host "[信息] 正在重载 DevShellTools 模块..." -ForegroundColor Cyan
                try {
                    Import-Module DevShellTools -Force -ErrorAction Stop
                    Write-Host "[成功] 模块已重载，命令改动已生效。" -ForegroundColor Green
                }
                catch {
                    Write-Host "[失败] 模块重载出错：$_" -ForegroundColor Red
                }
            }
            & $global:__DstOrigPrompt
'@
        Set-Item -Path Function:\global:prompt -Value ([scriptblock]::Create($hookCode.Trim()))
    }
    $global:__DstReloadRequested = $true
    Write-Host "[信息] 已安排重载：返回提示符后自动执行。" -ForegroundColor Cyan
    Write-Host "[提示] Profile 本身有改动（自定义函数/别名）请执行：. $PROFILE" -ForegroundColor DarkGray
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