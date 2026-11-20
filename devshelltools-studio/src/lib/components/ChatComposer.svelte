<script lang="ts">
  import type { Snippet } from "svelte";

  let {
    input = $bindable(""),
    loading,
    disabled,
    placeholder,
    rows,
    toneClass,
    onKeydown,
    onSend,
    onStop,
    children
  }: {
    input?: string;
    loading: boolean;
    disabled: boolean;
    placeholder: string;
    rows: number;
    toneClass: string;
    onKeydown: (event: KeyboardEvent) => void;
    onSend: () => void;
    onStop: () => void;
    children?: Snippet;
  } = $props();
</script>

<div class="flex gap-2 items-end">
  <div class="relative flex-1 min-w-0">
    {@render children?.()}
    <textarea
      id="dst-chat-input"
      bind:value={input}
      {rows}
      onkeydown={onKeydown}
      {placeholder}
      class="w-full px-3 py-2 text-sm bg-dst-elevated border rounded-lg text-dst-fg focus:outline-none resize-y min-h-[2.5rem] max-h-48 {toneClass}"
      disabled={disabled}></textarea>
  </div>
  {#if loading}
    <button
      type="button"
      class="px-4 py-2 text-sm bg-dst-btn-danger text-dst-btn-danger-fg hover:opacity-90 rounded-lg shrink-0"
      onclick={onStop}>
      停止
    </button>
  {:else}
    <button
      type="button"
      class="px-4 py-2 text-sm bg-dst-accent text-dst-accent-fg hover:bg-dst-accent-hover rounded-lg disabled:opacity-50 shrink-0"
      onclick={onSend}
      disabled={!input.trim() || disabled}>
      发送
    </button>
  {/if}
</div>
