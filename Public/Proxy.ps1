
function lpr {
<#
.SYNOPSIS
管理当前 PowerShell 会话代理。
.DESCRIPTION
支持 set、test、show、remove、doctor、help。
.EXAMPLE
lpr set
.EXAMPLE
lpr set http://127.0.0.1:7897
.EXAMPLE
lpr test
.EXAMPLE
lpr remove
#>
    [CmdletBinding()]
    param(
        [Parameter(Position=0)]
        [ValidateSet("set","test","show","remove","doctor","help")]
        [string]$Action = "help",

        [Parameter(Position=1)]
        [string]$ProxyUrl = "http://127.0.0.1:7897"
    )

    $names = @("HTTP_PROXY","HTTPS_PROXY","ALL_PROXY","http_proxy","https_proxy","all_proxy")

    switch ($Action) {
        "set" {
            try { $uri = [Uri]$ProxyUrl } catch { throw "代理地址无效：$ProxyUrl" }
            if (-not $uri.Host -or $uri.Port -le 0) { throw "代理地址缺少有效主机或端口：$ProxyUrl" }
            Set-DstProcessEnvironment -Names $names -Value $ProxyUrl
            Write-DstOk "已设置当前会话代理：$ProxyUrl"
            Write-DstWarn "只影响当前 PowerShell 窗口及其子进程。"
            lpr show
        }
        "show" {
            Write-DstTitle "当前会话代理"
            Get-DstProxyVariables | Format-Table Name, @{N="Value";E={if($_.Value){$_.Value}else{"<未设置>"}}} -AutoSize
        }
        "remove" {
            Set-DstProcessEnvironment -Names $names -Value $null
            Write-DstOk "当前会话代理变量已全部移除。"
            lpr show
        }
        "test" {
            $effective = [Environment]::GetEnvironmentVariable("HTTPS_PROXY","Process")
            if (-not $effective) { $effective = [Environment]::GetEnvironmentVariable("HTTP_PROXY","Process") }
            if (-not $effective) { throw "当前会话未设置代理，请先执行：lpr set" }
            $uri = [Uri]$effective
            $tcp = Test-NetConnection $uri.Host -Port $uri.Port -WarningAction SilentlyContinue
            if (-not $tcp.TcpTestSucceeded) {
                Write-DstFail "代理端口不可连接：$($uri.Host):$($uri.Port)"
                return
            }
            Write-DstOk "代理端口连接成功。"
            Assert-DstCommand "curl.exe"
            foreach ($target in @("https://github.com","https://chatgpt.com")) {
                Write-DstInfo "测试：$target"
                & curl.exe -I --silent --show-error --connect-timeout 10 --max-time 20 --proxy $effective $target |
                    Select-Object -First 5
                if ($LASTEXITCODE -eq 0) { Write-DstOk "$target 访问成功。" }
                else { Write-DstFail "$target 访问失败，curl 退出码：$LASTEXITCODE" }
            }
        }
        "doctor" {
            lpr show
            Write-DstTitle "用户级永久代理"
            $permanentFound = $false
            foreach ($name in $names) {
                $value = [Environment]::GetEnvironmentVariable($name,"User")
                if ($value) { $permanentFound = $true }
                "{0,-13}: {1}" -f $name, $(if($value){$value}else{"<未设置>"})
            }
            if ($permanentFound) {
                Write-DstWarn "检测到用户级永久代理变量；本工具不会自动修改，请在系统设置中显式清理。"
            }
            try { lpr test } catch { Write-DstWarn $_.Exception.Message }
        }
        "help" {
            dsh help proxy
        }
    }
}
