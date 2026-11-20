<script lang="ts">
  import type { CategoryInfo, ValidatedCodeBlock } from "../api";

  let {
    block,
    latest,
    unchanged,
    expanded,
    targetFile,
    categories,
    applying,
    applied,
    onToggle,
    onTargetChange,
    onApply,
    onTest
  }: {
    block: ValidatedCodeBlock;
    latest: boolean;
    unchanged: boolean;
    expanded: boolean;
    targetFile: string;
    categories: CategoryInfo[];
    applying: boolean;
    applied: boolean;
    onToggle: () => void;
    onTargetChange: (value: string) => void;
    onApply: () => void;
    onTest: () => void;
  } = $props();

  let exported = $derived(block.exported_functions ?? block.functions);
  let helpers = $derived(block.helper_functions ?? []);
  let title = $derived(
    `${exported.length > 0 ? exported.join(", ") : "（无公共命令）"}${
      helpers.length > 0 ? `（+${helpers.length} 辅助）` : ""
    }`
  );
  let canApply = $derived(
    block.syntax_ok && block.safety_ok && exported.length > 0
  );
</script>

{#if !latest}
  <div class="border border-dst-border rounded-lg overflow-hidden opacity-70">
    <div class="flex items-center justify-between px-3 py-1.5 bg-dst-surface gap-2">
      <div class="flex items-center gap-2 text-xs min-w-0">
        <span class="text-dst-accent font-mono truncate">{title}</span>
        <span class="text-dst-fg-muted shrink-0">已有更新版本</span>
      </div>
      <button type="button" class="text-xs text-dst-fg-muted hover:text-dst-fg shrink-0" onclick={onToggle}>
        {expanded ? "收起旧稿" : "查看旧稿"}
      </button>
    </div>
    {#if expanded}
      <pre class="px-3 py-2 text-xs text-dst-fg-muted overflow-x-auto whitespace-pre-wrap font-mono bg-dst-bg/40 border-t border-dst-border">{block.code}</pre>
    {/if}
  </div>
{:else if unchanged}
  <div class="border border-dst-border rounded-lg px-3 py-1.5 flex items-center justify-between gap-2 bg-dst-elevated">
    <div class="flex items-center gap-2 text-xs min-w-0">
      <span class="text-dst-accent font-mono truncate">{title}</span>
      <span class="text-dst-fg-muted">与上一版相同 · {block.syntax_ok ? "语法✓" : "语法✗"} · {block.safety_ok ? "安全✓" : "安全✗"}</span>
    </div>
    <button type="button" class="text-xs text-dst-fg-muted hover:text-dst-fg shrink-0" onclick={onToggle}>
      {expanded ? "收起" : "查看"}
    </button>
  </div>
  {#if expanded}
    <pre class="mt-0 border border-t-0 border-dst-border rounded-b-lg px-3 py-2 text-xs text-dst-fg-muted overflow-x-auto whitespace-pre-wrap font-mono bg-dst-bg/40">{block.code}</pre>
  {/if}
{:else}
  <div class="border border-dst-border rounded-lg overflow-hidden">
    <div
      class="flex items-center justify-between px-3 py-1.5 bg-dst-elevated cursor-pointer hover:bg-dst-muted"
      role="button"
      tabindex="0"
      onclick={onToggle}
      onkeydown={(event) => event.key === "Enter" && onToggle()}>
      <div class="flex items-center gap-2 text-xs">
        <span class="text-dst-fg-muted font-mono">{expanded ? "▾" : "▸"}</span>
        <span class="text-dst-accent font-mono">{title}</span>
        <span class="text-dst-success">最新</span>
        <span class="text-dst-fg-muted">{block.syntax_ok ? "语法✓" : "语法✗"} · {block.safety_ok ? "安全✓" : "安全✗"}</span>
      </div>
    </div>
    {#if expanded}
      <div class="px-3 py-2 bg-dst-bg/50 space-y-2">
        <pre class="text-xs text-dst-fg overflow-x-auto whitespace-pre-wrap font-mono">{block.code}</pre>
        {#if !block.syntax_ok || !block.safety_ok}
          <p class="text-xs text-dst-warning-fg">
            {[block.syntax_err, ...block.safety_violations].filter(Boolean).join("；") || "未通过校验"}
          </p>
        {:else if (block.quality_warnings ?? []).length > 0}
          <p class="text-xs text-dst-warning-fg">⚠ 质量提醒：{block.quality_warnings.join("；")}</p>
        {/if}
        <div class="flex flex-wrap items-center gap-2">
          <select
            value={targetFile}
            onchange={(event) => onTargetChange(event.currentTarget.value)}
            class="text-xs bg-dst-elevated border border-dst-border rounded px-2 py-1">
            {#each categories as category (category.file_name)}
              <option value={category.file_name}>{category.category.title}</option>
            {/each}
          </select>
          <button
            type="button"
            class="px-2 py-1 text-xs bg-dst-btn-success text-dst-btn-success-fg hover:opacity-90 rounded disabled:opacity-40"
            disabled={!canApply || applying}
            onclick={onApply}>
            {applying ? "插入中…" : "插入到分类"}
          </button>
          {#if applied}
            <button
              type="button"
              class="px-2 py-1 text-xs border border-dst-border bg-dst-surface text-dst-fg hover:bg-dst-elevated rounded"
              onclick={onTest}>
              测试首个示例
            </button>
          {/if}
        </div>
      </div>
    {/if}
  </div>
{/if}
