import { describe, expect, it } from "vitest";
import { assignCodeBlockTargets, findAssistantIndex } from "./chatState";
import type { CategoryInfo, ValidatedCodeBlock } from "./api";

const categories: CategoryInfo[] = [
  {
    file_name: "Git.ps1",
    category: { name: "git", title: "Git", description: "", aliases: [] },
    functions: []
  },
  {
    file_name: "Files.ps1",
    category: { name: "files", title: "Files", description: "", aliases: [] },
    functions: []
  }
];

const block = (category: string | null): ValidatedCodeBlock => ({
  code: "",
  syntax_ok: true,
  syntax_err: "",
  safety_ok: true,
  safety_violations: [],
  functions: [],
  exported_functions: [],
  helper_functions: [],
  first_example: null,
  quality_warnings: [],
  category
});

describe("chat state", () => {
  it("finds the live assistant message", () => {
    expect(findAssistantIndex([{ id: "a" }, { id: "b" }], "b")).toBe(1);
    expect(findAssistantIndex([{ id: "a" }], null)).toBe(-1);
  });

  it("maps streamed code blocks to category files", () => {
    const targets = assignCodeBlockTargets(
      [block("files"), block(null)],
      categories,
      3
    );
    expect(targets["3-0"]).toBe("Files.ps1");
    expect(targets["3-1"]).toBe("Git.ps1");
  });
});
