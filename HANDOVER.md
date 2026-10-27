# DevShellTools Studio 项目移交文档

> 本文档面向接手负责人，汇总项目研发路线、完成进度、当前状态及关键决策。
> 更新时间：2026-08-05（安装自毁修复 + 联合审核后）

---

## 一、项目概览

**DevShellTools Studio** 是一个 Tauri 2 便携桌面工具，用于可视化管理 Windows PowerShell 开发快捷命令模块 DevShellTools。

- **仓库根目录**：`D:\workspace_test\github_desk\DevShellTools`
- **远程仓库**：https://repo.mvp.edu.kg/QAS/WinShellTools.git（仓库已更名为 WinShellTools）
- **Studio 子目录**：`devshelltools-studio/`
- **Studio / 模板版本**：1.0.5
- **根目录模块版本**：1.0.4（与模板双轨，内容相近但不完全同步）
- **便携 exe**：`devshelltools-studio/src-tauri/target/release/devshelltools-studio.exe`
- **技术栈**：Tauri 2.8.5（devtools）+ Svelte 5（runes）+ Tailwind CSS 4 + Rust（edition 2021）
- **构建工具**：pnpm + cargo（离线优先）

---

## 二、研发路线与完成进度

### 里程碑总览

| 里程碑 | 内容 | 状态 |
|--------|------|------|
| **M1–M4** | 骨架 / CRUD / AI / 迁移导出日志 WebView2 | ✅ 完成 |
| **M5** | release 黑屏修复 + 工作区改 MyDocuments 模块路径 | ✅ 完成 |
| **M6** | 静默子进程 + 函数级 CRUD + 软安装/卸载 + 多 AI Profile + Toast | ✅ 完成 |
| **M6.1** | 安装反馈 + AI 预设/拉模型 + Toast/启动优化 | ✅ 完成 |
| **M6.2** | PS 批量解析 + 分类磁盘缓存 | ✅ 已合入主干 |
| **后 M6.1 修复串** | 安装提速、去 git 用户文案、ChatPanel 重构、TLS/预设等 | ✅ 完成（至 `7adc930`） |
| **M6.3** | 安装 source==target 自毁修复 + 软卸载保留工作区 + DST_MY_DOCUMENTS 隔离 + UX | ✅ 本轮 |

### 当前测试状态（2026-08-05 验证）

| 套件 | 测试数 | 状态 |
|------|--------|------|
| lib 单元测试 | 39 | ✅ 全过 |
| M2 集成测试 | 4 | ✅ 全过 |
| M3 集成测试 | 6 | ✅ 全过 |
| M4 集成测试 | 5 | ✅ 全过 |
| M6 集成测试 | 4 | ✅ 全过（含安装不自毁） |
| **总计** | **58** | **全过** |

> m1_acceptance 已随 git 工作区移除而删除。

### 关键设计（现行）

1. **工作区路径**：`[Environment]::GetFolderPath('MyDocuments')\WindowsPowerShell\Modules\DevShellTools`（非 `USERPROFILE\Documents`）。测试用环境变量 **`DST_MY_DOCUMENTS`** 隔离；仅改 `USERPROFILE` 无效。
2. **无 git 快照**：用户端不涉及版本控制；`git.rs` 已移除。
3. **安装**：工作区即 PS5.1 模块目录时跳过自复制；白名单复制模块文件；`Import-Module` 使用显式 `.psd1` 路径；Studio 每次安装前覆盖写入内嵌 `install.ps1`。
4. **软卸载**：含 `.studio` 的目录保留，仅清理 Profile 与 PS7 副本。
5. **分类加载**：`parse_public_batch` + 磁盘缓存；一致性校验延后到分类加载完成后；UI 加载遮罩。

---

## 三、构建与测试命令

### 构建便携 exe

```powershell
cd devshelltools-studio
pnpm install --no-frozen-lockfile          # 首次
$env:CARGO_NET_OFFLINE="true"
pnpm exec tauri build --no-bundle
# 产物：src-tauri/target/release/devshelltools-studio.exe
```

> **必须**用 `pnpm exec tauri build --no-bundle`，不能只用 `cargo build --release`（会绕过 frontendDist，启动 ERR_CONNECTION_REFUSED）。

### 测试

```powershell
cd devshelltools-studio\src-tauri
$env:CARGO_NET_OFFLINE="true"
cargo test --offline --lib -- --test-threads=1
cargo test --offline --test m2_acceptance -- --test-threads=1
cargo test --offline --test m3_acceptance -- --test-threads=1
cargo test --offline --test m4_acceptance -- --test-threads=1
cargo test --offline --test m6_acceptance -- --test-threads=1
```

---

## 四、待办事项

### 已完成（本轮）

- [x] 修复 install 自毁（source == PS5.1 目标）
- [x] 软卸载保留 Studio 工作区
- [x] `DST_MY_DOCUMENTS` 测试隔离
- [x] 初始化/分类加载文案与遮罩体验
- [x] 对齐 AGENTS.md / HANDOVER.md
- [x] m2/m4 验收测试与现行 API 对齐

### 后续可选

- [ ] 真正的 AI 流式逐 token 推送（Tauri event）
- [ ] 工作区 zip 单文件导出（引入 `zip` crate）
- [ ] 凭证迁移到 Windows Credential Manager
- [ ] Pester 覆盖根目录 PowerShell 模块
- [ ] CI/CD 自动构建发布
- [ ] 根模块 1.0.4 与模板 1.0.5 内容完全同步并统一升版

---

## 五、注意事项

1. **离线构建**：cargo 加 `--offline` 与 `$env:CARGO_NET_OFFLINE="true"`
2. **vite base**：`base: "./"` 必须在 `defineConfig` 顶层（否则 release 黑屏）
3. **版本双轨**：根 1.0.4 vs Studio/模板 1.0.5，升版时分别核对（见 `AGENTS.md`）
4. **真实 Documents**：勿在未设 `DST_MY_DOCUMENTS` 时对验收测试跑安装用例，以免污染用户模块目录

---

## 六、关键文件索引

| 文件 | 作用 |
|------|------|
| `AGENTS.md` | 仓库级 AI 代理指南 |
| `workspace.rs` | MyDocuments / `DST_MY_DOCUMENTS` 工作区路径 |
| `install_mgr.rs` | 软安装/卸载 |
| `templates/install.ps1` | 防自毁安装脚本（内嵌） |
| `templates/uninstall.ps1` | 软卸载脚本（内嵌） |
| `ps_parser.rs` / `sync.rs` | AST 批量解析 + 分类缓存 |
| `App.svelte` | 主界面（管理/AI/工具箱/设置） |
| `tests/common/mod.rs` | `IsolatedProfile`（`DST_MY_DOCUMENTS`） |
| `tests/m6_acceptance.rs` | 安装不自毁 / 软卸载验收 |
