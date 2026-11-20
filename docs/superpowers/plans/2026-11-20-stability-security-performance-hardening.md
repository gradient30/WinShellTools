# DevShellTools Stability, Security, and Performance Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the approved P0/P1/P2 hardening without regressing existing editing, installation, AI, or bundle workflows.

**Architecture:** Use a progressive hardening route. Centralize PowerShell-safe rendering, workspace path resolution, process execution, key protection, file transactions, runtime synchronization, and AI streaming; then split oversized modules only where the new boundaries are stable.

**Tech Stack:** PowerShell 5.1/7, Rust 2021, Tauri 2, Svelte 5, TypeScript, Vite 5, Windows DPAPI, offline Cargo builds.

**Spec:** `docs/superpowers/specs/2026-11-20-stability-security-performance-hardening-design.md`

## Global Constraints

- Windows PowerShell 5.1 and PowerShell 7 remain supported.
- Studio must not require administrator privileges or modify machine-level state.
- `DST_MY_DOCUMENTS` remains the test workspace override.
- `CARGO_NET_OFFLINE=true` builds and tests must continue to work.
- Existing Tauri command names used by the frontend must remain stable unless the command is intentionally deleted as an unused high-risk surface.
- No version number changes until all phases pass and a release is explicitly prepared.
- No user data is deleted before a replacement has been durably written and validated.
- Editing returns after the durable workspace transaction; runtime copy synchronization remains asynchronous.
- Security failures return actionable errors and must not silently fall back to unsafe execution.
- Every task follows red-green-refactor and ends with its focused verification command.

## Review Focus

- Untrusted metadata containing `$()`, backticks, quotes, semicolons, or newlines must remain inert data in generated PowerShell.
- Dynamically constructed PowerShell commands must not bypass the safety policy through quoting, git global options, variables, or `Invoke-Expression`.
- Concurrent edits, partial writes, and process termination must never leave a mixed module generation without a recovery path.
- DPAPI migration or AI streaming failures must leave the user with a working, understandable fallback instead of losing keys, sessions, or UI control.
- The asynchronous runtime-sync queue must not report success while a stale copy remains installed.

---

### Task 1: PowerShell-Safe Rendering and Metadata Validation

**Files:**
- Create: `devshelltools-studio/src-tauri/src/ps_render.rs`
- Modify: `devshelltools-studio/src-tauri/src/lib.rs`
- Modify: `devshelltools-studio/src-tauri/src/sync.rs`
- Modify: `devshelltools-studio/src-tauri/src/ps_parser.rs`
- Test: inline `#[cfg(test)]` modules in the changed files

**Interfaces:**
- Produces: `pub fn quote_data(value: &str) -> String`
- Produces: `pub fn validate_category_name(name: &str) -> DstResult<()>`
- Produces: `fn replace_block(src: &str, tag: &str, new_body: &str) -> DstResult<String>`
- Consumes: `DstResult<T>` from `error.rs`

- [ ] **Step 1: Write failing tests for literal encoding and missing markers**

```rust
#[test]
fn data_literal_neutralizes_powershell_expansion() {
    let value = "x'; $(Write-Output injected); `whoami`";
    assert_eq!(quote_data(value), "'x''; $(Write-Output injected); `whoami`'");
}

#[test]
fn replace_block_returns_error_when_marker_is_missing() {
    assert!(replace_block("function a {}", "HelpData", "x").is_err());
}

#[test]
fn category_name_rejects_shell_syntax() {
    assert!(validate_category_name("files").is_ok());
    assert!(validate_category_name("files; whoami").is_err());
}
```

- [ ] **Step 2: Run the focused tests and confirm they fail**

Run: `cargo test --offline ps_render::tests -- --test-threads=1`
Expected: compilation failure because `ps_render.rs` and the new interfaces do not exist.

- [ ] **Step 3: Implement the rendering primitives and replace string assembly**

Use single-quoted PowerShell data literals; double embedded single quotes. Validate category identifiers with `^[a-z][a-z0-9]*$`. Change `replace_block` to return `DstResult<String>` and make both missing start/end markers errors. Replace raw `c.category.name`, title, aliases, synopsis, and example interpolation with encoded values. Validate category metadata immediately after AST parsing.

- [ ] **Step 4: Add an injection regression test through `regenerate_all`**

Create a temporary isolated category whose title is `$(Write-Output DST_INJECTED)` and whose aliases contain quotes/backticks. Regenerate, parse `Help.ps1`, and assert syntax remains valid and no AST command contains `DST_INJECTED`.

- [ ] **Step 5: Run focused and full backend tests**

Run: `cargo test --offline --lib -- --test-threads=1`
Expected: all existing tests plus the new rendering tests pass.

---

### Task 2: AST-Based Safety Policy

**Files:**
- Modify: `devshelltools-studio/src-tauri/src/ps_parser.rs`
- Modify: `devshelltools-studio/src-tauri/src/safety.rs`
- Modify: `devshelltools-studio/src-tauri/src/commands.rs`
- Modify: `devshelltools-studio/src-tauri/src/export.rs`
- Modify: `devshelltools-studio/src-tauri/src/function_edit.rs`
- Test: inline `safety.rs` and `ps_parser.rs` tests plus `tests/p0_security_acceptance.rs`

**Interfaces:**
- Produces: `pub struct PsCommand { pub name: String, pub args: Vec<String>, pub dynamic: bool, pub within_should_process: bool }`
- Produces: `pub fn check_parsed(parsed: &ParsedPsFile) -> DstResult<SafetyReport>`
- Preserves: `pub fn check(code: &str) -> DstResult<SafetyReport>` as a convenience API that uses the same semantic policy.

- [ ] **Step 1: Write failing semantic-policy tests**

```rust
#[test]
fn git_global_options_do_not_bypass_hard_reset_rule() {
    let report = check("& git -c x=y reset --hard").unwrap();
    assert!(!report.ok);
}

#[test]
fn dynamic_invocation_is_blocked_by_default() {
    let report = check("$cmd = 'git'; & $cmd reset --hard").unwrap();
    assert!(!report.ok);
}

#[test]
fn same_call_remove_recursion_and_force_is_blocked() {
    let report = check("Remove-Item -LiteralPath $p -Recurse -Force").unwrap();
    assert!(!report.ok);
}
```

- [ ] **Step 2: Run the tests and verify current substring checks fail the bypass cases**

Run: `cargo test --offline safety::tests -- --test-threads=1`
Expected: at least the git-global-options and dynamic-invocation tests fail.

- [ ] **Step 3: Extend the PowerShell AST helper to emit command structure**

Collect `CommandAst` name, literal arguments, dynamic flag, and whether the nearest enclosing function declares `SupportsShouldProcess`. Deserialize with serde defaults so older cache data remains readable.

- [ ] **Step 4: Implement semantic rules**

Resolve git subcommands after global options; reject force-push variants, hard reset, and real clean. Reject dynamic command expressions, `Invoke-Expression`, user-scope environment writes, unconfirmed `Stop-Process`, and same-call recursive forced deletion unless `danger_mode` is active.

- [ ] **Step 5: Replace text-only enforcement at all write boundaries**

Parse once, then call `check_parsed` in category save/create, function upsert/apply, bundle preview/import, and AI code validation. Retain a documented fast precheck only as an optimization.

- [ ] **Step 6: Run security and full backend tests**

Run: `cargo test --offline --test p0_security_acceptance -- --test-threads=1`
Run: `cargo test --offline --lib -- --test-threads=1`
Expected: all pass.

---

### Task 3: Explicit AI Example Testing and Built-in Redlines

**Files:**
- Modify: `devshelltools-studio/src-tauri/src/commands.rs`
- Modify: `devshelltools-studio/src-tauri/src/function_edit.rs`
- Modify: `devshelltools-studio/src/lib/api.ts`
- Modify: `devshelltools-studio/src/App.svelte`
- Modify: `devshelltools-studio/src/lib/components/ChatPanel.svelte`
- Modify: `Public/Git.ps1`
- Modify: `Public/Proxy.ps1`
- Modify: `Public/Help.ps1`
- Modify: `README.md`
- Modify: `devshelltools-studio/templates/Public/Git.ps1`
- Modify: `devshelltools-studio/templates/Public/Proxy.ps1`
- Modify: `devshelltools-studio/templates/Public/Help.ps1`
- Test: `devshelltools-studio/src-tauri/tests/p0_security_acceptance.rs`

**Interfaces:**
- Produces: `ApplyAiResult { names: Vec<String> }`; insertion no longer schedules a self-test.
- Preserves: `test_function(file_name: String, func_name: String) -> DstResult<FunctionTestResult>` for explicit execution.

- [ ] **Step 1: Write failing module-policy acceptance tests**

```rust
#[test]
fn gpp_rejects_force_before_git_runs() {
    let script = "Import-Module '<manifest>'; try { gpp --force; exit 9 } catch { exit 0 }";
    assert_success(script);
}

#[test]
fn lpr_clean_user_is_not_a_supported_action() {
    let script = "Import-Module '<manifest>'; try { lpr clean-user; exit 9 } catch { exit 0 }";
    assert_success(script);
}
```

- [ ] **Step 2: Run acceptance tests and confirm they fail**

Run: `cargo test --offline --test p0_security_acceptance -- --test-threads=1`
Expected: force push and clean-user tests fail against current scripts.

- [ ] **Step 3: Enforce redlines in root and template module sources**

Reject all `--force*` and `-f` arguments in `gpp`. Remove the `clean-user` action from `lpr`, Help and README. Keep `doctor` read-only and show a warning when user-scope proxy variables exist.

- [ ] **Step 4: Remove automatic example execution and add explicit UI feedback**

After insertion, show the inserted names and a “测试首个示例” action. Before invoking, display the first example line and target file. No code is executed merely because insertion succeeded.

- [ ] **Step 5: Verify the complete existing behavior**

Run: `cargo test --offline --tests -- --test-threads=1`
Run: `pnpm build`
Expected: all tests pass and Svelte reports no errors.

---

### Task 4: Workspace Path Confinement and API Surface Reduction

**Files:**
- Create: `devshelltools-studio/src-tauri/src/workspace_path.rs`
- Modify: `devshelltools-studio/src-tauri/src/lib.rs`
- Modify: `devshelltools-studio/src-tauri/src/workspace.rs`
- Modify: `devshelltools-studio/src-tauri/src/commands.rs`
- Modify: `devshelltools-studio/src-tauri/src/chat_session.rs`
- Modify: `devshelltools-studio/src-tauri/src/logging.rs`
- Modify: `devshelltools-studio/src-tauri/src/ai_config.rs`
- Modify: `devshelltools-studio/src/lib/api.ts`
- Test: inline `workspace_path.rs` tests

**Interfaces:**
- Produces: `pub struct WorkspacePath(PathBuf)`
- Produces: `WorkspacePath::parse(rel: &str) -> DstResult<Self>`
- Produces: `WorkspacePath::existing(&self) -> DstResult<PathBuf>`
- Produces: `WorkspacePath::for_write(&self) -> DstResult<PathBuf>`
- Produces: `validate_component(value: &str, kind: &str) -> DstResult<()>`

- [ ] **Step 1: Write failing path tests**

```rust
#[test]
fn rejects_traversal_and_windows_prefixes() {
    for rel in ["../x.ps1", "C:x.ps1", r"\\server\share\x.ps1", "Public/a.ps1:ads"] {
        assert!(WorkspacePath::parse(rel).is_err(), "{rel}");
    }
}

#[test]
fn accepts_known_workspace_files() {
    assert!(WorkspacePath::parse("Public/Files.ps1").is_ok());
    assert!(WorkspacePath::parse("Private/Common.ps1").is_ok());
}
```

- [ ] **Step 2: Run the focused tests and confirm they fail**

Run: `cargo test --offline workspace_path::tests -- --test-threads=1`
Expected: module and type are missing.

- [ ] **Step 3: Implement component and workspace confinement**

Normalize separators, reject absolute/prefix/`..`/ADS components, canonicalize existing parents, and verify the resolved path remains below the workspace root.

- [ ] **Step 4: Route all workspace, session, log, and key filenames through validated components**

Delete the unused generic `read_workspace_file`, `write_workspace_file`, and `delete_workspace_file` commands and frontend wrappers.

- [ ] **Step 5: Run focused and regression tests**

Run: `cargo test --offline --lib -- --test-threads=1`
Run: `cargo test --offline --test m2_acceptance --test m4_acceptance --test m6_acceptance -- --test-threads=1`
Expected: all pass.

---

### Task 5: Tauri and WebView Least Privilege

**Files:**
- Modify: `devshelltools-studio/src-tauri/src/lib.rs`
- Modify: `devshelltools-studio/src-tauri/src/main.rs`
- Modify: `devshelltools-studio/src-tauri/Cargo.toml`
- Modify: `devshelltools-studio/src-tauri/capabilities/main.json`
- Modify: `devshelltools-studio/src-tauri/tauri.conf.json`
- Modify: `devshelltools-studio/package.json`
- Modify: `devshelltools-studio/pnpm-lock.yaml`
- Create: `devshelltools-studio/scripts/check-security-config.mjs`

**Interfaces:**
- Removes: frontend filesystem plugin exposure and global Tauri object.
- Preserves: all Studio-specific Tauri commands and local asset loading.

- [ ] **Step 1: Add a failing production-config check**

Add a script assertion that `withGlobalTauri`, `devtools`, `tauri-plugin-fs`, and `script-src 'unsafe-inline'` are absent from production configuration.

- [ ] **Step 2: Run the check and confirm the current configuration fails**

Run: `pnpm exec node scripts/check-security-config.mjs`
Expected: failures listing the broad permissions.

- [ ] **Step 3: Remove unused plugin and privilege**

Remove `@tauri-apps/plugin-fs`, the Rust plugin dependency, plugin initialization, fs capabilities, global Tauri, devtools feature, and inline script CSP permission. Keep inline styles only if the current UI still requires the inline animation delay.

- [ ] **Step 4: Rebuild frontend and Rust application**

Run: `pnpm build`
Run: `cargo check --offline`
Run: `pnpm exec node scripts/check-security-config.mjs`
Expected: all pass.

---

### Task 6: DPAPI Key Storage with Compatible Migration

**Files:**
- Create: `devshelltools-studio/src-tauri/src/key_store.rs`
- Modify: `devshelltools-studio/src-tauri/src/lib.rs`
- Modify: `devshelltools-studio/src-tauri/src/ai_config.rs`
- Modify: `devshelltools-studio/src-tauri/src/commands.rs`
- Modify: `devshelltools-studio/src-tauri/Cargo.toml`
- Test: inline `key_store.rs` tests plus `tests/p1_key_store_acceptance.rs`

**Interfaces:**
- Produces: `pub fn protect(secret: &[u8]) -> DstResult<Vec<u8>>`
- Produces: `pub fn unprotect(ciphertext: &[u8]) -> DstResult<Vec<u8>>`
- Produces: `pub fn load_secret(profile_id: &str) -> DstResult<String>`
- Produces: `pub fn save_secret(profile_id: &str, secret: &str) -> DstResult<()>`
- Implements: a private versioned temp-write-and-replace path that Task 7 can reuse.

- [ ] **Step 1: Write failing DPAPI and migration tests**

```rust
#[test]
fn protected_value_roundtrips_for_current_user() {
    let secret = b"sk-test-value";
    let protected = protect(secret).unwrap();
    assert_ne!(protected, secret);
    assert_eq!(unprotect(&protected).unwrap(), secret);
}

#[test]
fn legacy_plaintext_is_readable_before_migration() {
    write_legacy_key("profile", "legacy-secret");
    assert_eq!(load_secret("profile").unwrap(), "legacy-secret");
}
```

- [ ] **Step 2: Run the tests and confirm key_store is missing**

Run: `cargo test --offline key_store::tests -- --test-threads=1`
Expected: compilation failure.

- [ ] **Step 3: Implement current-user DPAPI protection and versioned storage**

Add the minimal `windows-sys` feature set. Use a version prefix before ciphertext. Write the encrypted file first; remove the legacy plaintext only after successful verification.

- [ ] **Step 4: Preserve availability on migration failure**

If encryption or atomic replacement fails, continue reading the legacy file and emit a warning. Add tests for the fallback and for profile-ID validation.

- [ ] **Step 5: Verify key status masking and export exclusion**

Add a Unicode mask test and rerun bundle export tests to prove keys never enter packages or sessions.

---

### Task 7: Recoverable Workspace File Transactions

**Files:**
- Create: `devshelltools-studio/src-tauri/src/file_txn.rs`
- Modify: `devshelltools-studio/src-tauri/src/lib.rs`
- Modify: `devshelltools-studio/src-tauri/src/workspace.rs`
- Modify: `devshelltools-studio/src-tauri/src/sync.rs`
- Modify: `devshelltools-studio/src-tauri/src/function_edit.rs`
- Modify: `devshelltools-studio/src-tauri/src/export.rs`
- Modify: `devshelltools-studio/src-tauri/src/commands.rs`
- Test: inline `file_txn.rs` tests plus `tests/p1_transaction_acceptance.rs`

**Interfaces:**
- Produces: `pub struct FileTransaction`
- Produces: `FileTransaction::new(transaction_root: &Path) -> DstResult<Self>`
- Produces: `stage(&mut self, target: &Path, bytes: &[u8]) -> DstResult<()>`
- Produces: `replace_all(&mut self) -> DstResult<()>`
- Produces: `recover_pending(transaction_root: &Path) -> DstResult<Vec<PathBuf>>`

- [ ] **Step 1: Write failing transaction tests**

```rust
#[test]
fn failed_second_replace_restores_first_file() {
    let fixture = Fixture::with_files(["a", "b"]);
    let result = fixture.txn_failing_on("b").replace_all();
    assert!(result.is_err());
    assert_eq!(fixture.read("a"), "old-a");
    assert_eq!(fixture.read("b"), "old-b");
}

#[test]
fn pending_journal_is_recovered_or_preserved() {
    let fixture = Fixture::with_pending_journal();
    assert!(recover_pending(fixture.root()).is_ok());
    assert!(fixture.recoverable_state_exists());
}
```

- [ ] **Step 2: Run the tests and confirm the module is missing**

Run: `cargo test --offline file_txn::tests -- --test-threads=1`
Expected: compilation failure.

- [ ] **Step 3: Implement staging, replace, rollback, and journaling**

Use same-volume temporary files, flush before replacement, back up originals, and install in the design-specified order. Record enough metadata to decide automatic recovery versus preserving the transaction for manual recovery.

- [ ] **Step 4: Integrate transactions into generated files and category saves**

Keep the original category file untouched if parsing or safety validation fails. Generated manifest, loader, and Help are staged together; the manifest is installed last.

- [ ] **Step 5: Run focused and acceptance tests**

Run: `cargo test --offline file_txn::tests -- --test-threads=1`
Run: `cargo test --offline --test p1_transaction_acceptance -- --test-threads=1`
Run: `cargo test --offline --tests -- --test-threads=1`
Expected: all pass.

---

### Task 8: Single-Instance Coalescing Runtime Sync

**Files:**
- Create: `devshelltools-studio/src-tauri/src/runtime_sync.rs`
- Modify: `devshelltools-studio/src-tauri/src/lib.rs`
- Modify: `devshelltools-studio/src-tauri/src/install_mgr.rs`
- Modify: `devshelltools-studio/src-tauri/src/commands.rs`
- Modify: `devshelltools-studio/src/lib/api.ts`
- Modify: `devshelltools-studio/src/lib/components/ToolsPage.svelte`
- Test: inline `runtime_sync.rs` tests plus `tests/p1_runtime_sync_acceptance.rs`

**Interfaces:**
- Produces: `pub fn request_sync() -> u64`
- Produces: `pub fn sync_status() -> RuntimeSyncStatus`
- Produces: `pub enum RuntimeSyncState { Idle, Queued, Syncing, Succeeded, Failed }`
- Preserves: `install_module`, `uninstall_module`, and manual retry behavior.

- [ ] **Step 1: Write failing coalescing tests**

```rust
#[test]
fn many_requests_coalesce_to_latest_generation() {
    let worker = TestWorker::blocked();
    for _ in 0..100 { runtime_sync::request_sync(); }
    worker.release();
    assert_eq!(worker.completed_generations(), vec![100]);
}

#[test]
fn failed_sync_does_not_change_successful_edit_state() {
    let status = run_failing_sync();
    assert!(matches!(status.state, RuntimeSyncState::Failed));
    assert!(status.last_error.is_some());
}
```

- [ ] **Step 2: Run the tests and confirm the current unbounded thread implementation cannot satisfy them**

Run: `cargo test --offline runtime_sync::tests -- --test-threads=1`
Expected: compilation failure or missing status API.

- [ ] **Step 3: Implement the worker and status state**

Use a bounded channel plus latest-generation coalescing. Start the worker once. Expose status snapshots and emit `runtime-sync-status` events after state changes.

- [ ] **Step 4: Replace all `spawn_sync_runtime_modules` callers**

Preserve immediate module install behavior and keep the manual “retry sync” action available after failures.

- [ ] **Step 5: Run concurrency and full acceptance tests**

Run: `cargo test --offline --test p1_runtime_sync_acceptance -- --test-threads=1`
Run: `cargo test --offline --tests -- --test-threads=1`
Expected: all pass.

---

### Task 9: Timed Process Runner and Per-Request AI Cancellation

**Files:**
- Create: `devshelltools-studio/src-tauri/src/process_runner.rs`
- Modify: `devshelltools-studio/src-tauri/src/process_util.rs`
- Modify: `devshelltools-studio/src-tauri/src/ps_parser.rs`
- Modify: `devshelltools-studio/src-tauri/src/function_edit.rs`
- Modify: `devshelltools-studio/src-tauri/src/install_mgr.rs`
- Modify: `devshelltools-studio/src-tauri/src/webview2.rs`
- Modify: `devshelltools-studio/src-tauri/src/pwsh.rs`
- Modify: `devshelltools-studio/src-tauri/src/ai_client.rs`
- Modify: `devshelltools-studio/src-tauri/Cargo.toml`
- Test: inline tests plus `tests/p1_timeout_acceptance.rs`

**Interfaces:**
- Produces: `pub struct ProcessSpec { pub program: OsString, pub args: Vec<OsString>, pub timeout: Duration, pub output_limit: usize }`
- Produces: `pub struct ProcessResult { pub status: ExitStatus, pub stdout: Vec<u8>, pub stderr: Vec<u8> }`
- Produces: `pub fn run(spec: ProcessSpec) -> DstResult<ProcessResult>`
- Produces: `pub struct ChatCancellation` with `cancel()` and `is_cancelled()`.
- Preserves: `ai_cancel_chat()` as the current-request cancellation command.

- [ ] **Step 1: Write failing timeout and output-limit tests**

```rust
#[test]
fn long_running_process_is_terminated() {
    let result = run(ProcessSpec::powershell("Start-Sleep -Seconds 30", 200, 1024));
    assert!(result.is_err());
    assert!(error_is_timeout(result));
}

#[test]
fn oversized_output_is_bounded() {
    let result = run(ProcessSpec::powershell("'x' * 100000", 5000, 1024));
    assert!(result.is_err());
    assert!(error_is_output_limit(result));
}
```

- [ ] **Step 2: Run the tests and confirm no timeout API exists**

Run: `cargo test --offline process_runner::tests -- --test-threads=1`
Expected: compilation failure.

- [ ] **Step 3: Implement bounded process execution**

Capture both streams on reader threads, poll with a deadline, kill on timeout, and join all readers before returning. Route every production child process through this runner.

- [ ] **Step 4: Implement AI client timeout, idle timeout, and per-request cancellation**

Use one reusable HTTP client with connect timeout. Apply a timeout to initial response and each streamed chunk. Replace the global `AtomicBool` with a current-request cancellation handle. Preserve explicit “已停止生成” errors.

- [ ] **Step 5: Run timeout and existing AI tests**

Run: `cargo test --offline --test p1_timeout_acceptance -- --test-threads=1`
Run: `cargo test --offline --lib ai_client::tests -- --test-threads=1`
Expected: all pass.

---

### Task 10: True Streaming AI Channel

**Files:**
- Modify: `devshelltools-studio/src-tauri/src/ai_client.rs`
- Modify: `devshelltools-studio/src-tauri/src/commands.rs`
- Modify: `devshelltools-studio/src/lib/api.ts`
- Modify: `devshelltools-studio/src/lib/components/ChatPanel.svelte`
- Create: `devshelltools-studio/src/lib/aiStream.ts`
- Modify: `devshelltools-studio/package.json`
- Modify: `devshelltools-studio/pnpm-lock.yaml`
- Test: inline Rust serialization tests and frontend pure-logic tests

**Interfaces:**
- Produces: `pub enum AiStreamEvent { Start, Delta { text }, Reasoning { text }, Validation { result }, Done, Error { message } }`
- Produces: Tauri command parameter `on_event: Channel<AiStreamEvent>`.
- Produces: `applyStreamEvent(state, event)` in `src/lib/aiStream.ts`.
- Preserves: final validated code block types used by current ChatPanel.

- [ ] **Step 1: Write failing event-order tests**

```rust
#[test]
fn delta_precedes_validation_and_done() {
    let events = run_mock_stream();
    assert!(matches!(events[0], AiStreamEvent::Start));
    assert!(events.iter().any(|e| matches!(e, AiStreamEvent::Delta { .. })));
    assert!(matches!(events.last(), Some(AiStreamEvent::Done)));
}
```

Frontend test:

```ts
it("appends delta before the final validation", () => {
  let state = initialStreamState();
  state = applyStreamEvent(state, { type: "delta", text: "hello" });
  expect(state.text).toBe("hello");
  expect(state.validation).toBeNull();
});
```

- [ ] **Step 2: Run tests and confirm streaming events do not exist**

Run: `cargo test --offline ai_client::tests::delta_precedes_validation -- --test-threads=1`
Run: `pnpm exec vitest run src/lib/aiStream.test.ts`
Expected: compilation failures.

- [ ] **Step 3: Send incremental events and validate once at completion**

Send `Start` immediately, each delta as received, then one `Validation` containing the existing `AiChatResult`, then `Done`. On cancellation, timeout, or network failure send `Error` and no success terminal event.

- [ ] **Step 4: Update ChatPanel to render the live buffer once**

Create the assistant message on `Start`, append deltas without session writes, persist once after `Done`, and clear loading on every terminal event. The explicit self-test action from Task 3 remains unchanged.

- [ ] **Step 5: Run Rust, frontend logic, and production build tests**

Run: `cargo test --offline --lib ai_client::tests -- --test-threads=1`
Run: `pnpm exec vitest run`
Run: `pnpm build`
Expected: all pass.

---

### Task 11: Context Budget, Cost Limits, and Endpoint Validation

**Files:**
- Create: `devshelltools-studio/src-tauri/src/ai_budget.rs`
- Modify: `devshelltools-studio/src-tauri/src/lib.rs`
- Modify: `devshelltools-studio/src-tauri/src/ai_client.rs`
- Modify: `devshelltools-studio/src-tauri/src/ai_config.rs`
- Modify: `devshelltools-studio/src-tauri/src/commands.rs`
- Modify: `devshelltools-studio/src/lib/components/AiSettings.svelte`
- Modify: `devshelltools-studio/src/lib/components/ChatPanel.svelte`
- Test: inline `ai_budget.rs` and `ai_config.rs` tests

**Interfaces:**
- Produces: `pub struct ContextBudget { pub max_chars: usize, pub max_messages: usize }`
- Produces: `pub fn trim_messages(messages: Vec<ChatMessage>, budget: ContextBudget) -> TrimResult`
- Produces: `pub fn validate_endpoint(base_url: &str, protocol: AiProtocol) -> DstResult<EndpointPolicy>`
- Produces: `TrimResult { messages, omitted_count, approximated_chars }`

- [ ] **Step 1: Write failing budget, cost, and URL tests**

```rust
#[test]
fn newest_messages_are_preserved_when_context_is_trimmed() {
    let result = trim_messages(long_history(), ContextBudget::default());
    assert!(result.omitted_count > 0);
    assert_eq!(result.messages.last().unwrap().content, "newest");
}

#[test]
fn public_plain_http_endpoint_is_rejected() {
    assert!(validate_endpoint("http://example.com/v1", AiProtocol::Openai).is_err());
    assert!(validate_endpoint("http://127.0.0.1:11434/v1", AiProtocol::Openai).is_ok());
}

#[test]
fn automatic_output_budget_never_exceeds_policy_cap() {
    assert!(effective_max_tokens(65_536) <= 32_768);
}
```

- [ ] **Step 2: Run focused tests and confirm they fail**

Run: `cargo test --offline ai_budget::tests -- --test-threads=1`
Expected: module and validation interfaces are missing.

- [ ] **Step 3: Implement deterministic trimming and URL policy**

Preserve the system prompt and newest user/assistant turns. Return an omission marker and surface an informational message in the UI. Permit HTTP only for loopback and private networks; require HTTPS for public hosts.

- [ ] **Step 4: Remove unbounded automatic cost expansion**

Allow at most one compatibility retry and cap output at 32,768. When content is still empty or truncated, return a clear error with the configured/effective budget.

- [ ] **Step 5: Run AI and build regressions**

Run: `cargo test --offline --lib ai_client::tests ai_config::tests ai_budget::tests -- --test-threads=1`
Run: `pnpm build`
Expected: all pass.

---

### Task 12: Boundary-Preserving Architecture Split and Release Gates

**Files:**
- Create: `devshelltools-studio/src-tauri/src/commands/workspace.rs`
- Create: `devshelltools-studio/src-tauri/src/commands/ai.rs`
- Create: `devshelltools-studio/src-tauri/src/commands/sessions.rs`
- Create: `devshelltools-studio/src-tauri/src/commands/tools.rs`
- Modify: `devshelltools-studio/src-tauri/src/commands.rs`
- Create: `devshelltools-studio/src/lib/chatState.ts`
- Create: `devshelltools-studio/src/lib/components/ChatCodeBlock.svelte`
- Create: `devshelltools-studio/src/lib/components/ChatMessageList.svelte`
- Create: `devshelltools-studio/src/lib/components/ChatComposer.svelte`
- Modify: `devshelltools-studio/src/lib/components/ChatPanel.svelte`
- Create: `scripts/verify.ps1`
- Create: `scripts/check-template-parity.ps1`
- Create: `scripts/check-dependencies.ps1`
- Modify: `devshelltools-studio/scripts/check-security-config.mjs`
- Create: `devshelltools-studio/src/lib/aiStream.test.ts`
- Create: `devshelltools-studio/src/lib/chatState.test.ts`
- Modify: `devshelltools-studio/package.json`
- Modify: `devshelltools-studio/pnpm-lock.yaml`
- Modify: `README.md`

**Interfaces:**
- Preserves: every frontend-invoked Tauri command name.
- Produces: platform-neutral `scripts/verify.ps1` with exit code 0 only when all gates pass.
- Produces: normalized template parity checker ignoring known metadata and version deltas.

- [ ] **Step 1: Write failing quality-gate and parity tests**

The security-config script must fail on a fixture containing `withGlobalTauri: true`. The parity script must fail on a fixture where shared implementation differs and pass when only metadata/version differs.

- [ ] **Step 2: Run the scripts and confirm the fixtures fail as expected**

Run: `pwsh -NoProfile -File scripts/check-template-parity.ps1 -SelfTest`
Run: `node devshelltools-studio/scripts/check-security-config.mjs --self-test`
Expected: self-tests exercise both pass and fail paths.

- [ ] **Step 3: Split commands without changing command identifiers**

Move handlers by responsibility, re-export them from `commands.rs`, and keep the existing `tauri::generate_handler!` names unchanged.

- [ ] **Step 4: Extract stable ChatPanel responsibilities**

Move code-block rendering and composer/autocomplete markup into components. Move session transitions and stream-state transformations into pure TypeScript modules with tests. Keep ChatPanel as orchestration only.

- [ ] **Step 5: Fix all lint, format, Clippy, and Svelte warnings**

Run: `cargo fmt --all`
Fix `clippy -D warnings`, Svelte accessibility warnings, and reactive initialization warnings without changing intended UI behavior.

- [ ] **Step 6: Run the full release gate**

Run: `pwsh -NoProfile -File scripts/verify.ps1`
Expected:

- `cargo fmt --check` passes.
- `cargo clippy --offline --all-targets -- -D warnings` passes.
- Rust unit and acceptance tests pass.
- Frontend tests pass.
- `pnpm build` passes without warnings.
- Security configuration and template parity checks pass.
- Dependency audit result is recorded with the scan date and tool version; unavailable audit tools are reported as a release limitation rather than silently treated as clean.
- `git diff --check` passes.

---

## Self-Review Coverage

- Safe rendering and metadata: Tasks 1, 2, 7, 12.
- Dynamic command bypass and `/danger` scope: Tasks 2 and 3.
- Secret storage and migration: Task 6.
- Recoverable writes and background sync: Tasks 7 and 8.
- Process and AI timeout/cancellation: Tasks 9 and 10.
- Streaming UX and cost control: Tasks 10 and 11.
- Tauri input/permission boundary: Tasks 4 and 5.
- Maintainability and release gates: Task 12.

## Execution Order

Execute tasks 1 through 12 in numeric order. Do not start P1 before all P0 acceptance tests pass. Do not start P2 before the P1 transaction, sync, key, and timeout acceptance tests pass. If a task exposes a design contradiction, stop and update the spec before changing later interfaces.
