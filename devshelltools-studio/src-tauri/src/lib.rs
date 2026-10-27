pub mod ai_client;
pub mod ai_config;
pub mod ai_presets;
pub mod chat_session;
pub mod commands;
pub mod consistency;
pub mod error;
pub mod export;
pub mod function_edit;
pub mod init_progress;
pub mod install_mgr;
pub mod logging;
pub mod migrate;
pub mod process_util;
pub mod ps_parser;
pub mod pwsh;
pub mod safety;
pub mod sync;
pub mod template;
pub mod webview2;
pub mod workspace;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            #[cfg(all(desktop, debug_assertions))]
            {
                use tauri::Manager;
                if let Some(window) = app.get_webview_window("main") {
                    window.open_devtools();
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::workspace_status,
            commands::init_workspace,
            commands::list_public_files,
            commands::read_workspace_file,
            commands::list_categories,
            commands::read_category_file,
            commands::write_workspace_file,
            commands::delete_workspace_file,
            commands::create_category,
            commands::delete_category,
            commands::update_category_file,
            commands::sync_public,
            commands::upsert_function,
            commands::delete_function,
            commands::test_function,
            commands::apply_ai_code,
            commands::install_status,
            commands::install_module,
            commands::uninstall_module,
            commands::consistency_check,
            commands::safety_check,
            commands::validate_ps_syntax,
            commands::get_ai_config,
            commands::save_ai_config,
            commands::save_ai_key,
            commands::get_ai_key_status,
            commands::ai_ready,
            commands::list_ai_profiles,
            commands::get_ai_profiles_meta,
            commands::save_ai_profile,
            commands::delete_ai_profile,
            commands::set_default_ai_profile,
            commands::test_ai_profile,
            commands::list_ai_presets,
            commands::suggest_ai_endpoint,
            commands::fetch_ai_models,
            commands::fetch_ai_models_preview,
            commands::ai_chat,
            commands::ai_cancel_chat,
            commands::ai_chat_with_validation,
            commands::list_chat_sessions,
            commands::load_chat_session,
            commands::save_chat_session,
            commands::new_chat_session,
            commands::load_or_create_chat_session,
            commands::set_active_chat_session,
            commands::check_migration,
            commands::migrate_legacy,
            commands::export_workspace,
            commands::import_workspace,
            commands::export_bundle,
            commands::preview_bundle,
            commands::import_bundle,
            commands::list_logs,
            commands::read_log,
            commands::webview2_status,
            commands::webview2_download_url,
            commands::pwsh_status,
            commands::pwsh_download_url,
        ])
        .run(tauri::generate_context!())
        .expect("启动 DevShellTools Studio 失败");
}