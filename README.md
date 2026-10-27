# DevShellTools

Windows 上可扩展的 PowerShell 开发 / 运维快捷命令模块，附带可视化管理工具 **DevShellTools Studio**。

> 仓库：<https://repo.mvp.edu.kg/QAS/WinShellTools>（仓库已更名为 WinShellTools；模块与产品名仍为 DevShellTools，安装目录与命令名不变）

| 组件 | 版本 | 说明 |
|------|------|------|
| 根目录 PowerShell 模块 | **1.0.4** | `dsh` 入口；兼容 Windows PowerShell 5.1 与 PowerShell 7 |
| DevShellTools Studio | **1.0.5** | Tauri 2 便携桌面应用；内嵌模板版本 1.0.5 |

> 版本双轨：仓库根目录模块源码为 1.0.4；Studio `templates/` 为 1.0.5（带分类元数据）。日常通过 Studio 管理时以工作区 / 模板为准。

---

## 目录

- [PowerShell 模块](#powershell-模块)
- [DevShellTools Studio](#devshelltools-studio)
- [打包与部署](#打包与部署)
- [常见问题 QA](#常见问题-qa)
- [变更记录](#变更记录)

---

## PowerShell 模块

### 安装

在仓库根目录执行：

```powershell
Set-ExecutionPolicy -Scope Process Bypass
.\install.ps1
```

重新打开 PowerShell，或在当前窗口执行：

```powershell
Import-Module DevShellTools -Force
```

卸载：

```powershell
.\uninstall.ps1
```

也可在 Studio 顶部使用「安装 / 卸载」（软卸载：保留 Studio 工作区，注销 shell 自动加载）。

### 帮助与入口

```powershell
dsh                 # 交互式分类菜单
dsh list
dsh help
dsh help files
dsh help powershell
dsh help proxy
dsh help git
dsh help network
dsh files           # 等价于查看 files 分类
Get-Help lt -Examples
```

### 目录结构

```text
DevShellTools/
├─ DevShellTools.psd1 / .psm1
├─ Private/Common.ps1
├─ Public/                 # Files / Git / Network / PowerShell / Proxy / Help
├─ install.ps1 / uninstall.ps1
└─ devshelltools-studio/   # 可视化管理工具
```

扩展方式：在 `Public` 新增分类脚本，并同步更新 `.psd1` / `.psm1` / `Help.ps1` 导出列表（Studio「同步公共部分」可自动完成）。

### 安全边界

- `lpr` 只改当前进程代理；清理用户级变量需显式操作。
- `gclean` 仅 dry-run，不真实删除。
- `killport` 默认只显示进程；须加 `-Stop` 才终止。
- `super` 打开新的管理员窗口，不提升当前窗口。
- `psb` 只改当前进程执行策略。
- 不提供 `git push --force` / `git reset --hard` / 真实 `git clean -f` 快捷命令。

---

## DevShellTools Studio

便携桌面工具，用于可视化管理模块：分类 / 命令 CRUD、一致性校验、安全与语法检查、AI 审阅与生成、迁移 / 导出导入、软安装卸载。

### 工作区路径

固定为系统 **MyDocuments**（支持文件夹重定向）下：

```text
<MyDocuments>\WindowsPowerShell\Modules\DevShellTools
```

与 PS 5.1 默认模块安装路径一致。元数据在 `.studio\`。

### 主要能力

| 功能 | 说明 |
|------|------|
| 管理 | 分类与命令编辑、测试、同步公共部分、一致性校验 |
| AI 助手 | 多 Profile；命令列表「AI审阅」会自动提问：检查问题 → 优化 → 扩展建议 |
| 工具箱 | 迁移、配置包同步（导出/导入：全部分类命令 + 辅助函数 + AI 配置，**不含 API Key**）、日志、WebView2 |
| 设置 | AI 多 Profile；PowerShell 7 运行时检测——解析/校验/自测优先 `pwsh`，未安装回退 5.1 并提供官方下载入口 |
| 安装 / 卸载 | 写入 / 清理 Profile；软卸载禁用 `psd1`/`psm1` 防自动加载，保留工作区 |

### 开发运行

前置：Windows、[Node.js](https://nodejs.org/) + [pnpm](https://pnpm.io/)、[Rust](https://rustup.rs/)、WebView2 Runtime、系统 `powershell.exe`。

```powershell
cd devshelltools-studio
pnpm install
pnpm tauri dev          # 完整应用（前端 + Rust）
# 或仅前端：
pnpm dev                # http://localhost:1420
```

---

## 打包与部署

### 1. 打包 Studio 便携 exe（推荐）

```powershell
cd devshelltools-studio
pnpm install --no-frozen-lockfile   # 首次或依赖变更后
$env:CARGO_NET_OFFLINE = "true"    # 有完整本地 cargo 缓存时可开启
pnpm exec tauri build --no-bundle
```

产物：

```text
devshelltools-studio\src-tauri\target\release\devshelltools-studio.exe
```

**必须**使用 `pnpm exec tauri build --no-bundle`。不要只用 `cargo build --release`，否则不会正确嵌入前端，启动后可能 `ERR_CONNECTION_REFUSED` / 黑屏。

`tauri.conf.json` 中 `bundle.active = false`，默认不生成安装程序，只产出单文件 exe。

### 2. 部署 Studio

1. 将 `devshelltools-studio.exe` 拷贝到任意目录（可放 U 盘）。
2. 双击运行；首次使用点「初始化工作区」。
3. 需要在新开 PowerShell 中使用 `dsh` 时，在 Studio 点「安装」。
4. 仅编辑、暂不使用 shell 命令时，可不安装；「卸载」会注销自动加载但保留工作区。

### 3. 仅部署 PowerShell 模块（不用 Studio）

```powershell
# 从仓库根目录
Set-ExecutionPolicy -Scope Process Bypass
.\install.ps1
```

会安装到 MyDocuments 下 PS 5.1 / PS 7 模块目录，并写入当前用户 Profile。

### 4. 回归测试（开发者）

```powershell
cd devshelltools-studio\src-tauri
$env:CARGO_NET_OFFLINE = "true"
cargo test --offline --lib -- --test-threads=1
cargo test --offline --test m2_acceptance -- --test-threads=1
cargo test --offline --test m3_acceptance -- --test-threads=1
cargo test --offline --test m4_acceptance -- --test-threads=1
cargo test --offline --test m6_acceptance -- --test-threads=1
```

验收测试通过 `DST_MY_DOCUMENTS` 隔离，仅改 `USERPROFILE` 无效；需在 Windows 上运行。

### 5. 版本号核对清单

升版本时请逐一修改：

**根模块 1.0.4：** `DevShellTools.psd1`、`README.md`、`install.ps1` 文案、`Public/Help.ps1`

**Studio / 模板 1.0.5：** `package.json`、`Cargo.toml`、`tauri.conf.json`、`templates/DevShellTools.psd1`、`templates/install.ps1`、`templates/Public/Help.ps1`、`src-tauri/src/template.rs`（`TEMPLATE_VERSION`）

---

## 常见问题 QA

### Q1. 安装后新开 PowerShell 没有 `dsh`？

1. 确认 Profile 含 `Import-Module DevShellTools`：  
   `notepad $PROFILE`
2. Documents 若重定向到其他盘（如 `F:\Users\...\Documents`），请检查 **MyDocuments** 与 `USERPROFILE\Documents` 两套 Profile，避免只清了一边。
3. 执行：`Import-Module DevShellTools -Force`，再试 `dsh`。
4. 用 Studio「安装」重写 Profile 与模块清单。

### Q2. Studio 点「安装」报错 Import-Module 找不到模块？

常见于工作区路径就是 PS5.1 模块目录时，旧版 `install.ps1` 会自删自拷。请使用当前仓库 / 新版 Studio（安装脚本会跳过同源自复制，并用显式 `.psd1` 路径导入）。

### Q3. Studio 点「卸载」后，新开窗口 `dsh` 仍可用？

原因通常有：

1. 模块文件仍在 `Modules\DevShellTools`，PowerShell **命令发现会自动加载**；
2. 另一套 Documents 下的 Profile 仍有 `Import-Module`；
3. `Modules` 目录里残留 `DevShellTools.backup.*`，被递归扫到。

当前软卸载会：清理多路径 Profile、禁用活动 `psd1`/`psm1`（改为 `*.dst-disabled`）、把 Modules 内历史 backup 移到 `Documents\DevShellTools-backups\`。请使用新版 exe 再卸一次，并新开窗口验证：

```powershell
Get-Command dsh -ErrorAction SilentlyContinue   # 应无结果
```

### Q4. 初始化成功后又提示「正在解析分类」？是不是失败了？

不是。初始化完成后还会用 PowerShell AST 解析分类元数据（首次约数秒）。界面会显示加载遮罩；完成后可使用。二次打开一般走缓存，会快很多。

### Q5. 解析分类时界面卡住 / 未响应？

解析会启动 `powershell.exe`。请等待顶部进度结束；加载期间已加遮罩避免误点。若长期无响应，检查杀软是否拦截静默 PowerShell，或查看 `.studio\logs\`。

### Q6. 一致性校验是什么？「同步公共部分」呢？

- **一致性校验**：比对 Public 实际命令 ↔ `.psd1` ↔ `.psm1` ↔ `Help.ps1` 是否一致。  
- **同步公共部分**：按当前 `Public/*.ps1` 重写上述公共文件的导出 / 帮助列表。保存分类时通常会自动同步；不一致时可手动点。

右侧标题旁的 **?** 可查看简要说明。

### Q7. release 版启动黑屏或 `ERR_CONNECTION_REFUSED`？

1. 确认用 `pnpm exec tauri build --no-bundle` 打包，而不是裸 `cargo build --release`。  
2. 确认 `vite.config.ts` 里 `base: "./"` 在 `defineConfig` **顶层**（不要只写在 `build` 里）。  
3. 确认机器已安装 [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)。

### Q8. 打包时提示无法覆盖 / 删除 exe？

旧版 `devshelltools-studio.exe` 仍在运行。先结束进程再打包。

### Q9. 命令列表点「AI审阅」没反应？

1. 先到「设置」配置可用的 AI Profile（协议、端点、模型、API Key）。  
2. 配置后点「AI审阅」会跳转助手并**自动发送**审阅提问（检查 / 优化 / 扩展建议）。  
3. 网络与 TLS 需能访问你配置的 API 端点。

### Q10. Studio 与根目录模块版本不一致？

设计如此：根模块源码 1.0.4，Studio 内嵌模板 1.0.5。用 Studio 初始化 / 安装后，工作区以模板版本为准。若要从 Git 根目录脚本安装，则得到 1.0.4 模块源。

### Q11. 开发版如何不安装直接试用模块？

```powershell
cd <仓库根目录>
Import-Module .\DevShellTools.psd1 -Force
dsh
```

### Q12. 备份文件在哪？

- 新版安装备份：`Documents\DevShellTools-backups\`（在 Modules **之外**）。  
- 旧版误放在 `Modules\DevShellTools.backup.*` 的目录，卸载时会挪到 `Documents\DevShellTools-backups\orphaned-module-backups\`。

### Q13. 在 Studio 删除命令后，PowerShell 里还能用？

分两种情况：

1. **新开窗口仍可用**：不应发生——运行时副本按内容逐字节同步，删除后 PS5.1/PS7 目录与 psd1 导出清单都会同步更新；若遇到请反馈。
2. **已打开的窗口仍可用**：正常。模块已加载进该会话内存，执行 `Import-Module DevShellTools -Force` 重载即可（工具箱/编辑操作的提示语中已注明）。

---

## 变更记录

### 模块 1.0.4

- 分类命令详情改为固定宽度左对齐，避免中文列宽错位。

### 模块 1.0.3

- `dsh` 交互式菜单与分类浏览，降低首次使用门槛。

### 模块 1.0.2

- 修复 `install.ps1` / `uninstall.ps1` 中 `Join-Path` 数组写法错误。

### Studio 1.0.5（摘要）

- 工作区对齐 MyDocuments 模块路径；软安装 / 软卸载。  
- 安装防自毁；卸载禁用自动加载并清理双 Profile / Modules 污染备份。  
- 分类批量解析与磁盘缓存；一致性校验与同步公共部分说明。  
- AI 多 Profile、命令「AI审阅」自动提问。  
- 去除用户侧 git 快照流程；便携 exe 构建修复（vite `base`）。

更细的研发里程碑见仓库内 [`HANDOVER.md`](./HANDOVER.md)；面向 AI 代理的约定见 [`AGENTS.md`](./AGENTS.md)。
