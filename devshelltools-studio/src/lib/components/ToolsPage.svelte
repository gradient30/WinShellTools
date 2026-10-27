<script lang="ts">
  import { api, type BundlePreview, type MigrationCheck, type MigrateResult, type Webview2Status } from "../api";
  import { showToast } from "../stores/toast";
  import BundleImportDialog from "./BundleImportDialog.svelte";

  let {
    migration = null,
    webview2 = null,
    logFiles = [],
    loaded = false,
    onRefresh,
    onWorkspaceChanged
  }: {
    migration: MigrationCheck | null;
    webview2: Webview2Status | null;
    logFiles: string[];
    loaded: boolean;
    onRefresh: () => void | Promise<void>;
    /** 工作区内容变化（如配置包导入）后刷新全局状态（分类列表/一致性/安装状态） */
    onWorkspaceChanged: () => void | Promise<void>;
  } = $props();

  let currentLog = $state("");
  let currentLogName = $state("");
  let busy = $state(false);
  let msg = $state("");
  let errMsg = $state("");

  async function doMigrate() {
    if (!migration?.has_legacy) return;
    if (
      !confirm(
        "将把较新的 Public 命令并入当前工作区，重生成公共部分并同步到 PowerShell 模块目录；旧 Studio 沙箱（Documents\\DevShellTools）会归档为 DevShellTools.migrated-*。是否继续？"
      )
    ) {
      return;
    }
    busy = true;
    errMsg = "";
    msg = "";
    try {
      const result: MigrateResult = await api.migrateLegacy();
      const files = result.migrated_files.length
        ? `更新文件：${result.migrated_files.join(", ")}`
        : "无需覆盖文件（工作区已是最新）";
      const archived = result.archived_dirs.length
        ? `\n已归档：${result.archived_dirs.join("；")}`
        : "";
      const full = `${result.message}\n${files}${archived}`;
      // 同步失败时后端仍可能 Ok，但 message 含失败说明——用警告而非成功绿条
      if (result.message.includes("同步到 PowerShell 模块目录失败")) {
        errMsg = full;
        showToast(result.message, "error", 7000);
      } else {
        msg = full;
        showToast(result.message, "success", 5000);
      }
      await onRefresh();
    } catch (e) {
      errMsg = String(e);
    } finally {
      busy = false;
    }
  }

  // ============ 配置包双向同步（一次配置，随处可用） ============

  let bundlePreview = $state<BundlePreview | null>(null);
  let bundleSourceDir = $state("");
  let bundleBusy = $state(false);

  /** 默认导出目录：记忆上次路径，否则 Documents\DevShellTools-sync */
  async function suggestExportDir(): Promise<string> {
    const remembered = localStorage.getItem("dst.sync.exportDir");
    if (remembered) return remembered;
    try {
      const status = await api.workspaceStatus();
      // root = <Documents>\WindowsPowerShell\Modules\DevShellTools，上三级即 Documents
      const docs = status.root.split(/[\\/]/).slice(0, -3).join("\\");
      return docs ? `${docs}\\DevShellTools-sync` : "DevShellTools-sync";
    } catch {
      return "DevShellTools-sync";
    }
  }

  async function doExportBundle() {
    const dir = prompt("输入导出目标目录（须为空目录或不存在）：", await suggestExportDir());
    if (!dir || !dir.trim()) return;
    busy = true;
    errMsg = "";
    msg = "";
    try {
      const result = await api.exportBundle(dir.trim());
      localStorage.setItem("dst.sync.exportDir", dir.trim());
      const aiNote =
        result.ai_profiles > 0 ? ` · AI 配置 ${result.ai_profiles} 个（不含 API Key）` : "";
      msg = `已导出 ${result.files.length} 个文件到：${result.target_dir}${aiNote}`;
      showToast("配置包导出成功", "success", 3000);
    } catch (e) {
      errMsg = String(e);
    } finally {
      busy = false;
    }
  }

  async function doImportBundle() {
    const remembered = localStorage.getItem("dst.sync.importDir") ?? "";
    const dir = prompt(
      "输入配置包目录（含 Public/*.ps1 的目录，兼容旧版平铺 .ps1 目录）：",
      remembered
    );
    if (!dir || !dir.trim()) return;
    const source = dir.trim();
    bundleBusy = true;
    busy = true;
    errMsg = "";
    msg = "";
    showToast("正在校验配置包（语法 / 安全 / 差异比对）…", "info", 4000);
    try {
      bundlePreview = await api.previewBundle(source);
      bundleSourceDir = source;
      localStorage.setItem("dst.sync.importDir", source);
    } catch (e) {
      errMsg = String(e);
    } finally {
      bundleBusy = false;
      busy = false;
    }
  }

  async function confirmBundleImport(selection: string[]) {
    bundleBusy = true;
    errMsg = "";
    try {
      const result = await api.importBundle(bundleSourceDir, selection);
      const parts = [`导入 ${result.imported.length} 项`];
      if (result.ai_added > 0) parts.push(`AI 配置新增 ${result.ai_added} 个`);
      if (result.regenerated) parts.push("公共部分已重生成");
      if (result.runtime_synced) parts.push("模块目录同步已触发");
      // 未安装（Profile 未配置）时新开窗口并不可用，需提示先安装
      let installNote = "";
      try {
        const st = await api.installStatus();
        if (!st.installed) installNote = "（尚未安装：请点顶部「安装」启用新开窗口使用）";
      } catch {
        /* 安装状态检测失败不影响导入结果展示 */
      }
      msg = `${parts.join(" · ")}。新开 PowerShell 窗口即可使用导入的命令${installNote}。`;
      if (result.errors.length > 0) {
        errMsg = result.errors.join("\n");
      }
      bundlePreview = null;
      showToast("配置包导入完成", "success", 4000);
      await onRefresh();
      await onWorkspaceChanged();
    } catch (e) {
      errMsg = String(e);
    } finally {
      bundleBusy = false;
    }
  }

  async function selectLog(name: string) {
    currentLogName = name;
    try {
      currentLog = await api.readLog(name);
    } catch (e) {
      currentLog = String(e);
    }
  }

  async function openWebview2Download() {
    const url = await api.webview2DownloadUrl();
    window.open(url, "_blank");
  }
</script>

<div class="p-5 max-w-3xl mx-auto space-y-6">
  <div class="flex items-center justify-between">
    <h2 class="text-lg font-semibold text-dst-accent">工具箱</h2>
    <button class="text-xs text-dst-fg-muted hover:text-dst-accent" onclick={() => onRefresh()} disabled={busy}>刷新</button>
  </div>

  {#if !loaded}
    <div class="space-y-3">
      <div class="h-24 bg-dst-elevated rounded animate-pulse"></div>
      <div class="h-24 bg-dst-elevated rounded animate-pulse"></div>
    </div>
  {:else}
    {#if msg}
      <div class="p-2 text-xs bg-dst-success-bg border border-dst-success text-dst-success-fg rounded">{msg}</div>
    {/if}
    {#if errMsg}
      <div class="p-2 text-xs bg-dst-danger-bg border border-dst-danger-border text-dst-danger-fg rounded">{errMsg}</div>
    {/if}

    <section class="bg-dst-elevated rounded-lg p-4 border border-dst-border">
      <h3 class="text-sm font-semibold text-dst-warning-fg mb-2">旧版迁移助手</h3>
      <p class="text-xs text-dst-fg-muted mb-3 leading-relaxed">
        当前工作区为最新编辑副本。迁移会合并其它位置中较新的命令，同步模块目录，并归档旧沙箱；完成后不应再提示「旧版安装」。
      </p>
      {#if migration?.has_legacy}
        <p class="text-xs text-dst-fg mb-2">仍需处理：</p>
        <ul class="text-xs text-dst-fg-muted mb-3 space-y-1.5">
          {#each migration.legacy_dirs as d}
            <li class="font-mono break-all leading-snug pl-2 border-l-2 border-dst-warning">{d}</li>
          {/each}
        </ul>
        <button
          class="px-3 py-1.5 text-sm bg-dst-btn-warning text-dst-btn-warning-fg hover:opacity-90 rounded disabled:opacity-50"
          onclick={doMigrate}
          disabled={busy}>
          {busy ? "迁移中…" : "执行迁移并清理旧版"}
        </button>
      {:else}
        <p class="text-xs text-dst-success/90">未检测到待迁移的旧版内容，当前即为最新。</p>
      {/if}
    </section>

    <section class="bg-dst-elevated rounded-lg p-4 border border-dst-border">
      <h3 class="text-sm font-semibold text-dst-accent mb-2">同步与迁移（配置包）</h3>
      <p class="text-xs text-dst-fg-muted mb-3 leading-relaxed">
        导出完整配置包（全部分类命令 + 辅助函数 + AI 配置，<b>不含 API Key</b>），拷贝到 U 盘 / 网盘分发给其它电脑；
        导入时逐文件校验语法与安全红线，勾选确认后自动重生成公共部分并同步 PowerShell 模块目录——一次配置，随处可用。
      </p>
      <div class="flex gap-2">
        <button
          class="px-3 py-1.5 text-sm bg-dst-accent text-dst-accent-fg hover:bg-dst-accent-hover rounded disabled:opacity-50"
          onclick={doExportBundle}
          disabled={busy || bundleBusy}>导出配置包</button
        >
        <button
          class="px-3 py-1.5 text-sm bg-dst-muted hover:bg-dst-muted rounded disabled:opacity-50"
          onclick={doImportBundle}
          disabled={busy || bundleBusy}>导入配置包</button
        >
      </div>
    </section>

    <section class="bg-dst-elevated rounded-lg p-4 border border-dst-border">
      <h3 class="text-sm font-semibold text-dst-success mb-2">WebView2 Runtime</h3>
      {#if webview2?.installed}
        <p class="text-xs text-dst-success">已安装，版本 {webview2.version}</p>
      {:else}
        <p class="text-xs text-dst-warning-fg mb-3">未检测到 WebView2 Runtime（Win10 需要安装）</p>
        <button class="px-3 py-1.5 text-sm bg-dst-btn-success text-dst-btn-success-fg hover:opacity-90 rounded" onclick={openWebview2Download}>下载 Evergreen Runtime</button>
      {/if}
    </section>

    <section class="bg-dst-elevated rounded-lg p-4 border border-dst-border">
      <h3 class="text-sm font-semibold text-dst-fg mb-2">操作日志</h3>
      {#if logFiles.length === 0}
        <p class="text-xs text-dst-fg-muted">暂无日志</p>
      {:else}
        <div class="flex gap-4">
          <ul class="text-xs text-dst-fg-muted w-40">
            {#each logFiles as f}
              <li>
                <button class="w-full text-left hover:text-dst-accent {currentLogName === f ? 'text-dst-accent' : ''}" onclick={() => selectLog(f)}>{f}</button>
              </li>
            {/each}
          </ul>
          <pre class="flex-1 text-xs font-mono bg-dst-bg border border-dst-border rounded p-2 overflow-auto max-h-64 text-dst-fg">{currentLog || "选择左侧日志文件查看"}</pre>
        </div>
      {/if}
    </section>
  {/if}

  {#if bundlePreview}
    <BundleImportDialog
      preview={bundlePreview}
      sourceDir={bundleSourceDir}
      busy={bundleBusy}
      onConfirm={confirmBundleImport}
      onCancel={() => (bundlePreview = null)} />
  {/if}
</div>
