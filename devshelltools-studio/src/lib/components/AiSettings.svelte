<script lang="ts">
  import { onMount } from "svelte";
  import { api, type AiPreset, type AiProfile, type AiProtocol, type PwshStatus } from "../api";
  import { showToast } from "../stores/toast";

  let profiles = $state<AiProfile[]>([]);
  let presets = $state<AiPreset[]>([]);
  let defaultId = $state<string>("");
  let loading = $state(true);
  let errMsg = $state<string | null>(null);
  let successMsg = $state<string | null>(null);
  /** PowerShell 7 运行时状态（顶部卡片；null = 检测中） */
  let pwsh = $state<PwshStatus | null>(null);

  let showDialog = $state(false);
  let editing = $state<AiProfile | null>(null);
  let selectedPresetId = $state("custom");
  let newKey = $state("");
  let testing = $state(false);
  let testOk = $state(false);
  let testMsg = $state("");
  let endpointNote = $state("");
  let testingProfile = $state<string | null>(null);
  let profileTestResult = $state<Record<string, { ok: boolean; msg: string }>>({});

  let fetchingModels = $state(false);
  let modelOptions = $state<string[]>([]);
  let showModelPicker = $state(false);
  let modelSearch = $state("");

  let filteredModels = $derived(
    modelOptions.filter((m) => m.toLowerCase().includes(modelSearch.trim().toLowerCase()))
  );

  async function load() {
    loading = true;
    errMsg = null;
    try {
      const [meta, presetList] = await Promise.all([api.getAiProfilesMeta(), api.listAiPresets()]);
      profiles = meta.profiles;
      defaultId = meta.default_profile_id ?? profiles[0]?.id ?? "";
      presets = presetList;
    } catch (e) {
      errMsg = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    void load();
    // PowerShell 7 检测独立加载，不阻塞 AI 配置列表
    api.pwshStatus().then((s) => (pwsh = s)).catch(() => (pwsh = { pwsh_found: false, version: "" }));
  });

  async function openPwshDownload() {
    try {
      const url = await api.pwshDownloadUrl();
      window.open(url, "_blank");
    } catch {
      window.open("https://aka.ms/powershell", "_blank");
    }
  }

  /// 根据 base_url 匹配预设 ID（不依赖 protocol）
  function matchPresetId(p: AiProfile): string {
    const hit = presets.find((x) =>
      x.openai_base_url === p.base_url ||
      (x.anthropic_base_url && x.anthropic_base_url === p.base_url)
    );
    return hit?.id ?? "custom";
  }

  /// 应用预设：根据当前协议选对应端点
  function applyPreset(presetId: string) {
    if (!editing || presetId === "custom") {
      selectedPresetId = presetId;
      return;
    }
    const p = presets.find((x) => x.id === presetId);
    if (!p) return;
    selectedPresetId = presetId;
    // 根据当前协议选端点
    if (editing.protocol === "anthropic" && p.supports_anthropic) {
      editing.base_url = p.anthropic_base_url;
      editing.model = p.anthropic_default_model;
    } else {
      editing.protocol = "openai";
      editing.base_url = p.openai_base_url;
      editing.model = p.openai_default_model;
    }
    // Kimi K2/K3 系列固定 temperature=1，存 1 避免旧配置残留 0.7
    if (presetId === "kimi" || editing.model.toLowerCase().startsWith("kimi-k")) {
      editing.temperature = 1;
      endpointNote = `已应用「${p.name}」端点（该模型固定采样，请求不传 temperature）`;
    } else {
      endpointNote = `已应用「${p.name}」端点`;
    }
    testOk = false;
    testMsg = "";
  }

  /// 切换协议：保持提供商不变，只切换端点
  async function onProtocolChange(next: AiProtocol) {
    if (!editing) return;
    const preset = presets.find((x) => x.id === selectedPresetId);
    if (preset) {
      // 有预设：从预设取对应协议端点
      if (next === "anthropic" && preset.supports_anthropic) {
        editing.base_url = preset.anthropic_base_url;
        editing.model = preset.anthropic_default_model;
        endpointNote = `已切换为「${preset.name}」Anthropic 端点`;
      } else if (next === "openai") {
        editing.base_url = preset.openai_base_url;
        editing.model = preset.openai_default_model;
        endpointNote = `已切换为「${preset.name}」OpenAI 端点`;
      } else {
        // 预设不支持 Anthropic，保持端点不变但提示
        endpointNote = `「${preset.name}」不支持 Anthropic 协议`;
        return;
      }
    } else {
      // 自定义：调后端 suggest_endpoint 自动匹配
      const suggestion = await api.suggestAiEndpoint(next, editing.base_url);
      editing.base_url = suggestion.base_url;
      editing.model = suggestion.default_model;
      endpointNote = suggestion.note;
    }
    editing.protocol = next;
    testOk = false;
    testMsg = "";
  }

  function openAdd() {
    const p = presets[0];
    editing = {
      id: `p-${Date.now()}`,
      name: "",
      protocol: "openai",
      base_url: p?.openai_base_url ?? "https://api.openai.com/v1",
      model: p?.openai_default_model ?? "gpt-4o-mini",
      temperature: 0.7,
      max_tokens: 8192,
      key_configured: false
    };
    selectedPresetId = p?.id ?? "custom";
    newKey = "";
    testOk = false;
    testMsg = "";
    endpointNote = "";
    modelOptions = [];
    showModelPicker = false;
    showDialog = true;
  }

  function openEdit(p: AiProfile) {
    editing = { ...p };
    selectedPresetId = matchPresetId(p);
    newKey = "";
    testOk = false;
    testMsg = "";
    endpointNote = "";
    modelOptions = [];
    showModelPicker = false;
    showDialog = true;
  }

  async function fetchModels() {
    if (!editing) return;
    fetchingModels = true;
    errMsg = null;
    try {
      if (newKey.trim()) {
        modelOptions = await api.fetchAiModelsPreview(editing.protocol, editing.base_url, newKey.trim());
      } else if (editing.key_configured) {
        modelOptions = await api.fetchAiModels(editing.id);
      } else {
        errMsg = "请先填写 API Key";
        return;
      }
      showModelPicker = true;
      modelSearch = "";
      if (modelOptions.length > 0 && !modelOptions.includes(editing.model)) {
        editing.model = modelOptions[0];
      }
    } catch (e) {
      errMsg = String(e);
    } finally {
      fetchingModels = false;
    }
  }

  function pickModel(id: string) {
    if (editing) {
      editing.model = id;
      if (id.toLowerCase().startsWith("kimi-k")) editing.temperature = 1;
    }
    showModelPicker = false;
    testOk = false;
  }

  async function testConnection() {
    if (!editing) return;
    testing = true;
    testOk = false;
    testMsg = "";
    errMsg = null;
    try {
      await api.saveAiProfile(editing, newKey.trim() || undefined);
      const reply = await api.testAiProfile(editing.id);
      testOk = true;
      testMsg = reply.slice(0, 120);
    } catch (e) {
      testOk = false;
      testMsg = String(e);
    } finally {
      testing = false;
    }
  }

  async function testProfile(id: string) {
    testingProfile = id;
    profileTestResult[id] = { ok: false, msg: "测试中…" };
    try {
      const reply = await api.testAiProfile(id);
      profileTestResult[id] = { ok: true, msg: reply.slice(0, 80) };
      showToast(`测试通过：${reply.slice(0, 80)}`, "success", 4000);
    } catch (e) {
      const msg = String(e).slice(0, 120);
      profileTestResult[id] = { ok: false, msg };
      showToast(`测试失败：${msg}`, "error", 5000);
    } finally {
      testingProfile = null;
    }
  }

  async function saveProfile() {
    if (!editing || !editing.name.trim()) {
      errMsg = "配置名称不能为空";
      return;
    }
    if (!testOk) {
      errMsg = "请先测试连接成功后再保存";
      return;
    }
    try {
      await api.saveAiProfile(editing, newKey.trim() || undefined);
      if (defaultId === editing.id || profiles.length === 0) {
        await api.setDefaultAiProfile(editing.id);
        defaultId = editing.id;
      }
      showDialog = false;
      successMsg = "已保存配置";
      await load();
      // 通知 AI 助手面板刷新
      window.dispatchEvent(new CustomEvent("ai-config-changed"));
    } catch (e) {
      errMsg = String(e);
    }
  }

  async function remove(id: string) {
    if (!confirm("确认删除此 AI 配置？")) return;
    try {
      await api.deleteAiProfile(id);
      await load();
      window.dispatchEvent(new CustomEvent("ai-config-changed"));
    } catch (e) {
      errMsg = String(e);
    }
  }

  async function setDefault(id: string) {
    try {
      await api.setDefaultAiProfile(id);
      defaultId = id;
      window.dispatchEvent(new CustomEvent("ai-config-changed"));
    } catch (e) {
      errMsg = String(e);
    }
  }
</script>

<div class="p-5 max-w-3xl mx-auto">
  <!-- PowerShell 运行时：解析/校验/自测优先 pwsh 7.x，未装回退 5.1 -->
  {#if pwsh}
    <div class="mb-4 bg-dst-elevated border border-dst-border rounded-lg p-3 flex items-center justify-between gap-3">
      <div class="min-w-0">
        <div class="text-sm font-medium {pwsh.pwsh_found ? 'text-dst-success' : 'text-dst-warning-fg'}">
          {pwsh.pwsh_found ? `✓ PowerShell 7（pwsh ${pwsh.version}）` : "仅 Windows PowerShell 5.1"}
        </div>
        <p class="text-xs text-dst-fg-muted mt-0.5">
          命令解析、语法校验与自测优先使用 pwsh 7.x，未安装时回退 5.1（功能不受影响，7.x 更快）。
        </p>
      </div>
      {#if !pwsh.pwsh_found}
        <button
          type="button"
          class="shrink-0 px-3 py-1.5 text-sm bg-dst-accent text-dst-accent-fg hover:bg-dst-accent-hover rounded"
          onclick={openPwshDownload}>下载 PowerShell 7</button
        >
      {/if}
    </div>
  {/if}

  <div class="flex items-center justify-between mb-4">
    <h2 class="text-lg font-semibold text-dst-accent">AI 配置</h2>
    <button class="px-3 py-1.5 text-sm bg-dst-accent text-dst-accent-fg hover:bg-dst-accent-hover rounded" onclick={openAdd}>+ 添加配置</button>
  </div>

  {#if errMsg}
    <div class="mb-3 p-2 text-xs bg-dst-danger-bg border border-dst-danger-border text-dst-danger-fg rounded">{errMsg}</div>
  {/if}
  {#if successMsg}
    <div class="mb-3 p-2 text-xs bg-dst-success-bg border border-dst-success text-dst-success-fg rounded">{successMsg}</div>
  {/if}

  {#if loading}
    <div class="space-y-2">
      <div class="h-10 bg-dst-elevated rounded animate-pulse"></div>
      <div class="h-10 bg-dst-elevated rounded animate-pulse"></div>
    </div>
  {:else if profiles.length === 0}
    <p class="text-sm text-dst-fg-muted">暂无配置，请点击「添加配置」。</p>
  {:else}
    <ul class="space-y-2">
      {#each profiles as p (p.id)}
        <li class="bg-dst-elevated border border-dst-border rounded p-3 flex items-center justify-between gap-3">
          <div>
            <div class="text-sm text-dst-fg font-medium">
              {p.name}
              {#if defaultId === p.id}<span class="text-xs text-dst-accent ml-2">默认</span>{/if}
            </div>
            <div class="text-xs text-dst-fg-muted mt-0.5">
              {p.protocol === "openai" ? "OpenAI 兼容" : "Anthropic"} · {p.model} · {p.key_configured ? "Key 已配置" : "Key 未配置"}
            </div>
          </div>
          <div class="flex gap-2 shrink-0">
            {#if defaultId !== p.id}
              <button class="text-xs text-dst-fg-muted hover:text-dst-accent" onclick={() => setDefault(p.id)}>设为默认</button>
            {/if}
            <button class="text-xs text-dst-success hover:text-dst-success-fg" onclick={() => testProfile(p.id)} disabled={testingProfile === p.id}>
              {testingProfile === p.id ? "测试中…" : "测试"}
            </button>
            {#if profileTestResult[p.id]}
              <span class="text-xs {profileTestResult[p.id].ok ? 'text-dst-success' : 'text-dst-danger'} ml-1 max-w-40 truncate" title={profileTestResult[p.id].msg}>
                {profileTestResult[p.id].ok ? "✓" : "✗"} {profileTestResult[p.id].msg}
              </span>
            {/if}
            <button class="text-xs text-dst-accent hover:text-dst-accent" onclick={() => openEdit(p)}>编辑</button>
            <button class="text-xs text-dst-danger hover:text-dst-danger" onclick={() => remove(p.id)}>删除</button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

{#if showDialog && editing}
  <div class="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
    <div class="bg-dst-surface border border-dst-border rounded-lg w-full max-w-lg p-5 max-h-[90vh] overflow-y-auto">
      <h3 class="text-base font-semibold text-dst-accent mb-3">{editing.name ? "编辑配置" : "添加配置"}</h3>
      <div class="space-y-3">
        <label class="block">
          <span class="text-xs text-dst-fg-muted">名称</span>
          <input bind:value={editing.name} class="mt-1 w-full px-2 py-1.5 text-sm bg-dst-elevated border border-dst-border rounded" />
        </label>
        <label class="block">
          <span class="text-xs text-dst-fg-muted">提供商预设</span>
          <select
            class="mt-1 w-full px-2 py-1.5 text-sm bg-dst-elevated border border-dst-border rounded"
            value={selectedPresetId}
            onchange={(e) => applyPreset((e.currentTarget as HTMLSelectElement).value)}>
            <option value="custom">自定义</option>
            {#each presets as p (p.id)}
              <option value={p.id}>{p.name}</option>
            {/each}
          </select>
        </label>
        <label class="block">
          <span class="text-xs text-dst-fg-muted">协议</span>
          <select
            class="mt-1 w-full px-2 py-1.5 text-sm bg-dst-elevated border border-dst-border rounded"
            value={editing.protocol}
            onchange={(e) => onProtocolChange((e.currentTarget as HTMLSelectElement).value as AiProtocol)}>
            <option value="openai">OpenAI 兼容</option>
            <option value="anthropic" disabled={selectedPresetId !== "custom" && !presets.find((p) => p.id === selectedPresetId)?.supports_anthropic}>
              Anthropic {selectedPresetId !== "custom" && !presets.find((p) => p.id === selectedPresetId)?.supports_anthropic ? "（该提供商不支持）" : ""}
            </option>
          </select>
        </label>
        <label class="block">
          <span class="text-xs text-dst-fg-muted">Base URL</span>
          <input bind:value={editing.base_url} class="mt-1 w-full px-2 py-1.5 text-sm bg-dst-elevated border border-dst-border rounded font-mono" />
          {#if endpointNote}
            <p class="text-xs text-dst-warning-fg mt-1">{endpointNote}</p>
          {/if}
        </label>
        <div class="block">
          <div class="flex items-center justify-between">
            <span class="text-xs text-dst-fg-muted">模型</span>
            <button
              type="button"
              class="text-xs text-dst-accent hover:text-dst-accent disabled:opacity-50"
              onclick={fetchModels}
              disabled={fetchingModels || (!editing.key_configured && !newKey.trim())}>
              {fetchingModels ? "拉取中…" : "拉取模型"}
            </button>
          </div>
          <input bind:value={editing.model} class="mt-1 w-full px-2 py-1.5 text-sm bg-dst-elevated border border-dst-border rounded font-mono" />
          {#if showModelPicker && modelOptions.length > 0}
            <div class="mt-2 border border-dst-border rounded bg-dst-bg/80 p-2">
              <input
                bind:value={modelSearch}
                placeholder="搜索模型…"
                class="w-full px-2 py-1 text-xs bg-dst-surface border border-dst-border rounded mb-2" />
              <ul class="max-h-40 overflow-y-auto text-xs space-y-0.5">
                {#each filteredModels as m (m)}
                  <li>
                    <button type="button" class="w-full text-left px-2 py-1 rounded hover:bg-dst-elevated font-mono text-dst-fg" onclick={() => pickModel(m)}>
                      {m}
                    </button>
                  </li>
                {/each}
                {#if filteredModels.length === 0}
                  <li class="text-dst-fg-muted px-2 py-1">无匹配模型</li>
                {/if}
              </ul>
            </div>
          {/if}
        </div>
        <label class="block">
          <span class="text-xs text-dst-fg-muted">max_tokens（输出上限；DeepSeek 建议 ≥16384，Studio 会自动关闭思考）</span>
          <input
            type="number"
            min="256"
            max="65536"
            step="256"
            bind:value={editing.max_tokens}
            class="mt-1 w-full px-2 py-1.5 text-sm bg-dst-elevated border border-dst-border rounded font-mono" />
        </label>
        <label class="block">
          <span class="text-xs text-dst-fg-muted">API Key {editing.key_configured ? "(已配置，留空不修改)" : ""}</span>
          <input type="password" bind:value={newKey} class="mt-1 w-full px-2 py-1.5 text-sm bg-dst-elevated border border-dst-border rounded font-mono" />
        </label>
        {#if testMsg}
          <div class="p-2 text-xs rounded border {testOk ? 'border-dst-success bg-dst-success-bg text-dst-success-fg' : 'border-dst-danger-border bg-dst-danger-bg text-dst-danger-fg'}">
            {testOk ? "连接成功：" : "连接失败："}{testMsg}
          </div>
        {/if}
        <div class="flex gap-2 pt-2">
          <button class="px-3 py-1.5 text-sm bg-dst-muted hover:bg-dst-muted rounded" onclick={testConnection} disabled={testing}>
            {testing ? "测试中…" : "测试连接"}
          </button>
          <button class="px-3 py-1.5 text-sm bg-dst-accent text-dst-accent-fg hover:bg-dst-accent-hover rounded disabled:opacity-50" onclick={saveProfile} disabled={!testOk}>保存</button>
          <button class="px-3 py-1.5 text-sm bg-dst-muted hover:bg-dst-muted rounded" onclick={() => (showDialog = false)}>取消</button>
        </div>
      </div>
    </div>
  </div>
{/if}