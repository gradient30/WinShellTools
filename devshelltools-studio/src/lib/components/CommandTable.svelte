<script lang="ts">
  import type { PsFunction, PsParam } from "../api";
  import { api } from "../api";
  import {
    editableDefaultParams,
    formatExamples,
    formatParamLine,
    formatUsage
  } from "../commandUsage";
  import { withBusy } from "../stores/busy";
  import { showToast } from "../stores/toast";

  let {
    fileName,
    functions,
    onChanged,
    onAiGenerate
  }: {
    fileName: string;
    functions: PsFunction[];
    onChanged: () => void | Promise<void>;
    onAiGenerate: (func: PsFunction | null) => void;
  } = $props();

  /** null=关闭；"__new__"=新建；其它=编辑中的命令名 */
  let editingName = $state<string | null>(null);
  let draftName = $state("");
  let draftSynopsis = $state("");
  let draftExample = $state("");
  /** 参数名 → 默认值草稿 */
  let draftDefaults = $state<Record<string, string>>({});
  let draftParams = $state<PsParam[]>([]);
  let busy = $state(false);
  let errMsg = $state<string | null>(null);
  let expandedName = $state<string | null>(null);

  function startAdd() {
    editingName = "__new__";
    draftName = "";
    draftSynopsis = "";
    draftExample = "";
    draftDefaults = {};
    draftParams = [];
    errMsg = null;
    expandedName = null;
  }

  function startEdit(f: PsFunction) {
    editingName = f.name;
    draftName = f.name;
    draftSynopsis = f.synopsis;
    draftExample = f.first_example || f.name;
    const editable = editableDefaultParams(f);
    draftParams = editable;
    const defaults: Record<string, string> = {};
    for (const p of editable) {
      defaults[p.name] = p.default_value ?? "";
    }
    draftDefaults = defaults;
    errMsg = null;
    expandedName = null;
  }

  function cancelEdit() {
    editingName = null;
    errMsg = null;
  }

  function toggleExpand(name: string) {
    expandedName = expandedName === name ? null : name;
  }

  function onBackdropClick(e: MouseEvent) {
    if (e.target === e.currentTarget && !busy) cancelEdit();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape" && editingName && !busy) {
      e.preventDefault();
      e.stopPropagation();
      cancelEdit();
    }
  }

  function isSafetyError(msg: string | null): boolean {
    return !!msg && msg.includes("安全规则拦截");
  }

  function safetyDetail(msg: string | null): string {
    if (!msg) return "";
    return msg.replace(/^安全规则拦截：?/, "").trim();
  }

  async function save(acknowledgeDanger = false) {
    if (!draftName.trim() || !draftSynopsis.trim()) {
      errMsg = "命令名与说明不能为空";
      return;
    }
    busy = true;
    errMsg = null;
    const name = draftName.trim();
    const paramDefaults: Record<string, string> = {};
    for (const [k, v] of Object.entries(draftDefaults)) {
      if (v.trim() !== "") paramDefaults[k] = v.trim();
    }
    try {
      await withBusy(`正在保存 ${name}…`, async () => {
        await api.upsertFunction(
          fileName,
          name,
          draftSynopsis.trim(),
          draftExample.trim() || name,
          null,
          `更新命令 ${name}`,
          Object.keys(paramDefaults).length > 0 ? paramDefaults : null,
          acknowledgeDanger
        );
      });
      editingName = null;
      void onChanged();
      showToast(`已保存 ${name}`, acknowledgeDanger ? "warning" : "success", 2200);
    } catch (e) {
      errMsg = String(e);
    } finally {
      busy = false;
    }
  }

  async function saveWithDangerAck() {
    const detail = safetyDetail(errMsg);
    const prompt = detail
      ? `当前命令触发了安全红线：\n\n${detail}\n\n确认后仅保存本命令。仍要保存？`
      : "确认绕过安全红线并保存本命令？";
    if (!confirm(prompt)) return;
    await save(true);
  }

  async function remove(funcName: string) {
    if (!confirm(`确认删除命令 ${funcName}？`)) return;
    busy = true;
    errMsg = null;
    try {
      await api.deleteFunction(fileName, funcName, `删除命令 ${funcName}`);
      void onChanged();
      showToast(`已删除 ${funcName}`, "info", 2000);
    } catch (e) {
      errMsg = String(e);
    } finally {
      busy = false;
    }
  }

  async function test(funcName: string) {
    busy = true;
    try {
      const r = await api.testFunction(fileName, funcName);
      const msg = r.ok ? r.stdout.trim() || "(执行成功，无输出)" : r.stderr;
      showToast(`${funcName} · ${r.ok ? "通过" : "失败"}\n${msg}`, r.ok ? "success" : "error", 5000);
    } catch (e) {
      showToast(`${funcName} · 失败\n${String(e)}`, "error", 5000);
    } finally {
      busy = false;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div>
  <div class="flex items-center justify-between mb-3">
    <h3 class="text-sm font-semibold text-dst-fg flex items-center gap-2">
      <span class="text-dst-accent">⚡</span> 命令列表
      <span class="text-xs text-dst-fg-muted font-normal">({functions.length})</span>
    </h3>
    <button
      class="px-3 py-1 text-xs bg-dst-accent text-dst-accent-fg hover:bg-dst-accent-hover rounded transition-colors disabled:opacity-50 flex items-center gap-1"
      onclick={startAdd}
      disabled={busy}>
      + 添加命令
    </button>
  </div>

  {#if errMsg && !editingName}
    <div class="mb-3 p-2.5 text-xs bg-dst-danger-bg border border-dst-danger-border text-dst-danger-fg rounded">{errMsg}</div>
  {/if}

  {#if functions.length === 0 && !editingName}
    <div class="text-center py-8 text-dst-fg-subtle text-sm">
      暂无命令，点击"添加命令"创建
    </div>
  {:else}
    <div class="space-y-2">
      {#each functions as f (f.name)}
        <div class="bg-dst-elevated border border-dst-border rounded-lg overflow-hidden transition-all hover:border-dst-border">
          <div class="flex items-center gap-2 px-3 py-2.5">
            <button
              type="button"
              class="flex items-center gap-3 min-w-0 flex-1 text-left hover:bg-dst-elevated rounded"
              aria-expanded={expandedName === f.name}
              onclick={() => toggleExpand(f.name)}>
              <code class="text-sm font-mono text-dst-accent font-medium shrink-0">{f.name}</code>
              <span class="text-xs text-dst-fg-muted truncate">{f.synopsis || "(无说明)"}</span>
              <span class="text-dst-fg-subtle text-xs ml-auto">{expandedName === f.name ? "▾" : "▸"}</span>
            </button>
            <div class="flex items-center gap-1 shrink-0">
              <button class="px-2 py-0.5 text-xs text-dst-accent hover:text-dst-accent hover:bg-dst-menu-hover rounded transition-colors" onclick={() => startEdit(f)} disabled={busy}>
                编辑
              </button>
              <button class="px-2 py-0.5 text-xs text-dst-success hover:text-dst-success-fg hover:bg-dst-success-bg rounded transition-colors" onclick={() => test(f.name)} disabled={busy}>
                测试
              </button>
              <button
                class="px-2 py-0.5 text-xs text-dst-warning hover:text-dst-warning-fg hover:bg-dst-warning-bg rounded transition-colors"
                onclick={() => onAiGenerate(f)}
                disabled={busy}
                title="用 AI 检查当前命令：有问题给修复，无问题给优化/扩展建议">
                AI审阅
              </button>
              <button class="px-2 py-0.5 text-xs text-dst-danger hover:text-dst-danger-fg hover:bg-dst-danger-bg rounded transition-colors" onclick={() => remove(f.name)} disabled={busy}>
                删
              </button>
            </div>
          </div>
          {#if expandedName === f.name}
            <div class="px-3 py-2.5 border-t border-dst-border bg-dst-bg/30 space-y-2 text-xs">
              <div>
                <span class="text-dst-fg-muted">用法：</span>
                <code class="text-dst-accent font-mono">{formatUsage(f)}</code>
              </div>
              {#if (f.parameters ?? []).length > 0}
                <div>
                  <div class="text-dst-fg-muted mb-1">参数：</div>
                  <ul class="space-y-0.5 pl-0.5">
                    {#each f.parameters ?? [] as p (p.name)}
                      <li class="text-dst-fg">
                        <code class="font-mono text-dst-accent/90">{p.name}</code>
                        <span class="text-dst-fg-muted"> — </span>
                        <span>{formatParamLine(p).replace(`${p.name}：`, "")}</span>
                      </li>
                    {/each}
                  </ul>
                </div>
              {/if}
              <div>
                <span class="text-dst-fg-muted">示例：</span>
                <code class="text-dst-fg font-mono">{formatExamples(f)}</code>
              </div>
              <div>
                <span class="text-dst-fg-muted">说明：</span>
                <span class="text-dst-fg">{f.synopsis || "(无)"}</span>
              </div>
            </div>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>

{#if editingName}
  <div
    class="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4"
    onclick={onBackdropClick}
    role="presentation">
    <div
      class="bg-dst-surface border border-dst-border rounded-lg w-full max-w-lg max-h-[90vh] overflow-y-auto p-5 shadow-xl"
      role="dialog"
      aria-modal="true"
      aria-labelledby="cmd-edit-title">
      <h3 id="cmd-edit-title" class="text-base font-semibold text-dst-accent mb-3">
        {editingName === "__new__" ? "新建命令" : `编辑 ${editingName}`}
      </h3>

      {#if errMsg}
        <div class="mb-3 p-2.5 text-xs bg-dst-danger-bg border border-dst-danger-border text-dst-danger-fg rounded space-y-1.5">
          <div>{errMsg}</div>
          {#if isSafetyError(errMsg)}
            <div class="text-dst-fg-muted leading-relaxed">
              这不代表无法退出：点「取消」或按 Esc 可放弃修改并关闭；若确需保存含红线的命令，请使用「确认风险并保存」。
            </div>
          {/if}
        </div>
      {/if}

      <div class="space-y-3">
        <div class="grid grid-cols-1 gap-3 sm:grid-cols-3">
          <div>
            <label class="block text-xs text-dst-fg-muted mb-1" for="cmd-name">命令名</label>
            <input
              id="cmd-name"
              bind:value={draftName}
              placeholder="如 gs"
              disabled={editingName !== "__new__"}
              class="w-full px-2.5 py-1.5 text-xs bg-dst-bg border border-dst-border rounded font-mono text-dst-accent focus:border-dst-accent focus:outline-none disabled:opacity-60" />
          </div>
          <div class="sm:col-span-2">
            <label class="block text-xs text-dst-fg-muted mb-1" for="cmd-synopsis">说明（SYNOPSIS）</label>
            <input
              id="cmd-synopsis"
              bind:value={draftSynopsis}
              placeholder="如 查看 Git 状态"
              class="w-full px-2.5 py-1.5 text-xs bg-dst-bg border border-dst-border rounded text-dst-fg focus:border-dst-accent focus:outline-none" />
          </div>
        </div>

        <div>
          <label class="block text-xs text-dst-fg-muted mb-1" for="cmd-example">示例（EXAMPLE）</label>
          <input
            id="cmd-example"
            bind:value={draftExample}
            placeholder="如 gg 或 gg 5"
            class="w-full px-2.5 py-1.5 text-xs bg-dst-bg border border-dst-border rounded font-mono text-dst-fg focus:border-dst-accent focus:outline-none" />
        </div>

        {#if draftParams.length > 0}
          <div class="rounded-md border border-dst-border bg-dst-elevated/60 p-3 space-y-2">
            <div class="text-xs font-medium text-dst-fg">参数默认值</div>
            <p class="text-[11px] text-dst-fg-muted leading-relaxed">
              修改后会写回函数的 <code class="font-mono">param(...)</code> 默认值（如 <code class="font-mono">$Count = 20</code>）。
            </p>
            {#each draftParams as p (p.name)}
              <div class="flex items-center gap-2">
                <label class="w-28 shrink-0 text-xs font-mono text-dst-accent" for={`def-${p.name}`}>
                  ${p.name}
                </label>
                <input
                  id={`def-${p.name}`}
                  bind:value={draftDefaults[p.name]}
                  class="flex-1 px-2.5 py-1.5 text-xs bg-dst-bg border border-dst-border rounded font-mono text-dst-fg focus:border-dst-accent focus:outline-none"
                  placeholder={p.default_value ?? ""} />
                {#if p.description}
                  <span class="hidden sm:inline text-[11px] text-dst-fg-muted truncate max-w-[10rem]" title={p.description}>
                    {p.description}
                  </span>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>

      <div class="flex flex-wrap gap-2 pt-4 items-center">
        <button
          type="button"
          class="px-3 py-1.5 text-xs bg-dst-accent text-dst-accent-fg hover:bg-dst-accent-hover rounded transition-colors disabled:opacity-50"
          onclick={() => save(false)}
          disabled={busy}>
          {busy ? "保存中…" : "保存"}
        </button>
        {#if isSafetyError(errMsg)}
          <button
            type="button"
            class="px-3 py-1.5 text-xs bg-dst-btn-warning text-dst-btn-warning-fg hover:opacity-90 rounded transition-colors disabled:opacity-50"
            onclick={saveWithDangerAck}
            disabled={busy}>
            {busy ? "保存中…" : "确认风险并保存"}
          </button>
        {/if}
        <button
          type="button"
          class="px-3 py-1.5 text-xs bg-dst-muted hover:bg-dst-muted rounded transition-colors"
          onclick={cancelEdit}
          disabled={busy}>
          取消 (Esc)
        </button>
      </div>
    </div>
  </div>
{/if}
