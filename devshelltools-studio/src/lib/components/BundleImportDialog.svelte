<script lang="ts">
  import type { BundlePreview, BundleItem } from "../api";

  let {
    preview,
    sourceDir,
    busy = false,
    onConfirm,
    onCancel
  }: {
    preview: BundlePreview;
    sourceDir: string;
    busy?: boolean;
    onConfirm: (selection: string[]) => void;
    onCancel: () => void;
  } = $props();

  /** 勾选集合：invalid 永不可勾 */
  let selected = $state<Set<string>>(
    new Set(preview.items.filter((i) => i.checked).map((i) => i.rel))
  );

  let selectedCount = $derived(selected.size);
  let summary = $derived.by(() => {
    const counts: Record<string, number> = {};
    for (const i of preview.items) {
      counts[i.action] = (counts[i.action] ?? 0) + 1;
    }
    return counts;
  });

  const ACTION_LABEL: Record<string, string> = {
    add: "新增",
    overwrite: "覆盖",
    same: "相同",
    keep_local: "保留本地",
    invalid: "无效"
  };
  const ACTION_CLASS: Record<string, string> = {
    add: "bg-dst-success-bg border-dst-success text-dst-success-fg",
    overwrite: "bg-dst-warning-bg border-dst-warning text-dst-warning-fg",
    same: "bg-dst-elevated border-dst-border text-dst-fg-muted",
    keep_local: "bg-dst-elevated border-dst-warning text-dst-warning-fg",
    invalid: "bg-dst-danger-bg border-dst-danger-border text-dst-danger-fg"
  };
  const KIND_LABEL: Record<string, string> = {
    category: "分类脚本",
    private: "辅助函数",
    ai: "AI 配置"
  };

  function toggle(item: BundleItem) {
    if (item.action === "invalid") return;
    if (selected.has(item.rel)) selected.delete(item.rel);
    else selected.add(item.rel);
    selected = new Set(selected);
  }

  function confirmImport() {
    if (selectedCount === 0 || busy) return;
    onConfirm([...selected]);
  }

  function formatTime(iso: string): string {
    if (!iso) return "";
    try {
      return new Date(iso).toLocaleString();
    } catch {
      return iso;
    }
  }
</script>

<div class="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
  <div class="bg-dst-surface border border-dst-border rounded-lg w-full max-w-2xl max-h-[90vh] overflow-y-auto p-5">
    <h2 class="text-lg font-semibold text-dst-accent mb-1">导入配置包</h2>
    <p class="text-xs text-dst-fg-muted mb-3 break-all font-mono">{sourceDir}</p>

    <div class="flex flex-wrap gap-x-4 gap-y-1 text-xs text-dst-fg-muted mb-3">
      <span>来源模板：{preview.template_version || "未知"}</span>
      {#if preview.exported_at}<span>导出时间：{formatTime(preview.exported_at)}</span>{/if}
      <span>
        新增 {summary.add ?? 0} · 覆盖 {summary.overwrite ?? 0} · 相同 {summary.same ?? 0} ·
        保留本地 {summary.keep_local ?? 0} · 无效 {summary.invalid ?? 0}
      </span>
    </div>
    {#if !preview.manifest_found}
      <div class="mb-3 p-2 text-xs bg-dst-warning-bg border border-dst-warning text-dst-warning-fg rounded">
        未找到 manifest.json（旧版平铺导出目录）；将按目录内 .ps1 脚本处理。
      </div>
    {/if}

    <div class="border border-dst-border rounded overflow-hidden mb-4">
      <table class="w-full text-xs">
        <thead>
          <tr class="bg-dst-elevated text-dst-fg-muted">
            <th class="w-8 px-2 py-1.5"></th>
            <th class="text-left px-2 py-1.5">文件</th>
            <th class="w-20 px-2 py-1.5 text-left">类型</th>
            <th class="w-16 px-2 py-1.5 text-right">命令</th>
            <th class="w-20 px-2 py-1.5 text-left">动作</th>
            <th class="w-52 px-2 py-1.5 text-left">说明</th>
          </tr>
        </thead>
        <tbody>
          {#each preview.items as item (item.rel)}
            <tr
              class="border-t border-dst-border {item.action === "invalid"
                ? "opacity-60"
                : "hover:bg-dst-elevated/50"}">
              <td class="px-2 py-1.5 text-center">
                <input
                  type="checkbox"
                  checked={selected.has(item.rel)}
                  disabled={item.action === "invalid"}
                  onchange={() => toggle(item)}
                  class="accent-dst-accent" />
              </td>
              <td class="px-2 py-1.5 font-mono text-dst-fg break-all">{item.rel}</td>
              <td class="px-2 py-1.5 text-dst-fg-muted">{KIND_LABEL[item.kind] ?? item.kind}</td>
              <td class="px-2 py-1.5 text-right text-dst-fg-muted">
                {#if item.kind === "category"}{item.commands}{:else}-{/if}
              </td>
              <td class="px-2 py-1.5">
                <span class="inline-block px-1.5 py-0.5 rounded border text-[10px] {ACTION_CLASS[item.action] ?? ""}">
                  {ACTION_LABEL[item.action] ?? item.action}
                </span>
              </td>
              <td class="px-2 py-1.5 text-dst-fg-muted break-all">{item.note}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <div class="flex items-center gap-2">
      <button
        type="button"
        class="px-3 py-1.5 text-sm bg-dst-accent text-dst-accent-fg hover:bg-dst-accent-hover rounded disabled:opacity-50"
        onclick={confirmImport}
        disabled={selectedCount === 0 || busy}>
        {busy ? "导入中…" : `确认导入（${selectedCount} 项）`}
      </button>
      <button
        type="button"
        class="px-3 py-1.5 text-sm bg-dst-muted hover:bg-dst-muted rounded disabled:opacity-50"
        onclick={onCancel}
        disabled={busy}>
        取消
      </button>
      <span class="ml-auto text-[11px] text-dst-fg-subtle">
        导入后自动重生成公共部分并同步 PowerShell 模块目录
      </span>
    </div>
  </div>
</div>
