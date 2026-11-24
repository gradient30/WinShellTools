// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { mount, tick, unmount } from "svelte";

const apiMock = vi.hoisted(() => ({
  getAiProfilesMeta: vi.fn(),
  listAiPresets: vi.fn(),
  pwshStatus: vi.fn(),
  saveAiProfile: vi.fn(),
  setDefaultAiProfile: vi.fn()
}));

vi.mock("../api", () => ({ api: apiMock }));
vi.mock("../stores/toast", () => ({ showToast: vi.fn() }));

import AiSettings from "./AiSettings.svelte";

const kimiProfile = {
  id: "kimi-profile",
  name: "k3.1-pro",
  protocol: "openai" as const,
  base_url: "https://api.kimi.com/coding/v1",
  model: "kimi-for-coding",
  temperature: 1,
  max_tokens: 8192,
  key_configured: true
};

async function flush() {
  await Promise.resolve();
  await tick();
  await Promise.resolve();
  await tick();
}

describe("AiSettings save state", () => {
  let app: ReturnType<typeof mount> | null = null;

  beforeEach(() => {
    apiMock.getAiProfilesMeta.mockResolvedValue({
      profiles: [kimiProfile],
      default_profile_id: kimiProfile.id
    });
    apiMock.listAiPresets.mockResolvedValue([]);
    apiMock.pwshStatus.mockResolvedValue({ pwsh_found: true, version: "7.6.4" });
    apiMock.saveAiProfile.mockImplementation(async (profile) => profile);
    apiMock.setDefaultAiProfile.mockResolvedValue(undefined);
  });

  afterEach(() => {
    if (app) unmount(app);
    app = null;
    document.body.innerHTML = "";
    vi.clearAllMocks();
  });

  it("keeps save enabled while editing an existing profile without a new key", async () => {
    const target = document.createElement("div");
    document.body.append(target);
    app = mount(AiSettings, { target });
    await flush();

    const editButton = Array.from(target.querySelectorAll("button")).find(
      (button) => button.textContent?.trim() === "编辑"
    );
    expect(editButton).toBeTruthy();
    editButton!.click();
    await flush();

    const saveButton = Array.from(target.querySelectorAll("button")).find(
      (button) => button.textContent?.trim() === "保存"
    ) as HTMLButtonElement | undefined;
    expect(saveButton).toBeTruthy();
    expect(saveButton!.disabled).toBe(false);

    const nameInput = target.querySelector(
      'input[bind\\:value], input'
    ) as HTMLInputElement;
    nameInput.value = "k3.1-pro-edited";
    nameInput.dispatchEvent(new Event("input", { bubbles: true }));
    await flush();
    expect(saveButton!.disabled).toBe(false);

    saveButton!.click();
    await flush();
    expect(apiMock.saveAiProfile).toHaveBeenCalledOnce();
  });

  it("allows saving a newly entered key without requiring a successful test", async () => {
    const target = document.createElement("div");
    document.body.append(target);
    app = mount(AiSettings, { target });
    await flush();

    const editButton = Array.from(target.querySelectorAll("button")).find(
      (button) => button.textContent?.trim() === "编辑"
    );
    editButton!.click();
    await flush();

    const keyInput = target.querySelector(
      'input[type="password"]'
    ) as HTMLInputElement;
    keyInput.value = "new-test-key";
    keyInput.dispatchEvent(new Event("input", { bubbles: true }));
    await flush();

    const saveButton = Array.from(target.querySelectorAll("button")).find(
      (button) => button.textContent?.trim() === "保存"
    ) as HTMLButtonElement;
    expect(saveButton.disabled).toBe(false);
    saveButton.click();
    await flush();
    expect(apiMock.saveAiProfile).toHaveBeenCalledOnce();
  });

  it("shows backend save errors inside the edit dialog", async () => {
    apiMock.saveAiProfile.mockRejectedValueOnce(
      new Error("AI 端点 URL 无效：bad endpoint")
    );
    const target = document.createElement("div");
    document.body.append(target);
    app = mount(AiSettings, { target });
    await flush();

    const editButton = Array.from(target.querySelectorAll("button")).find(
      (button) => button.textContent?.trim() === "编辑"
    );
    editButton!.click();
    await flush();

    const saveButton = Array.from(target.querySelectorAll("button")).find(
      (button) => button.textContent?.trim() === "保存"
    ) as HTMLButtonElement;
    saveButton.click();
    await flush();

    const dialog = target.querySelector('[role="alertdialog"], .fixed.inset-0');
    expect(dialog).toBeTruthy();
    expect(dialog!.textContent).toContain("AI 端点 URL 无效");
  });
});
