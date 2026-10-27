# AGENTS.md

本文件面向 AI 编码代理，描述 DevShellTools 仓库的结构、约定与工作流程。仓库主要文档与代码注释均使用中文。

## 项目概览

远程仓库：https://repo.mvp.edu.kg/QAS/WinShellTools.git（仓库已更名为 WinShellTools；模块 / 产品名仍为 DevShellTools，命令、目录与文件名不变）。

本仓库包含两个相关但独立的部分：

1. **DevShellTools（仓库根目录）**：一个 Windows PowerShell 模块（当前版本 **1.0.4**），提供开发与运维快捷命令（文件浏览、Git、网络诊断、代理管理、PowerShell 会话管理等）。目标运行时同时兼容 Windows PowerShell 5.1 与 PowerShell 7。命令行入口是 `dsh`（交互式分类菜单与帮助）。

2. **devshelltools-studio/（子目录）**：一个 Tauri 2 桌面应用（当前版本 **1.0.5**），是上述模块的可视化管理工具。工作区路径为系统 **MyDocuments** 下的 `WindowsPowerShell\Modules\DevShellTools`（与 `install.ps1` 安装到 PS 5.1 的默认路径一致，支持文件夹重定向），提供分类/函数的编辑、AI 辅助生成、安全检查、一致性校验、PowerShell 语法校验、迁移/导出导入、软安装/卸载。元数据在 `.studio/` 子目录。**不再使用 git 工作区快照**（用户端不涉及版本控制）。

版本双轨：根目录模块源码为 1.0.4；Studio `templates/` 内嵌模块为 1.0.5（带 `@DST-Category` 元数据）。两者内容相近但不完全同步——改模块时如需 Studio 同步，应同时更新 `templates/`。

## 目录结构

```text
DevShellTools/
├─ DevShellTools.psd1        # 模块清单：版本、FunctionsToExport 导出列表
├─ DevShellTools.psm1        # 模块加载器：dot-source Private/ 与 Public/，再 Export-ModuleMember
├─ Private/Common.ps1        # 内部辅助函数（Write-Dst*、Assert-Dst*、Test-Dst* 等），不导出
├─ Public/                   # 按分类组织的公共命令脚本（Files/Git/Network/PowerShell/Proxy/Help）
├─ install.ps1               # 安装：复制到 5.1 与 7 的模块目录，并向 Profile 写入 Import-Module
├─ uninstall.ps1             # 卸载：清理 Profile；含 .studio 的目录软保留
└─ devshelltools-studio/
   ├─ package.json           # 前端：Svelte 5（runes）+ Vite 5 + Tailwind CSS 4 + @tauri-apps/api
   ├─ vite.config.ts         # 端口 1420；base: "./" 必须在 defineConfig 顶层（黑屏修复）
   ├─ svelte.config.js       # compilerOptions.runes = true（Svelte 5 runes 模式）
   ├─ src/                   # 前端源码（App.svelte + lib/api.ts + lib/stores + lib/components）
   ├─ templates/             # 模块模板：初始化工作区时整份复制（1.0.5）
   └─ src-tauri/             # Rust 后端（Tauri 2，edition 2021，rust 1.77+）
      ├─ Cargo.toml
      ├─ tauri.conf.json     # bundle.active = false（默认不打包安装程序）
      ├─ src/                # lib.rs、commands、workspace、sync、ps_parser、consistency、
      │                      # safety、template、ai_*、migrate、export、install_mgr、
      │                      # logging、webview2、error、main 等（无 git.rs）
      └─ tests/              # common/（DST_MY_DOCUMENTS 隔离锁）、m2/m3/m4/m6_acceptance.rs
```

## 构建与测试命令

PowerShell 模块本身无构建步骤，直接导入即可：

```powershell
Import-Module ./DevShellTools.psd1 -Force   # 从仓库根目录加载开发版
dsh                                          # 验证入口可用
```

Studio 应用（在 `devshelltools-studio/` 下，包管理器为 pnpm）：

```bash
pnpm install                # 安装前端依赖
pnpm dev                    # 仅启动 Vite 前端（端口 1420）
pnpm tauri dev              # 启动完整桌面应用（前端 + Rust 后端）
pnpm build                  # 构建前端到 dist/
# 发布便携 exe（必须用 Tauri CLI，不能只用 cargo build --release）：
pnpm exec tauri build --no-bundle
# 产物：src-tauri/target/release/devshelltools-studio.exe
```

```powershell
cd devshelltools-studio\src-tauri
$env:CARGO_NET_OFFLINE="true"
cargo test --offline --lib -- --test-threads=1
cargo test --offline --test m2_acceptance -- --test-threads=1
cargo test --offline --test m3_acceptance -- --test-threads=1
cargo test --offline --test m4_acceptance -- --test-threads=1
cargo test --offline --test m6_acceptance -- --test-threads=1
```

测试注意事项：验收测试通过 `tests/common/mod.rs` 的 `IsolatedProfile` 设置 **`DST_MY_DOCUMENTS`**（及临时 `USERPROFILE`）隔离工作区。仅改 `USERPROFILE` **不会**改变 `[Environment]::GetFolderPath('MyDocuments')`。测试依赖系统 `powershell.exe`，仅能在 Windows 上完整运行。模块目前没有 Pester 测试。

## 代码组织与约定

### PowerShell 模块（根目录）

- **加载机制**：`DevShellTools.psm1` 依次 dot-source `Private/*.ps1` 与 `Public/*.ps1`，然后按清单显式 `Export-ModuleMember`。**新增导出函数必须同时改三处**：`Public/` 中的函数定义、`DevShellTools.psm1` 的 `$exports` 数组、`DevShellTools.psd1` 的 `FunctionsToExport`；此外还要在 `Public/Help.ps1` 的帮助数据中登记（`dsh` 帮助系统依赖它）。`sync.rs` / `consistency.rs` 会校验这四处一致。
- **命名约定**：首字母小写的函数（如 `gs`、`lt`、`killport`）为公共导出命令；首字母大写的函数（如 `Assert-Git`、`Write-Dst*`、`Show-Dst*`）为内部辅助函数，不导出。该约定被 Studio 的 `sync.rs::is_exported` 硬编码依赖。
- **注释风格**：每个公共函数带 PowerShell 标准注释块 `<# .SYNOPSIS ... .EXAMPLE ... #>`，内容为中文；模块顶部 `Set-StrictMode -Version Latest`。
- **分类元数据**：`devshelltools-studio/templates/Public/*.ps1` 文件开头有 `@DST-Category ... @DST-Category-End` 注释块（字段：`Name`、`Title`、`Description`、`Aliases`），供 Studio 解析分类；无此块的文件（如 `Help.ps1`）视为公共部分。
- **显示输出**：面向用户的消息用 `Write-Dst*` 辅助函数（`[成功]`/`[警告]` 等中文前缀 + 颜色）；分类详情用固定宽度左对齐格式化，不用 `Format-Table -AutoSize`（避免中文宽度计算错位）。
- **安装脚本**：`install.ps1` 在源目录等于 PS5.1 目标目录时跳过自复制（避免 Studio 工作区自毁）；白名单复制 `psd1/psm1/Private/Public`；`Import-Module` 使用显式清单路径。支持环境变量 `DST_MY_DOCUMENTS`（测试隔离）。`uninstall.ps1` 对含 `.studio` 的目录跳过删除（软卸载）。

### Studio 后端（src-tauri/src/）

- `commands.rs`：所有 `#[tauri::command]` 入口，注册于 `lib.rs`。**做文件 IO 或子进程的命令一律 `async + run_blocking`**（同步命令在主线程执行，会冻结窗口事件 = UI 假死；切工具箱卡顿即此类问题）；纯内存命令（如 `safety_check`）可保持同步。
- `workspace.rs`：工作区 = MyDocuments（或 `DST_MY_DOCUMENTS`）下 `WindowsPowerShell\Modules\DevShellTools`；元数据在 `.studio/workspace.json`。
- `install_mgr.rs`：软安装/卸载；每次执行前用内嵌模板覆盖工作区 `install.ps1`/`uninstall.ps1`；运行时副本同步（`sync_runtime_modules`）按**逐字节内容比较**（`files_identical`，勿改回 mtime 比较——函数级删除会漏同步导致 PS7 副本残留已删命令）。
- `pwsh.rs`：PowerShell 7 运行时检测（`OnceLock` 缓存）；解析/校验/自测优先 pwsh、回退 5.1（`ps_parser::which_powershell`）；设置页展示状态并在未装 7.x 时引导下载。
- `migrate.rs`：从旧路径合并 `Public/*.ps1`。
- `ps_parser.rs`：调用 `powershell.exe` AST 解析；支持 `parse_public_batch` 批量解析（另有容错版 `parse_public_batch_lenient`，供配置包预览：坏文件逐个标记不整体失败）。
- `sync.rs`：重生成公共部分；分类列表内存 + 磁盘缓存。
- `export.rs`：旧版平铺导出/导入 + **完整配置包**（bundle）双向同步：导出 `Public/*.ps1 + Private/Common.ps1 + ai_profiles.json（仅元数据）+ manifest.json`；导入走 预览（语法/安全校验 + 与本地比对给出 add/overwrite/same/keep_local/invalid 动作）→ 勾选确认 → 写入（UTF-8 BOM）→ 自动 `regenerate_all` → 后台同步运行时目录。目标目录必须为空/不存在。
- `safety.rs`：静态安全扫描，规则与下方"安全边界"一致。
- 依赖管理：刻意保持最小依赖；离线构建优先（`$env:CARGO_NET_OFFLINE="true"`）。

### Studio 前端（src/）

- Svelte 5 runes 模式（`svelte.config.js` 已开启），状态用 `lib/stores/workspace.svelte.ts`（runes store）等。
- 与后端的全部交互集中在 `lib/api.ts`（`@tauri-apps/api` 的 `invoke` 封装），新增 Tauri 命令时在此补充类型与封装。
- 样式用 Tailwind CSS 4（通过 `@tailwindcss/vite` 插件，`src/app.css`）。

## 安全边界（必须遵守）

模块的设计红线，新增快捷命令时默认不得违反（`safety.rs` 对 Studio 写入的代码强制执行同样的规则）：

- 不提供 `git push --force` / `git reset --hard` / `git clean -f`（真实删除）的快捷命令；`gclean` 只允许 `-nd`/`-ndx` dry-run。
- `lpr` 只修改当前进程的代理环境变量，禁止写 `User` 级环境变量（`SetEnvironmentVariable(..., "User")`）。
- `killport` 默认只显示进程，必须显式 `-Stop` 才终止；`Stop-Process` 须配合 `-Confirm` 或 `SupportsShouldProcess`。
- `super` 只能打开新的管理员窗口，不得悄悄提升当前窗口；`psb` 只改当前进程执行策略。
- 禁止 `Remove-Item -Recurse -Force` 式的危险删除（`uninstall.ps1` 等明确卸载场景除外；Studio 工作区含 `.studio` 时软卸载不得删除该目录）。
- `install.ps1` / `uninstall.ps1` 只动当前用户的模块目录与 Profile，不需要管理员权限；安装前会对**非同源**旧目标目录做带时间戳的备份。

**会话例外（Studio AI 助手）**：用户在 AI 对话中输入 `/danger` 可激活**本会话**最高权限（跳过 system prompt 与 AI 代码块的 `safety` 红线校验，插入前二次确认）；输入 `/safe` 或点横幅关闭可恢复默认红线（助手页用 CSS 保活，切 tab 不清危险模式）。管理页手写保存分类等路径始终走默认红线，不受 `/danger` 影响。

**生成质量验收（AI 助手）**：回复代码块做三层验收——①语法 + 安全线（不过不可插入）；②质量警告（缺 `.SYNOPSIS`/`.EXAMPLE`、命令名不规范、无公共命令、与已有命令重名——可插入但醒目提示）；③插入后自动隔离试跑首个新命令的 example（失败不回滚，toast 警告）。发送时自动注入**当前工作区命令清单**上下文（`chat_stream_with_options` 的 `extra_system`），约束 AI 禁止重名、只引用真实存在的命令。**命名即命令**：用户命令 = 小写函数名；块内大写辅助函数随公共命令一并写入分类文件（返回值只汇报公共命令名）。

**会话持久化**：对话自动写入工作区 `.studio/sessions/`；`/resume`（或 `/sessions`）在聊天区列出编号历史，输入数字确认恢复；`/new` 新建；`/cancel` 取消选号。选号列表为快照，编号与当次列表严格对应。

**配置包红线（双向同步）**：API Key 永不随配置包导出/导入——导出只打包 AI Profile 元数据（`ai_keys/` 不读取、`key_configured` 强制置 false），导入端同样强制 false；导入的每个 `.ps1` 必须通过语法 + `safety.rs` 红线双重校验（预览一次、写入前一次），失败项不可勾选/拒写。

## 部署 / 发布

- 模块发布即运行根目录 `install.ps1`（复制到 MyDocuments 下 PS5.1/PS7 模块目录并写 Profile）。
- Studio：`pnpm exec tauri build --no-bundle`；`bundle.active = false`，默认只产出 exe。
- 版本号分散在多处，升版本时需核对：
  - 根模块 1.0.4：`DevShellTools.psd1`、`README.md`、`install.ps1` 文案、`Public/Help.ps1`
  - Studio/模板 1.0.5：`package.json`、`Cargo.toml`、`tauri.conf.json`、`templates/DevShellTools.psd1`、`templates/install.ps1`、`templates/Public/Help.ps1`、`template.rs` 的 `TEMPLATE_VERSION`
