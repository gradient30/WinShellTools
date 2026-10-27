//! AI 会话持续保存（`.studio/sessions/`），供 `/resume` 编号恢复。
//! 参考 Claude Code：边聊边落盘 + 列表切换，编号与单次列表快照绑定。

use crate::error::{DstError, DstResult};
use crate::workspace;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

const INDEX_NAME: &str = "index.json";
const MAX_SESSIONS: usize = 100;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionMessage {
    pub id: String,
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredCodeBlock {
    pub code: String,
    pub syntax_ok: bool,
    pub syntax_err: String,
    pub safety_ok: bool,
    pub safety_violations: Vec<String>,
    pub functions: Vec<String>,
    /// 公共命令（小写函数名）；旧会话缺省回退为 functions
    #[serde(default)]
    pub exported_functions: Vec<String>,
    #[serde(default)]
    pub helper_functions: Vec<String>,
    #[serde(default)]
    pub quality_warnings: Vec<String>,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub profile_id: String,
    pub danger_mode: bool,
    pub messages: Vec<SessionMessage>,
    /// 助手消息下标 → 校验后的代码块（JSON 键为字符串）
    #[serde(default)]
    pub reply_code_blocks: HashMap<String, Vec<StoredCodeBlock>>,
    #[serde(default)]
    pub target_files: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub updated_at: String,
    pub message_count: usize,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SessionIndex {
    pub active_id: Option<String>,
    #[serde(default)]
    pub sessions: Vec<SessionSummary>,
}

fn sessions_dir() -> PathBuf {
    workspace::studio_dir().join("sessions")
}

fn index_path() -> PathBuf {
    sessions_dir().join(INDEX_NAME)
}

fn session_path(id: &str) -> PathBuf {
    sessions_dir().join(format!("{id}.json"))
}

fn ensure_dir() -> DstResult<()> {
    fs::create_dir_all(sessions_dir())
        .map_err(|e| DstError::Other(format!("创建 sessions 目录失败：{e}")))
}

fn atomic_write(path: &Path, data: &[u8]) -> DstResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| DstError::Other(format!("创建目录失败：{e}")))?;
    }
    let tmp = path.with_extension("json.tmp");
    {
        let mut f = fs::File::create(&tmp)
            .map_err(|e| DstError::Other(format!("写临时文件失败：{e}")))?;
        f.write_all(data)
            .map_err(|e| DstError::Other(format!("写入失败：{e}")))?;
        f.sync_all().ok();
    }
    fs::rename(&tmp, path).map_err(|e| DstError::Other(format!("落盘失败：{e}")))?;
    Ok(())
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// 从消息推导标题：首条非 slash 命令的用户内容。
pub fn derive_title(messages: &[SessionMessage]) -> String {
    for m in messages {
        if m.role != "user" {
            continue;
        }
        let t = m.content.trim();
        if t.is_empty() || t.starts_with('/') {
            continue;
        }
        let one_line = t.lines().next().unwrap_or(t).trim();
        let chars: Vec<char> = one_line.chars().collect();
        if chars.len() > 40 {
            return format!("{}…", chars[..40].iter().collect::<String>());
        }
        return one_line.to_string();
    }
    "新会话".into()
}

fn preview_from(messages: &[SessionMessage]) -> String {
    messages
        .iter()
        .rev()
        .find(|m| m.role == "user" && !m.content.trim().starts_with('/'))
        .map(|m| {
            let t = m.content.trim().lines().next().unwrap_or("").trim();
            let chars: Vec<char> = t.chars().collect();
            if chars.len() > 60 {
                format!("{}…", chars[..60].iter().collect::<String>())
            } else {
                t.to_string()
            }
        })
        .unwrap_or_default()
}

fn load_index() -> SessionIndex {
    let p = index_path();
    if !p.exists() {
        return SessionIndex::default();
    }
    match fs::read_to_string(&p) {
        Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
        Err(_) => SessionIndex::default(),
    }
}

fn save_index(idx: &SessionIndex) -> DstResult<()> {
    ensure_dir()?;
    let data = serde_json::to_vec_pretty(idx)?;
    atomic_write(&index_path(), &data)
}

fn summary_of(s: &ChatSession) -> SessionSummary {
    SessionSummary {
        id: s.id.clone(),
        title: s.title.clone(),
        updated_at: s.updated_at.clone(),
        message_count: s.messages.len(),
        preview: preview_from(&s.messages),
    }
}

fn rebuild_index_from_disk(active_id: Option<String>) -> DstResult<SessionIndex> {
    ensure_dir()?;
    let mut sessions = vec![];
    let dir = sessions_dir();
    if dir.is_dir() {
        for entry in fs::read_dir(&dir).map_err(|e| DstError::Other(e.to_string()))? {
            let entry = entry.map_err(|e| DstError::Other(e.to_string()))?;
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.ends_with(".json") || name == INDEX_NAME || name.ends_with(".tmp") {
                continue;
            }
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(sess) = serde_json::from_str::<ChatSession>(&raw) {
                    sessions.push(summary_of(&sess));
                }
            }
        }
    }
    sessions.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    if sessions.len() > MAX_SESSIONS {
        // 删掉最旧的超额文件
        for old in sessions.iter().skip(MAX_SESSIONS) {
            let _ = fs::remove_file(session_path(&old.id));
        }
        sessions.truncate(MAX_SESSIONS);
    }
    let active = active_id
        .filter(|id| sessions.iter().any(|s| &s.id == id))
        .or_else(|| sessions.first().map(|s| s.id.clone()));
    Ok(SessionIndex {
        active_id: active,
        sessions,
    })
}

/// 列出会话摘要（按更新时间降序）。编号由前端按此顺序生成。
pub fn list_sessions() -> DstResult<Vec<SessionSummary>> {
    let idx = rebuild_index_from_disk(load_index().active_id)?;
    save_index(&idx)?;
    Ok(idx.sessions)
}

pub fn get_active_id() -> DstResult<Option<String>> {
    let idx = load_index();
    Ok(idx.active_id)
}

pub fn set_active_id(id: Option<&str>) -> DstResult<()> {
    let mut idx = rebuild_index_from_disk(load_index().active_id)?;
    idx.active_id = id.map(|s| s.to_string());
    if let Some(aid) = &idx.active_id {
        if !idx.sessions.iter().any(|s| &s.id == aid) {
            return Err(DstError::Other(format!("会话不存在：{aid}")));
        }
    }
    save_index(&idx)
}

pub fn load_session(id: &str) -> DstResult<ChatSession> {
    let path = session_path(id);
    if !path.exists() {
        return Err(DstError::Other(format!("会话不存在：{id}")));
    }
    let raw = fs::read_to_string(&path).map_err(|e| DstError::Other(e.to_string()))?;
    let sess: ChatSession =
        serde_json::from_str(&raw).map_err(|e| DstError::Other(format!("会话损坏：{e}")))?;
    Ok(sess)
}

/// 保存会话并更新 index；空会话（无消息）也会保存以便 /new 占位。
pub fn save_session(mut session: ChatSession) -> DstResult<ChatSession> {
    if session.id.trim().is_empty() {
        return Err(DstError::Other("会话 id 不能为空".into()));
    }
    ensure_dir()?;
    let now = now_rfc3339();
    if session.created_at.is_empty() {
        session.created_at = now.clone();
    }
    session.updated_at = now;
    session.title = derive_title(&session.messages);
    let data = serde_json::to_vec_pretty(&session)?;
    atomic_write(&session_path(&session.id), &data)?;

    let mut idx = rebuild_index_from_disk(Some(session.id.clone()))?;
    idx.active_id = Some(session.id.clone());
    // rebuild 已含本会话；确保摘要最新
    if let Some(slot) = idx.sessions.iter_mut().find(|s| s.id == session.id) {
        *slot = summary_of(&session);
    } else {
        idx.sessions.insert(0, summary_of(&session));
    }
    idx.sessions.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| b.id.cmp(&a.id))
    });
    save_index(&idx)?;
    Ok(session)
}

pub fn new_session(profile_id: &str) -> DstResult<ChatSession> {
    let id = uuid_v4_simple();
    let now = now_rfc3339();
    let sess = ChatSession {
        id,
        title: "新会话".into(),
        created_at: now.clone(),
        updated_at: now,
        profile_id: profile_id.to_string(),
        danger_mode: false,
        messages: vec![],
        reply_code_blocks: HashMap::new(),
        target_files: HashMap::new(),
    };
    save_session(sess)
}

/// 启动时：有 active 则加载；否则新建。
pub fn load_or_create_active(profile_id: &str) -> DstResult<ChatSession> {
    let idx = rebuild_index_from_disk(load_index().active_id)?;
    let _ = save_index(&idx);
    if let Some(id) = idx.active_id {
        if let Ok(s) = load_session(&id) {
            return Ok(s);
        }
    }
    new_session(profile_id)
}

/// 按列表序号（1-based）取 id；列表必须与 list_sessions 同序。
pub fn id_at_index(summaries: &[SessionSummary], one_based: usize) -> DstResult<String> {
    if one_based == 0 || one_based > summaries.len() {
        return Err(DstError::Other(format!(
            "编号无效：请输入 1–{}（当前共 {} 个会话）",
            summaries.len().max(1),
            summaries.len()
        )));
    }
    Ok(summaries[one_based - 1].id.clone())
}

fn uuid_v4_simple() -> String {
    // 无 uuid crate：用时间戳+随机性足够本地会话
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:x}-{:x}", nanos, std::process::id())
}

/// 相对时间（中文简写），供列表展示。
pub fn format_relative(iso: &str) -> String {
    let Ok(dt) = DateTime::parse_from_rfc3339(iso) else {
        return iso.to_string();
    };
    let dt = dt.with_timezone(&Utc);
    let now = Utc::now();
    let secs = (now - dt).num_seconds();
    if secs < 60 {
        return "刚刚".into();
    }
    if secs < 3600 {
        return format!("{}分钟前", secs / 60);
    }
    if secs < 86400 {
        return format!("{}小时前", secs / 3600);
    }
    if secs < 86400 * 7 {
        return format!("{}天前", secs / 86400);
    }
    dt.format("%Y-%m-%d").to_string()
}

/// 格式化 `/resume` 列表正文。
pub fn format_resume_list(summaries: &[SessionSummary]) -> String {
    if summaries.is_empty() {
        return "暂无已保存的会话。继续对话将自动创建并保存；也可用 /new 新建。".into();
    }
    let mut lines = vec![
        "历史会话（输入编号恢复，/cancel 取消）：".to_string(),
        String::new(),
    ];
    for (i, s) in summaries.iter().enumerate() {
        let rel = format_relative(&s.updated_at);
        let title = if s.title.trim().is_empty() {
            "新会话"
        } else {
            s.title.as_str()
        };
        lines.push(format!(
            "{}) {} · {} · {}条",
            i + 1,
            title,
            rel,
            s.message_count
        ));
    }
    lines.push(String::new());
    lines.push("示例：输入 1 恢复第 1 个会话。".into());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static LOCK: Mutex<()> = Mutex::new(());

    fn with_tmp_docs<F: FnOnce()>(f: F) {
        let _g = LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let tmp = std::env::temp_dir().join(format!(
            "dst-sess-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let docs = tmp.join("Documents");
        fs::create_dir_all(&docs).unwrap();
        let old_docs = std::env::var("DST_MY_DOCUMENTS").ok();
        let old_up = std::env::var("USERPROFILE").ok();
        std::env::set_var("DST_MY_DOCUMENTS", &docs);
        std::env::set_var("USERPROFILE", &tmp);
        // 工作区根下需要 .studio
        let ws = docs
            .join("WindowsPowerShell")
            .join("Modules")
            .join("DevShellTools");
        fs::create_dir_all(ws.join(".studio")).unwrap();
        f();
        match old_docs {
            Some(v) => std::env::set_var("DST_MY_DOCUMENTS", v),
            None => std::env::remove_var("DST_MY_DOCUMENTS"),
        }
        match old_up {
            Some(v) => std::env::set_var("USERPROFILE", v),
            None => std::env::remove_var("USERPROFILE"),
        }
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn derive_title_skips_slash_commands() {
        let msgs = vec![
            SessionMessage {
                id: "1".into(),
                role: "user".into(),
                content: "/danger".into(),
            },
            SessionMessage {
                id: "2".into(),
                role: "user".into(),
                content: "请审阅 gs 命令".into(),
            },
        ];
        assert_eq!(derive_title(&msgs), "请审阅 gs 命令");
    }

    #[test]
    fn save_load_roundtrip_and_list_order() {
        with_tmp_docs(|| {
            let a = new_session("p1").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(50));
            let mut b = new_session("p1").unwrap();
            b.messages.push(SessionMessage {
                id: "u1".into(),
                role: "user".into(),
                content: "审阅 gs".into(),
            });
            // save_session 会重写 updated_at=now；再睡保证严格晚于 a
            std::thread::sleep(std::time::Duration::from_millis(50));
            let b = save_session(b).unwrap();

            let list = list_sessions().unwrap();
            assert!(list.len() >= 2);
            assert_eq!(list[0].id, b.id, "最新会话应排第一");
            assert_eq!(list[0].title, "审阅 gs");
            assert!(list[0].updated_at >= a.updated_at);

            let loaded = load_session(&b.id).unwrap();
            assert_eq!(loaded.messages.len(), 1);
            assert_eq!(loaded.messages[0].content, "审阅 gs");

            // 编号 1 → 最新
            let id1 = id_at_index(&list, 1).unwrap();
            assert_eq!(id1, b.id);
            let id2 = id_at_index(&list, 2).unwrap();
            assert_eq!(id2, a.id);
            assert!(id_at_index(&list, 0).is_err());
            assert!(id_at_index(&list, 99).is_err());
        });
    }

    #[test]
    fn format_resume_list_numbers_match_order() {
        let summaries = vec![
            SessionSummary {
                id: "a".into(),
                title: "审阅 gs".into(),
                updated_at: Utc::now().to_rfc3339(),
                message_count: 3,
                preview: "gs".into(),
            },
            SessionSummary {
                id: "b".into(),
                title: "新会话".into(),
                updated_at: (Utc::now() - chrono::Duration::hours(3)).to_rfc3339(),
                message_count: 1,
                preview: "".into(),
            },
        ];
        let text = format_resume_list(&summaries);
        assert!(text.contains("1) 审阅 gs"));
        assert!(text.contains("2) 新会话"));
        assert_eq!(id_at_index(&summaries, 1).unwrap(), "a");
        assert_eq!(id_at_index(&summaries, 2).unwrap(), "b");
    }

    #[test]
    fn empty_list_message() {
        let t = format_resume_list(&[]);
        assert!(t.contains("暂无"));
    }
}
