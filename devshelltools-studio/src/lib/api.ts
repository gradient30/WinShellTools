import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface WorkspaceStatus {
  initialized: boolean;
  root: string;
  version: string;
  template_version: string;
  created_at: string;
  last_sync: string;
  missing_files: string[];
  public_files: string[];
}

export interface InitProgress {
  step: number;
  label: string;
  percent: number;
}

export interface PsParam {
  name: string;
  type_name: string;
  default_value: string | null;
  mandatory: boolean;
  position: number | null;
  is_switch: boolean;
  description: string;
}

export interface PsFunction {
  name: string;
  synopsis: string;
  first_example: string;
  examples?: string[];
  parameters?: PsParam[];
}

export interface CategoryMeta {
  name: string;
  title: string;
  description: string;
  aliases: string[];
}

export interface CategoryInfo {
  file_name: string;
  category: CategoryMeta;
  functions: PsFunction[];
}

export interface ListCategoriesResult {
  categories: CategoryInfo[];
  cached: boolean;
}

export interface ConsistencyReport {
  ok: boolean;
  errors: string[];
  warnings: string[];
  actual_functions: string[];
  psd1_exports: string[];
  psm1_exports: string[];
  help_commands: string[];
}

export interface SafetyReport {
  ok: boolean;
  violations: string[];
}

export type AiProtocol = "openai" | "anthropic";

export interface AiConfig {
  protocol: AiProtocol;
  base_url: string;
  model: string;
  temperature: number;
  max_tokens: number;
}

export interface AiProfile {
  id: string;
  name: string;
  protocol: AiProtocol;
  base_url: string;
  model: string;
  temperature: number;
  max_tokens: number;
  key_configured: boolean;
}

export interface AiKeyStatus {
  configured: boolean;
  masked: string;
}

export interface ChatMessage {
  role: string;
  content: string;
}

export interface ValidatedCodeBlock {
  code: string;
  syntax_ok: boolean;
  syntax_err: string;
  safety_ok: boolean;
  safety_violations: string[];
  /** 块内全部函数（兼容保留） */
  functions: string[];
  /** 公共命令（小写函数名）——用户在终端真正可敲的命令 */
  exported_functions: string[];
  /** 内部辅助函数（大写开头，随命令写入但不作为命令汇报） */
  helper_functions: string[];
  /** 质量验收警告：缺 SYNOPSIS/EXAMPLE、命名不规范、无公共命令、与已有命令重名 */
  quality_warnings: string[];
  category: string | null;
}

export interface AiChatResult {
  reply: string;
  code_blocks: ValidatedCodeBlock[];
}

export interface SessionMessage {
  id: string;
  role: string;
  content: string;
}

export interface ChatSession {
  id: string;
  title: string;
  created_at: string;
  updated_at: string;
  profile_id: string;
  danger_mode: boolean;
  messages: SessionMessage[];
  reply_code_blocks: Record<string, ValidatedCodeBlock[]>;
  target_files: Record<string, string>;
}

export interface SessionSummary {
  id: string;
  title: string;
  updated_at: string;
  message_count: number;
  preview: string;
}

export interface ResumeListResult {
  summaries: SessionSummary[];
  list_text: string;
}

export interface MigrationCheck {
  has_legacy: boolean;
  legacy_dirs: string[];
  workspace_initialized: boolean;
}

export interface MigrateResult {
  migrated_files: string[];
  archived_dirs: string[];
  message: string;
}

export interface Webview2Status {
  installed: boolean;
  version: string;
  needs_guidance: boolean;
}

export interface ImportResult {
  imported: string[];
  skipped: string[];
  errors: string[];
}

// ============ 完整配置包（双向同步 / 迁移） ============

export type BundleKind = "category" | "private" | "ai";
export type BundleAction = "add" | "overwrite" | "same" | "keep_local" | "invalid";

export interface ExportBundleResult {
  /** 写出的文件（含 manifest.json，相对包目录） */
  files: string[];
  /** 随包导出的 AI Profile 数（0 = 无配置未打包） */
  ai_profiles: number;
  target_dir: string;
}

export interface BundleItem {
  rel: string;
  kind: BundleKind;
  action: BundleAction;
  checked: boolean;
  commands: number;
  syntax_ok: boolean;
  safety_ok: boolean;
  note: string;
}

export interface BundlePreview {
  manifest_found: boolean;
  template_version: string;
  exported_at: string;
  items: BundleItem[];
  ai_new: number;
  ai_existing: number;
}

export interface ImportBundleResult {
  imported: string[];
  ai_added: number;
  /** 是否已重生成 psd1/psm1/Help.ps1 */
  regenerated: boolean;
  /** 是否已触发后台运行时目录同步 */
  runtime_synced: boolean;
  errors: string[];
}

export interface InstallStatus {
  workspace_ready: boolean;
  ps51_module_present: boolean;
  ps7_module_present: boolean;
  profile_configured: boolean;
  installed: boolean;
}

export interface InstallResult {
  status: InstallStatus;
  message: string;
  verified: boolean;
}

export interface AiPreset {
  id: string;
  name: string;
  openai_base_url: string;
  anthropic_base_url: string;
  openai_default_model: string;
  anthropic_default_model: string;
  supports_anthropic: boolean;
}

export interface AiEndpointSuggestion {
  base_url: string;
  default_model: string;
  protocol: AiProtocol;
  note: string;
}

export interface FunctionTestResult {
  ok: boolean;
  stdout: string;
  stderr: string;
}

/** AI 插入结果：命令名 + 首个新命令的隔离自测结果 */
export interface ApplyAiResult {
  names: string[];
  self_test: FunctionTestResult | null;
}

export const api = {
  workspaceStatus: () => invoke<WorkspaceStatus>("workspace_status"),
  initWorkspace: () => invoke<string>("init_workspace"),
  onInitProgress: (handler: (p: InitProgress) => void) =>
    listen<InitProgress>("init-progress", (e) => handler(e.payload)),
  listPublicFiles: () => invoke<string[]>("list_public_files"),
  readWorkspaceFile: (rel: string) => invoke<string>("read_workspace_file", { rel }),
  listCategories: () => invoke<ListCategoriesResult>("list_categories"),
  readCategoryFile: (fileName: string) => invoke<string>("read_category_file", { fileName }),
  writeWorkspaceFile: (rel: string, content: string, message: string) =>
    invoke<void>("write_workspace_file", { rel, content, message }),
  deleteWorkspaceFile: (rel: string, message: string) =>
    invoke<void>("delete_workspace_file", { rel, message }),
  createCategory: (fileName: string, content: string, message: string) =>
    invoke<void>("create_category", { fileName, content, message }),
  deleteCategory: (fileName: string, message: string) =>
    invoke<void>("delete_category", { fileName, message }),
  updateCategoryFile: (
    fileName: string,
    content: string,
    message: string,
    acknowledgeDanger?: boolean
  ) =>
    invoke<void>("update_category_file", {
      fileName,
      content,
      acknowledgeDanger: acknowledgeDanger ?? false,
      message
    }),
  syncPublic: (message: string) => invoke<void>("sync_public", { message }),
  upsertFunction: (
    fileName: string,
    name: string,
    synopsis: string,
    example: string,
    body: string | null,
    message: string,
    paramDefaults?: Record<string, string> | null,
    acknowledgeDanger?: boolean
  ) =>
    invoke<void>("upsert_function", {
      fileName,
      name,
      synopsis,
      example,
      body,
      paramDefaults: paramDefaults ?? null,
      acknowledgeDanger: acknowledgeDanger ?? false,
      message
    }),
  deleteFunction: (fileName: string, funcName: string, message: string) =>
    invoke<void>("delete_function", { fileName, funcName, message }),
  testFunction: (fileName: string, funcName: string) =>
    invoke<FunctionTestResult>("test_function", { fileName, funcName }),
  applyAiCode: (fileName: string, code: string, message: string, dangerMode = false) =>
    invoke<ApplyAiResult>("apply_ai_code", {
      fileName,
      code,
      message,
      dangerMode
    }),
  installStatus: () => invoke<InstallStatus>("install_status"),
  installModule: () => invoke<InstallResult>("install_module"),
  uninstallModule: () => invoke<InstallResult>("uninstall_module"),
  consistencyCheck: () => invoke<ConsistencyReport>("consistency_check"),
  safetyCheck: (code: string) => invoke<SafetyReport>("safety_check", { code }),
  validatePsSyntax: (code: string) => invoke<void>("validate_ps_syntax", { code }),
  getAiConfig: () => invoke<AiConfig>("get_ai_config"),
  saveAiConfig: (config: AiConfig) => invoke<void>("save_ai_config", { config }),
  saveAiKey: (key: string) => invoke<void>("save_ai_key", { key }),
  getAiKeyStatus: () => invoke<AiKeyStatus>("get_ai_key_status"),
  aiReady: () => invoke<boolean>("ai_ready"),
  listAiProfiles: () => invoke<AiProfile[]>("list_ai_profiles"),
  getAiProfilesMeta: () =>
    invoke<{ profiles: AiProfile[]; default_profile_id: string | null }>("get_ai_profiles_meta"),
  saveAiProfile: (profile: AiProfile, key?: string) =>
    invoke<AiProfile>("save_ai_profile", { input: { profile, key: key ?? null } }),
  deleteAiProfile: (id: string) => invoke<void>("delete_ai_profile", { id }),
  setDefaultAiProfile: (id: string) => invoke<void>("set_default_ai_profile", { id }),
  testAiProfile: (id: string) => invoke<string>("test_ai_profile", { id }),
  listAiPresets: () => invoke<AiPreset[]>("list_ai_presets"),
  suggestAiEndpoint: (protocol: AiProtocol, currentBaseUrl?: string) =>
    invoke<AiEndpointSuggestion>("suggest_ai_endpoint", {
      protocol,
      currentBaseUrl: currentBaseUrl ?? null
    }),
  fetchAiModels: (id: string) => invoke<string[]>("fetch_ai_models", { id }),
  fetchAiModelsPreview: (protocol: AiProtocol, baseUrl: string, key: string) =>
    invoke<string[]>("fetch_ai_models_preview", {
      input: { protocol, base_url: baseUrl, key }
    }),
  aiChat: (messages: ChatMessage[], profileId?: string, dangerMode = false, workspaceContext?: string) =>
    invoke<string>("ai_chat", {
      messages,
      profileId: profileId ?? null,
      dangerMode,
      workspaceContext: workspaceContext ?? null
    }),
  /** 中断进行中的 AI 请求 */
  aiCancelChat: () => invoke<void>("ai_cancel_chat"),
  aiChatWithValidation: (
    messages: ChatMessage[],
    profileId?: string,
    dangerMode = false,
    workspaceContext?: string
  ) =>
    invoke<AiChatResult>("ai_chat_with_validation", {
      messages,
      profileId: profileId ?? null,
      dangerMode,
      workspaceContext: workspaceContext ?? null
    }),
  listChatSessions: () => invoke<ResumeListResult>("list_chat_sessions"),
  loadChatSession: (id: string) => invoke<ChatSession>("load_chat_session", { id }),
  saveChatSession: (session: ChatSession) =>
    invoke<ChatSession>("save_chat_session", { session }),
  newChatSession: (profileId: string) =>
    invoke<ChatSession>("new_chat_session", { profileId }),
  loadOrCreateChatSession: (profileId: string) =>
    invoke<ChatSession>("load_or_create_chat_session", { profileId }),
  setActiveChatSession: (id: string) =>
    invoke<void>("set_active_chat_session", { id }),
  checkMigration: () => invoke<MigrationCheck>("check_migration"),
  migrateLegacy: () => invoke<MigrateResult>("migrate_legacy"),
  exportWorkspace: (targetDir: string) => invoke<string[]>("export_workspace", { targetDir }),
  importWorkspace: (sourceDir: string) => invoke<ImportResult>("import_workspace", { sourceDir }),
  exportBundle: (targetDir: string) => invoke<ExportBundleResult>("export_bundle", { targetDir }),
  previewBundle: (sourceDir: string) => invoke<BundlePreview>("preview_bundle", { sourceDir }),
  importBundle: (sourceDir: string, selection: string[]) =>
    invoke<ImportBundleResult>("import_bundle", { sourceDir, selection }),
  listLogs: () => invoke<string[]>("list_logs"),
  readLog: (name: string) => invoke<string>("read_log", { name }),
  webview2Status: () => invoke<Webview2Status>("webview2_status"),
  webview2DownloadUrl: () => invoke<string>("webview2_download_url")
};
