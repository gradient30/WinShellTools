<script lang="ts">
  import type { Snippet } from "svelte";

  let { children }: { children: Snippet } = $props();
</script>

<div class="flex-1 overflow-y-auto px-4 py-4 space-y-4">
  {@render children()}
</div>
