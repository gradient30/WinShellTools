<script lang="ts">
  import { onMount } from "svelte";
  import {
    workspace,
    loading,
    errorMsg,
    successMsg,
    initProgress,
    refresh,
    init,
    clearMessages
  } from "./lib/stores/workspace";
  import { showToast } from "./lib/stores/toast";
  import { withBusy } from "./lib/stores/busy";
  import { api, type CategoryInfo, type ConsistencyReport, type InstallStatus, type MigrationCheck, type PsFunction, type Webview2Status } from "./lib/api";
  import CategoryList from "./lib/components/CategoryList.svelte";
  import CategoryEditor from "./lib/components/CategoryEditor.svelte";
  import NewCategoryDialog from "./lib/components/NewCategoryDialog.svelte";
  import ChatPanel from "./lib/components/ChatPanel.svelte";
  import AiSettings from "./lib/components/AiSettings.svelte";
  import ToolsPage from "./lib/components/ToolsPage.svelte";
  import ToastHost from "./lib/components/ToastHost.svelte";
  import BusyOverlay from "./lib/components/BusyOverlay.svelte";
  import ThemeSwitch from "./lib/components/ThemeSwitch.svelte";
  import { buildCommandReviewPrompt, extractFunctionSource } from "./lib/psSource";

  type Tab = "manage" | "chat" | "settings" | "tools";
  let tab = $state<Tab>("manage");

  let categories = $state<CategoryInfo[]>([]);
  let categoriesLoading = $state(false);
  let categoriesLoadMsg = $state("");
  let categoriesLoaded = $state(false);
  let selectedFileName = $state<string | null>(null);
  let selectedCategory = $derived(categories.find((c) => c.file_name === selectedFileName) ?? null);
  let fileContent = $state("");
  let consistency = $state<ConsistencyReport | null>(null);
  let showNewDialog = $state(false);
  let aiReady = $state(false);
  let aiReadyLoading = $state(false);
  let aiReadyChecked = $state(false);
  let installStatus = $state<InstallStatus | null>(null);
  let installBusy = $state(false);
  let aiPrompt = $state("");
  /** 递增以触发 ChatPanel 自动发送审阅提问 */
  let aiAutoSendToken = $state(0);

  let toolsMigration = $state<MigrationCheck | null>(null);
  let toolsWebview2 = $state<Webview2Status | null>(null);
  let toolsLogFiles = $state<string[]>([]);
  let toolsLoaded = $state(false);
  /** 右侧栏帮助：consistency | sync | null */
  let sidebarTip = $state<"consistency" | "sync" | null>(null);

  const TIP_CONSISTENCY =
    "比对四处是否一致：Public 实际导出的命令 ↔ 模块清单(.psd1) ↔ 加载器(.psm1) ↔ 帮助(Help.ps1)。通过表示导出列表齐全，可被 PowerShell 正确加载；不通过时可按错误修复，或点下方「同步公共部分」。「实际」= 扫描到的命令数，「psd1」= 清单声明数。";
  const TIP_SYNC =
    "不会还原或删除自定义命令。仅按当前 Public/*.ps1 重写 .psd1、.psm1、Help.ps1 的导出与帮助列表，分类源码不动。保存时通常已自动同步；不一致时再点此。";

  function toggleSidebarTip(which: "consistency" | "sync") {
    sidebarTip = sidebarTip === which ? null : which;
  }

  onMount(async () => {
    await refresh();
    if ($workspace?.initialized) {
      void loadInstallStatus();
      void loadCategories();
      void loadAiReady();
    }
    // AI 配置变更后即时刷新 AI 助手状态
    window.addEventListener("ai-config-changed", () => {
      void loadAiReady();
    });
  });

  $effect(() => {
    if (tab === "tools" && $workspace?.initialized && !toolsLoaded) {
      loadToolsData();
    }
  });

  async function loadInstallStatus() {
    try {
      installStatus = await api.installStatus();
    } catch (e) {
      installStatus = null;
      showToast(`安装状态检测失败：${String(e)}`, "error", 5000);
    }
  }

  async function loadCategories() {
    if (categoriesLoading) return;
    categoriesLoading = true;
    // 与「初始化成功」分阶段：成功条改为后续步骤提示，避免矛盾观感
    if ($successMsg === "工作区初始化成功。") {
      successMsg.set("工作区已就绪，正在解析分类元数据…");
    }
    categoriesLoadMsg = "正在解析分类（PowerShell 元数据）…";
    try {
      const result = await api.listCategories();
      categories = result.categories;
      categoriesLoaded = true;
      categoriesLoadMsg = result.cached ? "已从缓存加载分类" : "分类解析完成";
      if ($successMsg?.includes("正在解析分类")) {
        successMsg.set(null);
      }
      if (!result.cached) {
        showToast("分类信息已更新", "success", 2500);
      }
      if (!selectedFileName && categories.length > 0) {
        selectedFileName = categories[0].file_name;
        await loadFileContent();
      } else if (selectedFileName) {
        await loadFileContent();
      }
    } catch (e) {
      console.error(e);
      showToast(String(e), "error");
      categoriesLoaded = true;
      categoriesLoadMsg = "加载失败";
    } finally {
      categoriesLoading = false;
      setTimeout(() => {
        categoriesLoadMsg = "";
      }, 2000);
    }
  }

  async function loadFileContent() {
    if (!selectedFileName) return;
    try {
      fileContent = await api.readCategoryFile(selectedFileName);
    } catch (e) {
      console.error(e);
    }
  }

  async function loadConsistency() {
    try {
      consistency = await api.consistencyCheck();
    } catch (e) {
      console.error(e);
    }
  }

  async function loadAiReady() {
    // 首次以外不要进入 loading 分支，否则会卸载 ChatPanel，导致配置切换后对话状态丢失、像“没即时生效”
    const firstCheck = !aiReadyChecked;
    if (firstCheck) aiReadyLoading = true;
    try {
      aiReady = await api.aiReady();
    } catch {
      aiReady = false;
    } finally {
      aiReadyLoading = false;
      aiReadyChecked = true;
    }
  }

  async function loadManageSidebar() {
    await loadConsistency();
  }

  async function loadToolsData() {
    try {
      [toolsMigration, toolsWebview2, toolsLogFiles] = await Promise.all([
        api.checkMigration(),
        api.webview2Status(),
        api.listLogs()
      ]);
      toolsLoaded = true;
    } catch (e) {
      console.error(e);
      toolsLoaded = true;
    }
  }

  /** 工作区内容被工具箱（如配置包导入）改变后，刷新全局状态 */
  async function handleWorkspaceChanged() {
    await loadCategories();
    if (tab === "manage") void loadManageSidebar();
    void loadInstallStatus();
  }

  $effect(() => {
    // 等分类加载完成后再做一致性校验，避免与 parse_public_batch 叠加重负载导致界面卡顿
    if (
      tab === "manage" &&
      $workspace?.initialized &&
      categoriesLoaded &&
      !categoriesLoading &&
      !consistency
    ) {
      void loadManageSidebar();
    }
  });

  async function handleInit() {
    await init();
    if ($workspace?.initialized) {
      categoriesLoaded = false;
      consistency = null;
      await loadInstallStatus();
      await loadCategories();
      aiReadyChecked = false;
      void loadAiReady();
      toolsLoaded = false;
    }
  }

  async function onSelect(fileName: string) {
    selectedFileName = fileName;
    await loadFileContent();
  }

  async function handleSave(content: string, message: string, acknowledgeDanger = false) {
    if (!selectedFileName) return;
    try {
      await withBusy(`正在保存 ${selectedFileName}…`, async () => {
        await api.updateCategoryFile(selectedFileName, content, message, acknowledgeDanger);
      });
      await loadCategories();
      if (tab === "manage") void loadManageSidebar();
      showToast(acknowledgeDanger ? "已保存（已确认风险）" : "已保存", acknowledgeDanger ? "warning" : "success", 1800);
    } catch (e) {
      errorMsg.set(String(e));
    }
  }

  async function handleDelete(fileName: string) {
    if (!confirm(`确认删除分类文件 ${fileName}？此操作会自动重生成公共部分。`)) return;
    try {
      await api.deleteCategory(fileName, `删除分类 ${fileName}`);
      if (selectedFileName === fileName) selectedFileName = null;
      await loadCategories();
      if (tab === "manage") void loadManageSidebar();
      showToast(`已删除 ${fileName}`, "info", 2000);
    } catch (e) {
      errorMsg.set(String(e));
    }
  }

  async function handleCreate(fileName: string, content: string, message: string) {
    try {
      await api.createCategory(fileName, content, message);
      await loadCategories();
      selectedFileName = fileName;
      if (tab === "manage") void loadManageSidebar();
      showNewDialog = false;
      showToast(`已创建 ${fileName}`, "success", 2000);
    } catch (e) {
      errorMsg.set(String(e));
    }
  }

  async function handleSync() {
    try {
      await withBusy("正在全量同步公共部分…", async () => {
        await api.syncPublic("手动同步公共部分");
        await loadCategories();
        await loadManageSidebar();
      });
      showToast("公共部分已重生成", "success", 2500);
    } catch (e) {
      errorMsg.set(String(e));
    }
  }

  async function handleApplyCode(code: string, fileName: string, dangerMode = false) {
    try {
      const names = await withBusy(`正在插入到 ${fileName}…`, async () => {
        return api.applyAiCode(
          fileName,
          code,
          `AI 插入到 ${fileName}`,
          dangerMode
        );
      });
      selectedFileName = fileName;
      await loadCategories();
      void loadManageSidebar();
      void loadInstallStatus();
      showToast(
        dangerMode ? `已插入 ${names.join(", ")}（危险模式）` : `已插入 ${names.join(", ")}`,
        dangerMode ? "info" : "success",
        3500
      );
    } catch (e) {
      errorMsg.set(String(e));
      throw e;
    }
  }

  function handleAiGenerate(func: PsFunction | null) {
    const cat = selectedCategory;
    if (!cat) return;
    if (!aiReady) {
      showToast("请先配置 AI，再使用命令审阅", "error", 4000);
      tab = "settings";
      return;
    }

    if (func) {
      const source = extractFunctionSource(fileContent, func.name);
      aiPrompt = buildCommandReviewPrompt({
        categoryTitle: cat.category.title,
        categoryName: cat.category.name,
        fileName: cat.file_name,
        funcName: func.name,
        synopsis: func.synopsis,
        example: func.first_example || func.name,
        siblingNames: cat.functions.map((f) => f.name),
        source
      });
    } else {
      aiPrompt = [
        `请为 DevShellTools 分类「${cat.category.title}」（${cat.category.name}，文件 ${cat.file_name}）设计一个实用的新快捷命令。`,
        `已有命令：${cat.functions.map((f) => f.name).join(", ") || "(无)"}。`,
        "请给出：用途说明、.SYNOPSIS/.EXAMPLE、完整函数代码（powershell 代码块），并遵守安全红线。"
      ].join("\n");
    }
    aiAutoSendToken += 1;
    tab = "chat";
  }

  async function handleInstallToggle() {
    if (installStatus?.installed) {
      if (!confirm("确认卸载？将移除 Profile 中的 Import-Module 和模块副本。")) {
        return;
      }
      installBusy = true;
      try {
        const result = await withBusy("正在卸载 DevShellTools…", () => api.uninstallModule());
        installStatus = result.status;
        showToast(result.message, result.verified ? "success" : "info", 6000);
      } catch (e) {
        showToast(String(e), "error", 6000);
      } finally {
        installBusy = false;
      }
    } else {
      installBusy = true;
      try {
        const result = await withBusy("正在安装 DevShellTools…", () => api.installModule());
        installStatus = result.status;
        showToast(result.message, result.verified ? "success" : "info", 6000);
      } catch (e) {
        showToast(String(e), "error", 6000);
      } finally {
        installBusy = false;
      }
    }
  }

    </script>

<main class="h-screen flex flex-col bg-dst-bg">
  <ToastHost />
  <BusyOverlay />
  <header class="px-5 py-3 bg-dst-surface border-b border-dst-border flex items-center justify-between">
    <div>
      <h1 class="text-lg font-bold text-dst-accent">DevShellTools Studio</h1>
      <p class="text-xs text-dst-fg-muted">模板 v1.0.5 · M6</p>
    </div>
    <nav class="flex gap-1 items-center">
      <button
        class="px-3 py-1 text-xs rounded {tab === 'manage' ? 'bg-dst-accent text-dst-accent-fg' : 'bg-dst-muted text-dst-fg hover:bg-dst-menu-hover'}"
        onclick={() => (tab = "manage")}>管理</button
      >
      <button
        class="px-3 py-1 text-xs rounded {tab === 'chat' ? 'bg-dst-accent text-dst-accent-fg' : 'bg-dst-muted text-dst-fg hover:bg-dst-menu-hover'}"
        onclick={() => (tab = "chat")}>AI 助手 {aiReadyLoading ? "…" : aiReady ? "" : "(未配置)"}</button
      >
      <button
        class="px-3 py-1 text-xs rounded {tab === 'tools' ? 'bg-dst-accent text-dst-accent-fg' : 'bg-dst-muted text-dst-fg hover:bg-dst-menu-hover'}"
        onclick={() => (tab = "tools")}>工具箱</button
      >
      <button
        class="px-3 py-1 text-xs rounded {tab === 'settings' ? 'bg-dst-accent text-dst-accent-fg' : 'bg-dst-muted text-dst-fg hover:bg-dst-menu-hover'}"
        onclick={() => (tab = "settings")}>设置</button
      >
      <span class="w-px h-5 bg-dst-border mx-1"></span>
      <ThemeSwitch />
      {#if $workspace?.initialized || installStatus?.installed}
        <span class="w-px h-5 bg-dst-border mx-1"></span>
        <button
          class="px-3 py-1 text-xs rounded disabled:opacity-50 {installStatus?.installed
            ? 'bg-dst-btn-warning hover:opacity-90 text-dst-btn-warning-fg'
            : 'bg-dst-btn-success hover:opacity-90 text-dst-btn-success-fg'}"
          onclick={handleInstallToggle}
          disabled={installBusy || installStatus === null}
          title={installStatus
            ? `模块:${installStatus.ps51_module_present || installStatus.ps7_module_present ? "有" : "无"} · Profile:${installStatus.profile_configured ? "已配置" : "未配置"}`
            : "正在检测安装状态…"}>
          {installBusy
            ? "处理中…"
            : installStatus === null
              ? "检测中…"
              : installStatus.installed
                ? "卸载"
                : "安装"}
        </button>
      {/if}
    </nav>
  </header>

  {#if $errorMsg}
    <div class="px-4 py-2 bg-dst-danger-bg border-b border-dst-danger-border text-dst-danger-fg text-sm flex justify-between">
      <span>{$errorMsg}</span>
      <button class="ml-3 text-dst-danger hover:text-dst-danger-fg" onclick={clearMessages}>×</button>
    </div>
  {/if}
  {#if $successMsg}
    <div class="px-4 py-2 bg-dst-success-bg border-b border-dst-success text-dst-success-fg text-sm flex justify-between">
      <span>{$successMsg}</span>
      <button class="ml-3 text-dst-success hover:text-dst-success-fg" onclick={clearMessages}>×</button>
    </div>
  {/if}

  {#if $loading && !$workspace}
    <div class="flex-1 flex flex-col items-center justify-center text-dst-fg-muted gap-3">
      <div class="h-8 w-8 border-2 border-dst-accent/30 border-t-dst-accent rounded-full animate-spin"></div>
      <p class="text-sm">正在连接工作区…</p>
    </div>
  {:else if !$workspace?.initialized}
    <div class="flex-1 flex items-center justify-center p-6">
      <div class="bg-dst-elevated rounded-lg p-6 border border-dst-border max-w-md w-full text-center">
        <h2 class="text-lg font-semibold text-dst-warning-fg mb-2">首次使用</h2>
        <p class="text-sm text-dst-fg mb-1">未检测到工作区。</p>
        <p class="text-xs text-dst-fg-muted mb-4">
          将在 <code class="text-dst-accent">{$workspace?.root ?? ""}</code> 初始化 DevShellTools 1.0.5 模板。
        </p>
        {#if $initProgress}
          <div class="mb-4 text-left">
            <div class="flex justify-between text-xs text-dst-fg-muted mb-1">
              <span>{$initProgress.label}</span>
              <span>{$initProgress.percent}%</span>
            </div>
            <div class="h-2 bg-dst-surface rounded overflow-hidden">
              <div class="h-full bg-dst-accent text-dst-accent-fg transition-all duration-300" style="width: {$initProgress.percent}%"></div>
            </div>
          </div>
        {/if}
        <button
          class="px-4 py-2 bg-dst-accent hover:bg-dst-accent-hover text-dst-accent-fg rounded text-sm disabled:opacity-50"
          onclick={handleInit}
          disabled={$loading}>
          {$loading ? "初始化中…" : "初始化工作区"}
        </button>
      </div>
    </div>
  {:else}
    <div class="flex-1 flex flex-col min-h-0 overflow-hidden">
    {#if categoriesLoading}
      <div class="px-4 py-2 bg-dst-surface border-b border-dst-accent text-dst-fg text-sm flex items-center gap-3 shrink-0">
        <div class="h-4 w-4 border-2 border-dst-accent/30 border-t-dst-accent rounded-full animate-spin shrink-0"></div>
        <span>{categoriesLoadMsg || "正在加载分类信息…"}</span>
        <span class="text-xs text-dst-accent/80 ml-auto">首次约 5–10 秒，之后从缓存秒开；加载期间请稍候</span>
      </div>
    {/if}
    <div class="flex-1 overflow-hidden relative min-h-0">
      <!-- 分类加载遮罩仅盖住管理页，避免挡住 AI 助手的编辑/回退/停止 -->
      <div
        class="absolute inset-0 flex overflow-hidden {tab === 'manage' ? 'z-10' : 'z-0 pointer-events-none hidden'}"
        aria-hidden={tab !== "manage"}>
        {#if categoriesLoading}
          <div
            class="absolute inset-0 z-20 bg-dst-bg/40 backdrop-blur-[1px] flex items-center justify-center pointer-events-auto"
            aria-busy="true">
            <div class="bg-dst-surface border border-dst-border rounded-lg px-5 py-4 text-sm text-dst-fg shadow-lg max-w-sm text-center">
              <div class="mx-auto mb-3 h-6 w-6 border-2 border-dst-accent/30 border-t-dst-accent rounded-full animate-spin"></div>
              <p>{categoriesLoadMsg || "正在解析分类元数据…"}</p>
              <p class="text-xs text-dst-fg-muted mt-2">后台调用 PowerShell AST，界面暂时锁定以免误点</p>
            </div>
          </div>
        {/if}
        <CategoryList {categories} {selectedFileName} loading={categoriesLoading} onSelect={onSelect} />
        <CategoryEditor
          category={selectedCategory}
          {fileContent}
          onSave={handleSave}
          onDelete={handleDelete}
          onChanged={loadCategories}
          onAiGenerate={handleAiGenerate} />
        <aside class="w-64 shrink-0 bg-dst-surface border-l border-dst-border overflow-y-auto p-3">
          <div class="flex items-center gap-1.5 mb-2">
            <h3 class="text-xs font-semibold text-dst-fg-muted">一致性校验</h3>
            <button
              type="button"
              class="inline-flex h-4 w-4 items-center justify-center rounded-full border text-[10px] leading-none transition-colors {sidebarTip === 'consistency'
                ? 'border-dst-accent text-dst-accent bg-dst-elevated'
                : 'border-dst-border text-dst-fg-muted hover:border-dst-accent hover:text-dst-accent'}"
              onclick={() => toggleSidebarTip("consistency")}
              aria-expanded={sidebarTip === "consistency"}
              aria-label="一致性校验说明">?</button>
          </div>
          {#if sidebarTip === "consistency"}
            <p class="mb-3 rounded border border-dst-border bg-dst-bg/80 px-2.5 py-2 text-[11px] leading-relaxed text-dst-fg">
              {TIP_CONSISTENCY}
            </p>
          {/if}
          {#if categoriesLoading || !consistency}
            <div class="h-16 bg-dst-elevated rounded animate-pulse"></div>
            {#if categoriesLoading}
              <p class="text-xs text-dst-fg-muted mt-2">等待分类解析完成…</p>
            {/if}
          {:else if consistency}
            <div
              class="p-2 rounded text-xs mb-3 {consistency.ok
                ? 'bg-dst-success-bg border border-dst-success text-dst-success-fg'
                : 'bg-dst-danger-bg border border-dst-danger-border text-dst-danger-fg'}">
              {consistency.ok ? "✓ 通过" : "✗ 不一致"}
            </div>
            {#if consistency.errors.length > 0}
              <ul class="text-xs text-dst-danger-fg space-y-0.5 mb-2">
                {#each consistency.errors as e}<li>· {e}</li>{/each}
              </ul>
            {/if}
            <div class="text-xs text-dst-fg-muted mb-3">
              实际 {consistency.actual_functions.length} · psd1 {consistency.psd1_exports.length}
            </div>
          {/if}

          <div class="mt-4 flex flex-wrap items-center gap-2">
            <div class="flex items-center gap-1">
              <button
                class="px-2 py-1 text-xs bg-dst-muted hover:bg-dst-muted rounded"
                onclick={handleSync}>同步公共部分</button>
              <button
                type="button"
                class="inline-flex h-4 w-4 items-center justify-center rounded-full border text-[10px] leading-none transition-colors {sidebarTip === 'sync'
                  ? 'border-dst-accent text-dst-accent bg-dst-elevated'
                  : 'border-dst-border text-dst-fg-muted hover:border-dst-accent hover:text-dst-accent'}"
                onclick={() => toggleSidebarTip("sync")}
                aria-expanded={sidebarTip === "sync"}
                aria-label="同步公共部分说明">?</button>
            </div>
            <button class="px-2 py-1 text-xs bg-dst-accent text-dst-accent-fg hover:bg-dst-accent-hover rounded" onclick={() => (showNewDialog = true)}>新建分类</button>
          </div>
          {#if sidebarTip === "sync"}
            <p class="mt-2 rounded border border-dst-border bg-dst-bg/80 px-2.5 py-2 text-[11px] leading-relaxed text-dst-fg">
              {TIP_SYNC}
            </p>
          {/if}
        </aside>
      </div>

      <div
        class="absolute inset-0 overflow-hidden {tab === 'chat' ? 'z-10' : 'z-0 pointer-events-none hidden'}"
        aria-hidden={tab !== "chat"}>
        {#if aiReady}
          <ChatPanel
            {categories}
            initialPrompt={aiPrompt}
            autoSendToken={aiAutoSendToken}
            onApplyCode={handleApplyCode}
            onOpenSettings={() => (tab = "settings")} />
        {:else if aiReadyLoading}
          <div class="h-full flex items-center justify-center text-dst-fg-muted text-sm">正在检查 AI 配置…</div>
        {:else}
          <div class="h-full flex items-center justify-center p-6">
            <div class="text-center">
              <p class="text-sm text-dst-warning-fg mb-3">AI 未配置</p>
              <button class="px-4 py-2 bg-dst-accent text-dst-accent-fg hover:bg-dst-accent-hover rounded text-sm" onclick={() => (tab = "settings")}>前往设置</button>
            </div>
          </div>
        {/if}
      </div>

      <div
        class="absolute inset-0 overflow-y-auto {tab === 'settings' ? 'z-10' : 'z-0 pointer-events-none hidden'}"
        aria-hidden={tab !== "settings"}>
        <AiSettings />
      </div>

      <div
        class="absolute inset-0 overflow-y-auto {tab === 'tools' ? 'z-10' : 'z-0 pointer-events-none hidden'}"
        aria-hidden={tab !== "tools"}>
        <ToolsPage
          migration={toolsMigration}
          webview2={toolsWebview2}
          logFiles={toolsLogFiles}
          loaded={toolsLoaded}
          onRefresh={loadToolsData}
          onWorkspaceChanged={handleWorkspaceChanged} />
      </div>
    </div>
    </div>
  {/if}

  {#if showNewDialog}
    <NewCategoryDialog onCreate={handleCreate} onCancel={() => (showNewDialog = false)} />
  {/if}
</main>
