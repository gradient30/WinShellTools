<#!
@DST-Category
Name: git
Title: Git
Description: 状态、分支、提交、拉取、推送、日志和安全预览
Aliases:
@DST-Category-End
#>

function Assert-Git { Assert-DstCommand "git" }

function gs {
<#
.SYNOPSIS
查看 Git 工作区状态。
.EXAMPLE
gs
#>
    [CmdletBinding()] param()
    Assert-Git; & git status -sb
}
function gb {
<#
.SYNOPSIS
查看本地分支、跟踪关系和最后提交。
.EXAMPLE
gb
#>
    [CmdletBinding()] param()
    Assert-Git; & git branch -vv
}
function gco {
<#
.SYNOPSIS
使用 git checkout 切换分支或恢复路径。
.EXAMPLE
gco main
#>
    [CmdletBinding()] param([Parameter(ValueFromRemainingArguments=$true)][object[]]$GitArgs)
    Assert-Git; & git checkout @GitArgs
}
function gsw {
<#
.SYNOPSIS
使用 git switch 切换分支。
.EXAMPLE
gsw main
#>
    [CmdletBinding()] param([Parameter(ValueFromRemainingArguments=$true)][object[]]$GitArgs)
    Assert-Git; & git switch @GitArgs
}
function gswc {
<#
.SYNOPSIS
创建并切换到新分支。
.EXAMPLE
gswc feat/demo
#>
    [CmdletBinding()] param([Parameter(Mandatory,Position=0)][string]$Name)
    Assert-Git; & git switch -c $Name
}
function ga {
<#
.SYNOPSIS
添加指定文件到暂存区。
.EXAMPLE
ga README.md
#>
    [CmdletBinding()] param([Parameter(ValueFromRemainingArguments=$true)][object[]]$GitArgs)
    Assert-Git; & git add @GitArgs
}
function gaa {
<#
.SYNOPSIS
将全部变更加入暂存区。
.EXAMPLE
gaa
#>
    [CmdletBinding()] param()
    Assert-Git; & git add -A
}
function gcmsg {
<#
.SYNOPSIS
按指定说明创建 Git 提交。
.EXAMPLE
gcmsg "fix: 修复登录问题"
#>
    [CmdletBinding()] param([Parameter(Mandatory,Position=0)][string]$Message)
    Assert-Git; & git commit -m $Message
}
function gpp {
<#
.SYNOPSIS
推送当前分支到远程；名称避开 gp 冲突。
.EXAMPLE
gpp
.EXAMPLE
gpp origin main
#>
    [CmdletBinding()] param([Parameter(ValueFromRemainingArguments=$true)][object[]]$GitArgs)
    foreach ($arg in @($GitArgs)) {
        $text = [string]$arg
        if ($text -match '^--force' -or $text -match '^-[^-]*f') {
            throw "gpp 禁止 force push；请改用普通推送或显式执行 git 命令。"
        }
    }
    Assert-Git; & git push @GitArgs
}
function gpl {
<#
.SYNOPSIS
使用 fast-forward-only 安全拉取。
.EXAMPLE
gpl
#>
    [CmdletBinding()] param([Parameter(ValueFromRemainingArguments=$true)][object[]]$GitArgs)
    Assert-Git; & git pull --ff-only @GitArgs
}
function gf {
<#
.SYNOPSIS
获取全部远程并清理已删除的远程引用。
.EXAMPLE
gf
#>
    [CmdletBinding()] param()
    Assert-Git; & git fetch --all --prune
}
function gg {
<#
.SYNOPSIS
显示图形化精简提交历史，默认显示 20 条。
.EXAMPLE
gg
.EXAMPLE
gg 5
#>
    [CmdletBinding()]
    param(
        [Parameter(Position = 0)]
        [ValidateRange(1, 1000)]
        [int]$Count = 20
    )

    Assert-Git
    & git log --graph --decorate --oneline --all "--max-count=$Count"
}
function gd {
<#
.SYNOPSIS
查看未暂存差异。
.EXAMPLE
gd
#>
    [CmdletBinding()] param([Parameter(ValueFromRemainingArguments=$true)][object[]]$GitArgs)
    Assert-Git; & git diff @GitArgs
}
function gds {
<#
.SYNOPSIS
查看已暂存差异。
.EXAMPLE
gds
#>
    [CmdletBinding()] param([Parameter(ValueFromRemainingArguments=$true)][object[]]$GitArgs)
    Assert-Git; & git diff --staged @GitArgs
}
function grv {
<#
.SYNOPSIS
查看 Git 远程地址。
.EXAMPLE
grv
#>
    [CmdletBinding()] param()
    Assert-Git; & git remote -v
}
function gclean {
<#
.SYNOPSIS
预览 Git 未跟踪文件清理结果，不会实际删除。
.EXAMPLE
gclean
.EXAMPLE
gclean -IncludeIgnored
#>
    [CmdletBinding()] param([switch]$IncludeIgnored)
    Assert-Git
    if ($IncludeIgnored) { & git clean -ndx } else { & git clean -nd }
    Write-DstWarn "这里只是预览，没有删除任何文件。"
}
