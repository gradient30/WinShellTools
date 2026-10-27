<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    api,
    type AiProfile,
    type CategoryInfo,
    type ChatMessage,
    type ChatSession,
    type SessionSummary,
    type ValidatedCodeBlock
  } from "../api";
  import {
    filterSlashCommands,
    parseSlashQuery,
    type SlashCommand
  } from "../slashCommands";
  import { showToast } from "../stores/toast";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import MarkdownText from "./MarkdownText.svelte";

  let {
    categories,
    initialPrompt,
    autoSendToken = 0,
    onApplyCode,
    onOpenSettings
  }: {
    categories: CategoryInfo[];
    initialPrompt: string;
    autoSendToken?: number;
    onApplyCode: (code: string, fileName: string, dangerMode?: boolean) => Promise<void>;
    onOpenSettings: () => void;
  } = $props();

  /** 带稳定 id 的 UI 消息（发往 API 时剥掉 id） */
  type UiMessage = ChatMessage & { id: string };

  let profiles = $state<AiProfile[]>([]);
  let profileId = $state("");
  let messages = $state<UiMessage[]>([]);
  let input = $state("");
  let loading = $state(false);
  let errMsg = $state<string | null>(null);
  /** 本会话最高权限：输入 /danger 开启，/safe 关闭 */
  let dangerMode = $state(false);
  let applying = $state(false);
  /** 正在插入的代码块 key，用于按钮文案 */
  let applyingKey = $state<string | null>(null);
  /** 危险模式插入前的二次确认 */
  let dangerInsertConfirm = $state<{
    block: ValidatedCodeBlock;
    key: string;
    fileName: string;
  } | null>(null);
  let replyCodeBlocks = $state<Record<number, ValidatedCodeBlock[]>>({});
  let targetFiles = $state<Record<string, string>>({});
  let expandedBlocks = $state<Set<string>>(new Set());
  let lastAutoToken = $state(0);
  let profilesReady = $state(false);
  let sessionReady = $state(false);
  let sessionId = $state("");
  let sessionTitle = $state("新会话");
  /** /resume 选号模式：列表快照冻结，编号与 summaries 下标严格对应 */
  let resumePick = $state<{
    summaries: SessionSummary[];
    listText: string;
  } | null>(null);
  let persistTimer: ReturnType<typeof setTimeout> | null = null;
  /** /resume 选号：连按两次 Esc 取消（与 /cancel 等价） */
  let lastEscAt = 0;
  /** `/` 命令联想高亮下标；与当前 input 相同时表示已填入/Esc 关闭 */
  let slashIndex = $state(0);
  let slashSuppressedFor = $state<string | null>(null);

  let slashMatches = $derived.by(() => {
    const q = parseSlashQuery(input, !!resumePick);
    if (q === null) return [] as SlashCommand[];
    return filterSlashCommands(q, !!resumePick);
  });
  let slashOpen = $derived(
    slashMatches.length > 0 && slashSuppressedFor !== input
  );
  let slashPrefixKey = $derived(
    parseSlashQuery(input, !!resumePick) ?? ""
  );

  $effect(() => {
    void slashPrefixKey;
    slashIndex = 0;
  });

  $effect(() => {
    if (slashIndex >= slashMatches.length) {
      slashIndex = Math.max(0, slashMatches.length - 1);
    }
  });

  function acceptSlash(cmd?: SlashCommand) {
    const pick = cmd ?? slashMatches[slashIndex];
    if (!pick) return;
    const next = pick.name;
    input = next;
    slashSuppressedFor = next;
    slashIndex = 0;
  }

  function closeSlashPalette() {
    slashSuppressedFor = input;
  }

  /** 就地编辑：正在编辑的用户消息 id；null 表示未在编辑 */
  let editingId = $state<string | null>(null);
  let editText = $state("");
  /** 回退后可「取消回退」恢复的快照 */
  let rewindBackup = $state<{
    messages: UiMessage[];
    replyCodeBlocks: Record<number, ValidatedCodeBlock[]>;
    targetFiles: Record<string, string>;
    expandedBlocks: string[];
    input: string;
  } | null>(null);

  let msgSeq = 0;
  function newId(role: string): string {
    msgSeq += 1;
    return `${role}-${msgSeq}-${Date.now()}`;
  }

  function toApiMessages(list: UiMessage[]): ChatMessage[] {
    return list.map(({ role, content }) => ({ role, content }));
  }

  function buildSessionPayload(): ChatSession {
    const blocks: Record<string, ValidatedCodeBlock[]> = {};
    for (const [k, v] of Object.entries(replyCodeBlocks)) {
      blocks[String(k)] = v;
    }
    return {
      id: sessionId || `pending-${Date.now()}`,
      title: sessionTitle,
      created_at: "",
      updated_at: "",
      profile_id: profileId,
      danger_mode: dangerMode,
      messages: messages.map((m) => ({ id: m.id, role: m.role, content: m.content })),
      reply_code_blocks: blocks,
      target_files: { ...targetFiles }
    };
  }

  function applySession(sess: ChatSession) {
    sessionId = sess.id;
    sessionTitle = sess.title || "新会话";
    profileId = sess.profile_id || profileId;
    dangerMode = !!sess.danger_mode;
    messages = (sess.messages || []).map((m) => ({
      id: m.id || newId(m.role),
      role: m.role,
      content: m.content
    }));
    const blocks: Record<number, ValidatedCodeBlock[]> = {};
    for (const [k, v] of Object.entries(sess.reply_code_blocks || {})) {
      const n = parseInt(k, 10);
      if (!Number.isNaN(n)) blocks[n] = v;
    }
    replyCodeBlocks = blocks;
    targetFiles = { ...(sess.target_files || {}) };
    expandedBlocks = new Set();
    editingId = null;
    editText = "";
    rewindBackup = null;
    resumePick = null;
    errMsg = null;
  }

  async function persistNow() {
    if (!sessionId || !profileId || resumePick) return;
    try {
      const saved = await api.saveChatSession(buildSessionPayload());
      sessionId = saved.id;
      sessionTitle = saved.title;
    } catch (e) {
      console.warn("会话保存失败", e);
    }
  }

  function schedulePersist() {
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      persistTimer = null;
      void persistNow();
    }, 300);
  }

  async function loadProfiles(syncDefault = true) {
    const meta = await api.getAiProfilesMeta();
    profiles = meta.profiles;
    const defaultId = meta.default_profile_id;
    const defaultExists = !!defaultId && profiles.some((p) => p.id === defaultId);
    if (syncDefault && defaultExists) {
      profileId = defaultId!;
      return;
    }
    if (profileId && profiles.some((p) => p.id === profileId)) return;
    profileId =
      (defaultExists ? defaultId! : null) ??
      profiles.find((p) => p.key_configured)?.id ??
      profiles[0]?.id ??
      "";
  }

  onMount(() => {
    void (async () => {
      try {
        await loadProfiles(true);
        if (profileId) {
          const sess = await api.loadOrCreateChatSession(profileId);
          applySession(sess);
        }
      } catch (e) {
        errMsg = String(e);
      } finally {
        sessionReady = true;
        profilesReady = true;
      }
    })();

    const onConfigChanged = () => {
      void loadProfiles(true).catch((e) => {
        errMsg = String(e);
      });
    };
    window.addEventListener("ai-config-changed", onConfigChanged);
    const onKeydown = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // 联想面板打开时优先关闭面板（由 textarea 处理）；此处仅处理选号 Esc×2
      if (slashOpen) return;
      if (!resumePick) return;
      e.preventDefault();
      handleResumeEscape();
    };
    window.addEventListener("keydown", onKeydown);
    return () => {
      window.removeEventListener("ai-config-changed", onConfigChanged);
      window.removeEventListener("keydown", onKeydown);
      if (persistTimer) clearTimeout(persistTimer);
      void persistNow();
    };
  });

  // 仅响应「命令列表 → AI审阅」的 token，不与编辑/回退共享 effect，避免状态被冲掉
  $effect(() => {
    const token = autoSendToken;
    const ready = profilesReady && sessionReady;
    const pid = profileId;
    if (!ready || !pid) return;
    if (token <= 0 || token === lastAutoToken) return;
    lastAutoToken = token;
    const prompt = initialPrompt.trim();
    if (!prompt) return;
    editingId = null;
    editText = "";
    rewindBackup = null;
    resumePick = null;
    replyCodeBlocks = {};
    targetFiles = {};
    expandedBlocks = new Set();
    input = "";
    void sendPrompt(prompt, { replaceAll: true });
  });

  async function openResumeList() {
    await persistNow();
    const result = await api.listChatSessions();
    // 冻结快照：之后编号只对这一次 summaries 有效
    resumePick = {
      summaries: result.summaries,
      listText: result.list_text
    };
    input = "";
    lastEscAt = 0;
    showToast(
      result.summaries.length
        ? `请输入编号 1–${result.summaries.length} 恢复；/cancel 或连按两次 Esc 取消`
        : "暂无历史会话",
      "info",
      4000
    );
  }

  function cancelResumePick(reason: "command" | "esc" = "command") {
    if (!resumePick) return;
    resumePick = null;
    lastEscAt = 0;
    errMsg = null;
    input = "";
    showToast(reason === "esc" ? "已取消恢复（Esc）" : "已取消恢复", "info", 2000);
  }

  function handleResumeEscape() {
    if (!resumePick) return;
    const now = Date.now();
    if (now - lastEscAt < 1500) {
      cancelResumePick("esc");
      return;
    }
    lastEscAt = now;
    showToast("再按一次 Esc 取消恢复列表", "info", 2000);
  }

  async function confirmResumeByNumber(n: number) {
    const snap = resumePick;
    if (!snap) return;
    if (n < 1 || n > snap.summaries.length) {
      errMsg = `编号无效：请输入 1–${snap.summaries.length}，或 /cancel`;
      return;
    }
    const target = snap.summaries[n - 1];
    if (!target) {
      errMsg = "编号无效";
      return;
    }
    const sess = await api.loadChatSession(target.id);
    applySession(sess);
    showToast(`已恢复：${sess.title || target.title}`, "success", 3000);
  }

  async function startNewSession() {
    await persistNow();
    const sess = await api.newChatSession(profileId);
    applySession(sess);
    showToast("已新建会话", "success", 2000);
  }

  function handleSessionCommand(raw: string): boolean {
    const trimmed = raw.trim();
    const cmd = trimmed.toLowerCase();

    // 选号模式：只接受数字 /cancel /resume，绝不把杂输入发给模型
    if (resumePick) {
      if (cmd === "/cancel") {
        cancelResumePick("command");
        return true;
      }
      if (cmd === "/resume" || cmd === "/sessions") {
        input = "";
        void openResumeList().catch((e) => {
          errMsg = String(e);
        });
        return true;
      }
      if (/^\d+$/.test(trimmed)) {
        input = "";
        void confirmResumeByNumber(parseInt(trimmed, 10)).catch((e) => {
          errMsg = String(e);
        });
        return true;
      }
      errMsg = `请输入编号 1–${resumePick.summaries.length || 0} 恢复；/cancel 或连按两次 Esc 取消`;
      input = "";
      return true;
    }

    if (cmd === "/resume" || cmd === "/sessions") {
      input = "";
      void openResumeList().catch((e) => {
        errMsg = String(e);
      });
      return true;
    }
    if (cmd === "/new") {
      input = "";
      void startNewSession().catch((e) => {
        errMsg = String(e);
      });
      return true;
    }
    if (cmd === "/danger") {
      dangerMode = true;
      input = "";
      messages = [
        ...messages,
        { id: newId("user"), role: "user", content: "/danger" },
        {
          id: newId("assistant"),
          role: "assistant",
          content:
            "【危险模式已开启】本会话可生成/插入含 git reset --hard、force-push、真实 git clean 等破坏性命令。输入 /safe 可恢复默认红线。"
        }
      ];
      showToast("危险模式已开启", "info", 3500);
      schedulePersist();
      return true;
    }
    if (cmd === "/safe") {
      dangerMode = false;
      input = "";
      messages = [
        ...messages,
        { id: newId("user"), role: "user", content: "/safe" },
        {
          id: newId("assistant"),
          role: "assistant",
          content: "已关闭危险模式，恢复默认安全红线。"
        }
      ];
      showToast("已恢复安全模式", "success", 2500);
      schedulePersist();
      return true;
    }
    return false;
  }

  /** 注入当前工作区真实命令清单，约束 AI：禁止重名、只引用真实存在的命令 */
  function buildWorkspaceContext(): string {
    if (!categories.length) return "";
    const lines = categories.map((c) => {
      const cmds = c.functions
        .filter((f) => /^[a-z]/.test(f.name))
        .map((f) => f.name)
        .join(", ");
      return `${c.category.name}: ${cmds || "(无)"}`;
    });
    return [
      "【当前工作区命令清单】（这些是真实可用的用户命令名；生成新命令时禁止与之重名，引用时只用清单内的名称）：",
      ...lines
    ].join("\n");
  }

  async function sendPrompt(text: string, opts?: { replaceAll?: boolean }) {
    const trimmed = text.trim();
    if (!trimmed || loading || !profileId) return;
    if (!opts?.replaceAll && handleSessionCommand(trimmed)) return;

    // 正常对话时退出选号模式（不应发生：选号已拦截）
    resumePick = null;

    if (opts?.replaceAll) {
      messages = [{ id: newId("user"), role: "user", content: trimmed }];
      replyCodeBlocks = {};
      targetFiles = {};
    } else {
      messages = [...messages, { id: newId("user"), role: "user", content: trimmed }];
    }

    input = "";
    editingId = null;
    editText = "";
    rewindBackup = null;
    loading = true;
    errMsg = null;

    const apiMessages = toApiMessages(messages);
    try {
      const result = await api.aiChatWithValidation(
        apiMessages,
        profileId,
        dangerMode,
        buildWorkspaceContext()
      );
      const assistantIdx = messages.length;
      messages = [
        ...messages,
        { id: newId("assistant"), role: "assistant", content: result.reply }
      ];
      if (result.code_blocks.length > 0) {
        replyCodeBlocks[assistantIdx] = result.code_blocks;
        result.code_blocks.forEach((block, bi) => {
          if (block.category) {
            const match = categories.find((c) => c.category.name === block.category);
            targetFiles[`${assistantIdx}-${bi}`] = match?.file_name ?? categories[0]?.file_name ?? "";
          } else {
            targetFiles[`${assistantIdx}-${bi}`] = categories[0]?.file_name ?? "";
          }
        });
        focusLatestBlocks(assistantIdx, result.code_blocks);
      }
      schedulePersist();
    } catch (e) {
      const msg = String(e);
      if (!msg.includes("已停止生成")) errMsg = msg;
      schedulePersist();
    } finally {
      loading = false;
    }
  }

  async function send() {
    await sendPrompt(input);
  }

  async function stopGeneration() {
    if (!loading) return;
    try {
      await api.aiCancelChat();
    } catch {
      /* ignore */
    }
  }

  function truncateFrom(index: number) {
    messages = messages.slice(0, index);
    const nextBlocks: Record<number, ValidatedCodeBlock[]> = {};
    Object.entries(replyCodeBlocks).forEach(([k, v]) => {
      const n = parseInt(k, 10);
      if (!Number.isNaN(n) && n < index) nextBlocks[n] = v;
    });
    replyCodeBlocks = nextBlocks;
    const nextTargets: Record<string, string> = {};
    Object.entries(targetFiles).forEach(([k, v]) => {
      const msgIdx = parseInt(k.split("-")[0] ?? "", 10);
      if (!Number.isNaN(msgIdx) && msgIdx < index) nextTargets[k] = v;
    });
    targetFiles = nextTargets;
    expandedBlocks = new Set(
      [...expandedBlocks].filter((k) => {
        const msgIdx = parseInt(k.split("-")[0] ?? "", 10);
        return Number.isNaN(msgIdx) || msgIdx < index;
      })
    );
  }

  /** 就地编辑：不删消息，在气泡内改写 */
  async function beginEdit(index: number) {
    if (loading) {
      showToast("请先停止当前生成，再编辑", "info", 2500);
      return;
    }
    const m = messages[index];
    if (!m || m.role !== "user") return;
    editingId = m.id;
    editText = m.content;
    rewindBackup = null;
    errMsg = null;
    showToast("已进入编辑，改完后点「重新发送」", "info", 2500);
    await tick();
    const el = document.getElementById(`dst-edit-${m.id}`) as HTMLTextAreaElement | null;
    el?.focus();
    el?.scrollIntoView({ block: "nearest" });
  }

  function cancelEdit() {
    editingId = null;
    editText = "";
  }

  /** 用编辑后的文本截断并重发 */
  async function confirmEditResend() {
    if (loading || !editingId) return;
    const idx = messages.findIndex((m) => m.id === editingId);
    if (idx < 0) {
      cancelEdit();
      return;
    }
    const text = editText.trim();
    if (!text) {
      showToast("内容不能为空", "error", 2000);
      return;
    }
    truncateFrom(idx);
    editingId = null;
    editText = "";
    showToast("正在按修改后的提示重新发送…", "info", 2000);
    await sendPrompt(text);
  }

  /** 回退到此：删掉该条及之后，原文放入底部输入框待重发 */
  async function rewindTo(index: number) {
    if (loading) {
      showToast("请先停止当前生成，再回退", "info", 2500);
      return;
    }
    const m = messages[index];
    if (!m || m.role !== "user") return;
    rewindBackup = {
      messages: messages.map((x) => ({ ...x })),
      // $state 深层对象是 Proxy，structuredClone 会抛 DataCloneError 导致回退静默失效；
      // $state.snapshot 返回脱离代理的深拷贝
      replyCodeBlocks: $state.snapshot(replyCodeBlocks),
      targetFiles: { ...targetFiles },
      expandedBlocks: [...expandedBlocks],
      input
    };
    editingId = null;
    editText = "";
    truncateFrom(index);
    input = m.content;
    errMsg = null;
    schedulePersist();
    showToast("已回退：修改底部内容后发送，或点「取消回退」", "info", 3500);
    await tick();
    const el = document.getElementById("dst-chat-input") as HTMLTextAreaElement | null;
    el?.focus();
    el?.scrollIntoView({ block: "nearest" });
  }

  function cancelRewind() {
    if (!rewindBackup) return;
    messages = rewindBackup.messages;
    replyCodeBlocks = rewindBackup.replyCodeBlocks;
    targetFiles = rewindBackup.targetFiles;
    expandedBlocks = new Set(rewindBackup.expandedBlocks);
    input = rewindBackup.input;
    rewindBackup = null;
    schedulePersist();
    showToast("已取消回退，对话已恢复", "info", 2000);
  }

  async function apply(block: ValidatedCodeBlock, key: string) {
    if (!block.syntax_ok || !block.safety_ok) return;
    const fileName = targetFiles[key];
    if (!fileName) {
      errMsg = "请选择目标分类";
      return;
    }
    if (dangerMode) {
      dangerInsertConfirm = { block, key, fileName };
      return;
    }
    await doApply(block, key, fileName, false);
  }

  async function doApply(
    block: ValidatedCodeBlock,
    key: string,
    fileName: string,
    useDangerMode: boolean
  ) {
    applying = true;
    applyingKey = key;
    errMsg = null;
    try {
      await onApplyCode(block.code, fileName, useDangerMode);
    } catch (e) {
      errMsg = String(e);
    } finally {
      applying = false;
      applyingKey = null;
    }
  }

  function confirmDangerInsert() {
    const pending = dangerInsertConfirm;
    if (!pending) return;
    dangerInsertConfirm = null;
    void doApply(pending.block, pending.key, pending.fileName, true);
  }

  function cancelDangerInsert() {
    dangerInsertConfirm = null;
  }

  function toggleBlock(key: string) {
    if (expandedBlocks.has(key)) expandedBlocks.delete(key);
    else expandedBlocks.add(key);
    expandedBlocks = new Set(expandedBlocks);
  }

  function canApply(block: ValidatedCodeBlock): boolean {
    // 只有公共命令（小写函数）可插入；仅辅助函数的块插入必然失败
    const exported = block.exported_functions ?? block.functions;
    return block.syntax_ok && block.safety_ok && exported.length > 0;
  }

  /** 标题：公共命令优先；辅助函数单独标注，避免误当作用户命令 */
  function blockTitle(block: ValidatedCodeBlock): string {
    const exported = block.exported_functions ?? block.functions;
    const helpers = block.helper_functions ?? [];
    const base = exported.length > 0 ? exported.join(", ") : "（无公共命令）";
    return helpers.length > 0 ? `${base}（+${helpers.length} 辅助）` : base;
  }

  function scriptIdentity(block: ValidatedCodeBlock): string | null {
    if (block.functions.length === 0) return null;
    return [...block.functions].map((f) => f.toLowerCase()).sort().join(",");
  }

  function normalizeCode(code: string): string {
    return code.replace(/\r\n/g, "\n").trim();
  }

  let latestKeyByScript = $derived.by(() => {
    const map = new Map<string, string>();
    const idxs = Object.keys(replyCodeBlocks)
      .map((k) => parseInt(k, 10))
      .filter((n) => !Number.isNaN(n))
      .sort((a, b) => a - b);
    for (const i of idxs) {
      const blocks = replyCodeBlocks[i] ?? [];
      blocks.forEach((block, bi) => {
        const id = scriptIdentity(block);
        if (id) map.set(id, `${i}-${bi}`);
      });
    }
    return map;
  });

  function isLatestScriptBlock(block: ValidatedCodeBlock, key: string): boolean {
    const id = scriptIdentity(block);
    if (!id) return true;
    return latestKeyByScript.get(id) === key;
  }

  function findEarlierBlock(
    identity: string,
    beforeMsgIdx: number,
    beforeBlockIdx = Infinity
  ): ValidatedCodeBlock | null {
    for (let i = beforeMsgIdx; i >= 0; i--) {
      const blocks = replyCodeBlocks[i];
      if (!blocks) continue;
      const maxBi = i === beforeMsgIdx ? Math.min(beforeBlockIdx, blocks.length) - 1 : blocks.length - 1;
      for (let bi = maxBi; bi >= 0; bi--) {
        if (scriptIdentity(blocks[bi]) === identity) return blocks[bi];
      }
    }
    return null;
  }

  function isUnchangedFromPrevious(
    block: ValidatedCodeBlock,
    msgIdx: number,
    blockIdx: number
  ): boolean {
    const id = scriptIdentity(block);
    if (!id) return false;
    const earlier = findEarlierBlock(id, msgIdx, blockIdx);
    if (!earlier) return false;
    return normalizeCode(earlier.code) === normalizeCode(block.code);
  }

  function focusLatestBlocks(msgIdx: number, blocks: ValidatedCodeBlock[]) {
    const next = new Set(expandedBlocks);
    blocks.forEach((block, bi) => {
      const id = scriptIdentity(block);
      const key = `${msgIdx}-${bi}`;
      if (!id) {
        next.add(key);
        return;
      }
      for (const [iStr, list] of Object.entries(replyCodeBlocks)) {
        list.forEach((b, bix) => {
          const k = `${iStr}-${bix}`;
          if (k !== key && scriptIdentity(b) === id) next.delete(k);
        });
      }
      if (isUnchangedFromPrevious(block, msgIdx, bi)) next.delete(key);
      else next.add(key);
    });
    expandedBlocks = next;
  }

  function textWithoutCode(text: string): string {
    return text.replace(/```[\s\S]*?```/g, "").trim();
  }
</script>

<div class="h-full flex flex-col">
  <div class="px-4 py-2.5 border-b border-dst-border flex items-center justify-between gap-2">
    <div class="flex items-center gap-2 min-w-0">
      <span class="text-lg">🤖</span>
      <h3 class="text-sm font-semibold text-dst-accent shrink-0">AI 助手</h3>
      <span class="text-xs text-dst-fg-muted truncate max-w-[140px]" title={sessionTitle}>{sessionTitle}</span>
      <select
        bind:value={profileId}
        class="text-xs bg-dst-elevated border border-dst-border rounded px-2 py-1 text-dst-fg max-w-[160px]"
        disabled={loading}
        onchange={() => schedulePersist()}>
        {#each profiles as p (p.id)}
          <option value={p.id}>{p.name} · {p.model}</option>
        {/each}
      </select>
    </div>
    <div class="flex items-center gap-2 shrink-0">
      <button
        type="button"
        class="text-xs text-dst-fg-muted hover:text-dst-fg"
        disabled={loading}
        onclick={() => void openResumeList()}
        title="列出历史会话">
        /resume
      </button>
      <button
        type="button"
        class="text-xs text-dst-fg-muted hover:text-dst-fg"
        disabled={loading}
        onclick={() => void startNewSession()}
        title="新建会话">
        /new
      </button>
      <button type="button" class="text-xs text-dst-fg-muted hover:text-dst-fg" onclick={onOpenSettings}>
        配置
      </button>
    </div>
  </div>

  {#if dangerMode}
    <div class="px-4 py-2 bg-dst-danger-bg border-b border-dst-danger-border text-dst-danger-fg text-xs flex items-center justify-between gap-2">
      <span>⚠ 危险模式：本会话已放宽安全红线（reset --hard / force-push / 真实 clean 等）</span>
      <button
        type="button"
        class="shrink-0 px-2 py-0.5 rounded border border-dst-danger-border hover:bg-dst-danger-bg"
        onclick={() => {
          dangerMode = false;
          schedulePersist();
          showToast("已恢复安全模式", "success", 2000);
        }}>
        /safe
      </button>
    </div>
  {/if}

  {#if resumePick}
    <div class="px-4 py-3 bg-dst-elevated border-b border-dst-border text-dst-fg text-xs whitespace-pre-wrap font-mono leading-relaxed">
      {resumePick.listText}
      <p class="mt-2 text-dst-fg-muted font-sans">提示：输入编号恢复；/cancel 或连按两次 Esc 取消</p>
    </div>
  {/if}

  {#if errMsg}
    <div class="px-4 py-2 bg-dst-danger-bg border-b border-dst-danger-border text-dst-danger-fg text-xs flex justify-between gap-2">
      <span class="break-words min-w-0">{errMsg}</span>
      <button type="button" class="text-dst-danger shrink-0" onclick={() => (errMsg = null)}>×</button>
    </div>
  {/if}

  <div class="flex-1 overflow-y-auto px-4 py-4 space-y-4">
    {#if messages.length === 0 && !loading && !resumePick}
      <div class="flex flex-col items-center justify-center h-full text-center gap-3">
        <div class="text-4xl">🤖</div>
        <p class="text-sm text-dst-fg-muted">可直接提问，或从命令列表点「AI审阅」</p>
        <p class="text-xs text-dst-fg-subtle">审阅流程：检查问题 → 优化建议 → 可新增命令建议；校验通过后可插入分类</p>
        <p class="text-xs text-dst-fg-subtle">
          会话会自动保存；输入 <code class="text-dst-accent">/resume</code> 按编号恢复，
          <code class="text-dst-fg-muted">/new</code> 新建，
          <code class="text-dst-warning">/danger</code> 放宽红线
        </p>
      </div>
    {/if}

    {#each messages as m, i (m.id)}
      {#if m.role === "user"}
        <div class="flex justify-end gap-2" data-msg-id={m.id}>
          <div class="flex flex-col items-end gap-1.5 max-w-[85%] min-w-0 w-full">
            {#if editingId === m.id}
              <!-- 就地编辑：气泡内改写，不依赖底部输入框 -->
              <div class="w-full rounded-lg border-2 border-dst-warning bg-dst-surface p-2 space-y-2">
                <p class="text-xs text-dst-warning-fg px-1">编辑此条提示词，确认后将删除其后的回复并重新发送</p>
                <textarea
                  id="dst-edit-{m.id}"
                  bind:value={editText}
                  rows={8}
                  class="w-full px-3 py-2 text-sm bg-dst-bg border border-dst-warning rounded-lg text-dst-fg focus:outline-none focus:border-dst-warning resize-y min-h-[8rem]"
                  disabled={loading}></textarea>
                <div class="flex gap-2 justify-end">
                  <button
                    type="button"
                    class="px-3 py-1.5 text-xs bg-dst-muted hover:bg-dst-muted rounded"
                    onclick={cancelEdit}
                    disabled={loading}>
                    取消
                  </button>
                  <button
                    type="button"
                    class="px-3 py-1.5 text-xs bg-dst-accent text-dst-accent-fg hover:bg-dst-accent-hover rounded disabled:opacity-50"
                    onclick={confirmEditResend}
                    disabled={loading || !editText.trim()}>
                    重新发送
                  </button>
                </div>
              </div>
            {:else}
              <div class="bg-dst-bubble-user rounded-lg rounded-tr-sm px-3 py-2 text-sm text-dst-bubble-user-fg whitespace-pre-wrap break-words max-h-64 overflow-y-auto">
                {m.content}
              </div>
              <div class="flex gap-2 items-center relative z-10">
                <button
                  type="button"
                  class="px-2.5 py-1 text-xs rounded bg-dst-elevated border border-dst-accent text-dst-accent hover:bg-dst-menu-hover hover:border-dst-accent disabled:opacity-40"
                  disabled={loading || editingId !== null}
                  onclick={() => beginEdit(i)}>
                  编辑
                </button>
                <button
                  type="button"
                  class="px-2.5 py-1 text-xs rounded bg-dst-elevated border border-dst-warning text-dst-warning-fg hover:bg-dst-warning-bg hover:border-dst-warning disabled:opacity-40"
                  disabled={loading || editingId !== null}
                  onclick={() => rewindTo(i)}>
                  回退到此
                </button>
              </div>
            {/if}
          </div>
          <div class="w-8 h-8 rounded-full bg-dst-accent text-dst-accent-fg border border-dst-accent flex items-center justify-center text-sm shrink-0">
            👤
          </div>
        </div>
      {:else}
        <div class="flex justify-start gap-2">
          <div class="w-8 h-8 rounded-full bg-dst-success-bg border border-dst-success flex items-center justify-center text-sm shrink-0">
            🤖
          </div>
          <div class="flex flex-col gap-2 max-w-[85%] min-w-0">
            {#if textWithoutCode(m.content)}
              <div class="bg-dst-bubble-ai rounded-lg rounded-tl-sm px-3 py-2 text-dst-bubble-ai-fg">
                <MarkdownText source={textWithoutCode(m.content)} />
              </div>
            {/if}
            {#if replyCodeBlocks[i]}
              {#each replyCodeBlocks[i] as block, bi}
                {@const key = `${i}-${bi}`}
                {@const latest = isLatestScriptBlock(block, key)}
                {@const unchanged = latest && isUnchangedFromPrevious(block, i, bi)}
                {@const title = blockTitle(block)}
                {#if !latest}
                  <div class="border border-dst-border rounded-lg overflow-hidden opacity-70">
                    <div class="flex items-center justify-between px-3 py-1.5 bg-dst-surface gap-2">
                      <div class="flex items-center gap-2 text-xs min-w-0">
                        <span class="text-dst-accent font-mono truncate">{title}</span>
                        <span class="text-dst-fg-muted shrink-0">已有更新版本</span>
                      </div>
                      <button type="button" class="text-xs text-dst-fg-muted hover:text-dst-fg shrink-0" onclick={() => toggleBlock(key)}>
                        {expandedBlocks.has(key) ? "收起旧稿" : "查看旧稿"}
                      </button>
                    </div>
                    {#if expandedBlocks.has(key)}
                      <pre class="px-3 py-2 text-xs text-dst-fg-muted overflow-x-auto whitespace-pre-wrap font-mono bg-dst-bg/40 border-t border-dst-border">{block.code}</pre>
                    {/if}
                  </div>
                {:else if unchanged}
                  <div class="border border-dst-border rounded-lg px-3 py-1.5 flex items-center justify-between gap-2 bg-dst-elevated">
                    <div class="flex items-center gap-2 text-xs min-w-0">
                      <span class="text-dst-accent font-mono truncate">{title}</span>
                      <span class="text-dst-fg-muted">与上一版相同 · {block.syntax_ok ? "语法✓" : "语法✗"} · {block.safety_ok ? "安全✓" : "安全✗"}</span>
                    </div>
                    <button type="button" class="text-xs text-dst-fg-muted hover:text-dst-fg shrink-0" onclick={() => toggleBlock(key)}>
                      {expandedBlocks.has(key) ? "收起" : "查看"}
                    </button>
                  </div>
                  {#if expandedBlocks.has(key)}
                    <pre class="mt-0 border border-t-0 border-dst-border rounded-b-lg px-3 py-2 text-xs text-dst-fg-muted overflow-x-auto whitespace-pre-wrap font-mono bg-dst-bg/40">{block.code}</pre>
                  {/if}
                {:else}
                  <div class="border border-dst-border rounded-lg overflow-hidden">
                    <div
                      class="flex items-center justify-between px-3 py-1.5 bg-dst-elevated cursor-pointer hover:bg-dst-muted"
                      role="button"
                      tabindex="0"
                      onclick={() => toggleBlock(key)}
                      onkeydown={(e) => e.key === "Enter" && toggleBlock(key)}>
                      <div class="flex items-center gap-2 text-xs">
                        <span class="text-dst-fg-muted font-mono">{expandedBlocks.has(key) ? "▾" : "▸"}</span>
                        <span class="text-dst-accent font-mono">{title}</span>
                        <span class="text-dst-success">最新</span>
                        <span class="text-dst-fg-muted">{block.syntax_ok ? "语法✓" : "语法✗"} · {block.safety_ok ? "安全✓" : "安全✗"}</span>
                      </div>
                    </div>
                    {#if expandedBlocks.has(key)}
                      <div class="px-3 py-2 bg-dst-bg/50 space-y-2">
                        <pre class="text-xs text-dst-fg overflow-x-auto whitespace-pre-wrap font-mono">{block.code}</pre>
                        {#if !block.syntax_ok || !block.safety_ok}
                          <p class="text-xs text-dst-warning-fg">
                            {[block.syntax_err, ...block.safety_violations].filter(Boolean).join("；") || "未通过校验"}
                          </p>
                        {:else if (block.quality_warnings ?? []).length > 0}
                          <p class="text-xs text-dst-warning-fg">
                            ⚠ 质量提醒：{block.quality_warnings.join("；")}
                          </p>
                        {/if}
                        <div class="flex items-center gap-2">
                          <select bind:value={targetFiles[key]} class="text-xs bg-dst-elevated border border-dst-border rounded px-2 py-1">
                            {#each categories as c}
                              <option value={c.file_name}>{c.category.title}</option>
                            {/each}
                          </select>
                          <button
                            type="button"
                            class="px-2 py-1 text-xs bg-dst-btn-success text-dst-btn-success-fg hover:opacity-90 rounded disabled:opacity-40"
                            disabled={!canApply(block) || applying}
                            onclick={() => apply(block, key)}>
                            {applyingKey === key ? "插入中…" : "插入到分类"}
                          </button>
                        </div>
                      </div>
                    {/if}
                  </div>
                {/if}
              {/each}
            {/if}
          </div>
        </div>
      {/if}
    {/each}

    {#if loading}
      <div class="flex justify-start gap-2">
        <div class="w-8 h-8 rounded-full bg-dst-success-bg border border-dst-success flex items-center justify-center text-sm shrink-0">
          🤖
        </div>
        <div class="flex items-center gap-3 bg-dst-elevated rounded-lg rounded-tl-sm px-3 py-2">
          <div class="flex items-center gap-2">
            <span class="inline-block w-2 h-2 bg-dst-success rounded-full animate-bounce" style="animation-delay: 0ms"></span>
            <span class="inline-block w-2 h-2 bg-dst-success rounded-full animate-bounce" style="animation-delay: 150ms"></span>
            <span class="inline-block w-2 h-2 bg-dst-success rounded-full animate-bounce" style="animation-delay: 300ms"></span>
            <span class="text-xs text-dst-fg-muted ml-1">正在生成…</span>
          </div>
          <button
            type="button"
            class="px-2 py-0.5 text-xs rounded border border-dst-danger-border text-dst-danger hover:bg-dst-danger-bg"
            onclick={stopGeneration}>
            停止
          </button>
        </div>
      </div>
    {/if}
  </div>

  <div class="p-3 border-t border-dst-border space-y-2 shrink-0">
    {#if rewindBackup}
      <div class="flex items-center justify-between gap-2 px-1">
        <p class="text-xs text-dst-warning-fg">已回退：编辑下方内容后发送，将从该条继续</p>
        <button type="button" class="text-xs text-dst-fg-muted hover:text-dst-fg shrink-0" onclick={cancelRewind} disabled={loading}>
          取消回退
        </button>
      </div>
    {/if}
    {#if editingId}
      <p class="text-xs text-dst-warning-fg px-1">正在编辑上方消息 — 请在气泡内点「重新发送」</p>
    {/if}
    <div class="flex gap-2 items-end">
      <div class="relative flex-1 min-w-0">
        {#if slashOpen}
          <div
            class="absolute bottom-full left-0 right-0 mb-1 z-30 rounded-lg border border-dst-border bg-dst-surface shadow-lg overflow-hidden max-h-56 overflow-y-auto"
            role="listbox"
            aria-label="斜杠命令联想">
            {#each slashMatches as cmd, i (cmd.name)}
              <button
                type="button"
                role="option"
                aria-selected={i === slashIndex}
                class="w-full flex items-baseline gap-3 px-3 py-2 text-left text-sm transition-colors {i === slashIndex
                  ? 'bg-dst-menu-hover'
                  : 'hover:bg-dst-elevated'}"
                onmousedown={(e) => {
                  // 避免抢焦点导致 textarea blur 后 Enter 异常
                  e.preventDefault();
                  acceptSlash(cmd);
                }}>
                <code class="font-mono text-dst-accent shrink-0">{cmd.name}</code>
                <span class="text-xs text-dst-fg-muted truncate">{cmd.description}</span>
              </button>
            {/each}
            <div class="px-3 py-1.5 text-[10px] text-dst-fg-subtle border-t border-dst-border bg-dst-elevated/50">
              ↑↓ 选择 · Tab/Enter 填入 · Esc 关闭
            </div>
          </div>
        {/if}
        <textarea
          id="dst-chat-input"
          bind:value={input}
          rows={rewindBackup ? 5 : 2}
          onkeydown={(e) => {
            if (slashOpen) {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                slashIndex = (slashIndex + 1) % slashMatches.length;
                return;
              }
              if (e.key === "ArrowUp") {
                e.preventDefault();
                slashIndex =
                  (slashIndex - 1 + slashMatches.length) % slashMatches.length;
                return;
              }
              if (e.key === "Tab") {
                e.preventDefault();
                acceptSlash();
                return;
              }
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                acceptSlash();
                return;
              }
              if (e.key === "Escape") {
                e.preventDefault();
                e.stopPropagation();
                closeSlashPalette();
                return;
              }
            }
            // Esc 选号：由 window 统一处理（避免冒泡导致一次按键计两次）
            if (e.key === "Enter" && !e.shiftKey && !loading && !editingId) {
              e.preventDefault();
              void send();
            }
          }}
          placeholder={resumePick
            ? `编号 1–${resumePick.summaries.length || 0} 恢复 · /cancel 或 Esc×2 取消`
            : rewindBackup
              ? "回退后可修改，然后发送…"
              : dangerMode
                ? "危险模式已开…（/safe 关闭）"
                : "继续追问…（输入 / 可联想命令 · Shift+Enter 换行）"}
          class="w-full px-3 py-2 text-sm bg-dst-elevated border rounded-lg text-dst-fg focus:outline-none resize-y min-h-[2.5rem] max-h-48 {resumePick
            ? 'border-dst-accent focus:border-dst-accent'
            : rewindBackup
              ? 'border-dst-warning focus:border-dst-warning'
              : dangerMode
                ? 'border-dst-danger-border focus:border-dst-danger-border'
                : 'border-dst-border focus:border-dst-accent'}"
          disabled={loading || !profileId || editingId !== null}></textarea>
      </div>
      {#if loading}
        <button
          type="button"
          class="px-4 py-2 text-sm bg-dst-btn-danger text-dst-btn-danger-fg hover:opacity-90 rounded-lg shrink-0"
          onclick={stopGeneration}>
          停止
        </button>
      {:else}
        <button
          type="button"
          class="px-4 py-2 text-sm bg-dst-accent text-dst-accent-fg hover:bg-dst-accent-hover rounded-lg disabled:opacity-50 shrink-0"
          onclick={send}
          disabled={!input.trim() || !profileId || editingId !== null}>
          发送
        </button>
      {/if}
    </div>
  </div>
</div>

<ConfirmDialog
  open={dangerInsertConfirm !== null}
  title="危险模式插入确认"
  message="当前为危险模式，即将插入可能含破坏性操作的代码（如 git reset --hard、force-push 等）。确认继续？"
  confirmText="确认插入"
  cancelText="取消"
  tone="danger"
  busy={applying}
  onConfirm={confirmDangerInsert}
  onCancel={cancelDangerInsert} />
