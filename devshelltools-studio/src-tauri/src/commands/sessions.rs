use super::run_blocking;
use crate::chat_session::{self, ChatSession, SessionSummary};
use crate::error::DstResult;

#[derive(Debug, Clone, serde::Serialize)]
pub struct ResumeListResult {
    pub summaries: Vec<SessionSummary>,
    pub list_text: String,
}

#[tauri::command]
pub async fn list_chat_sessions() -> DstResult<ResumeListResult> {
    run_blocking(|| {
        let summaries = chat_session::list_sessions()?;
        let list_text = chat_session::format_resume_list(&summaries);
        Ok(ResumeListResult {
            summaries,
            list_text,
        })
    })
    .await
}

#[tauri::command]
pub async fn load_chat_session(id: String) -> DstResult<ChatSession> {
    run_blocking(move || {
        let session = chat_session::load_session(&id)?;
        chat_session::set_active_id(Some(&id))?;
        Ok(session)
    })
    .await
}

#[tauri::command]
pub async fn save_chat_session(session: ChatSession) -> DstResult<ChatSession> {
    run_blocking(move || chat_session::save_session(session)).await
}

#[tauri::command]
pub async fn new_chat_session(profile_id: String) -> DstResult<ChatSession> {
    run_blocking(move || chat_session::new_session(&profile_id)).await
}

#[tauri::command]
pub async fn load_or_create_chat_session(profile_id: String) -> DstResult<ChatSession> {
    run_blocking(move || chat_session::load_or_create_active(&profile_id)).await
}

#[tauri::command]
pub async fn set_active_chat_session(id: String) -> DstResult<()> {
    run_blocking(move || chat_session::set_active_id(Some(&id))).await
}
