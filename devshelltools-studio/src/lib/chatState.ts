import type { CategoryInfo, ValidatedCodeBlock } from "./api";

export function findAssistantIndex(
  messages: Array<{ id: string }>,
  assistantId: string | null
): number {
  if (!assistantId) return -1;
  return messages.findIndex((message) => message.id === assistantId);
}

export function assignCodeBlockTargets(
  blocks: ValidatedCodeBlock[],
  categories: CategoryInfo[],
  assistantIndex: number,
  existing: Record<string, string> = {}
): Record<string, string> {
  const next = { ...existing };
  blocks.forEach((block, blockIndex) => {
    const match = block.category
      ? categories.find((category) => category.category.name === block.category)
      : null;
    next[`${assistantIndex}-${blockIndex}`] =
      match?.file_name ?? categories[0]?.file_name ?? "";
  });
  return next;
}
