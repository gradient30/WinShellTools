//! 验收测试共享环境：串行化 Documents 隔离，避免并行测试互相污染。
//!
//! 工作区路径基于 MyDocuments（`DST_MY_DOCUMENTS` / GetFolderPath），
//! 仅改 USERPROFILE 无法隔离真实 Documents。

use std::env;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static DOCS_LOCK: Mutex<()> = Mutex::new(());

pub struct IsolatedProfile {
    _lock: std::sync::MutexGuard<'static, ()>,
    tmp: PathBuf,
    original_userprofile: Option<String>,
    original_docs: Option<String>,
}

impl IsolatedProfile {
    pub fn new(prefix: &str) -> Self {
        let _lock = DOCS_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let original_userprofile = env::var("USERPROFILE").ok();
        let original_docs = env::var("DST_MY_DOCUMENTS").ok();
        let tmp = make_tmp_user_profile(prefix);
        let docs = tmp.join("Documents");
        std::fs::create_dir_all(&docs).unwrap();
        env::set_var("USERPROFILE", tmp.to_str().unwrap());
        env::set_var("DST_MY_DOCUMENTS", docs.to_str().unwrap());
        Self {
            _lock,
            tmp,
            original_userprofile,
            original_docs,
        }
    }

    #[allow(dead_code)]
    pub fn documents_dir(&self) -> PathBuf {
        self.tmp.join("Documents")
    }
}

impl Drop for IsolatedProfile {
    fn drop(&mut self) {
        match &self.original_docs {
            Some(v) => env::set_var("DST_MY_DOCUMENTS", v),
            None => env::remove_var("DST_MY_DOCUMENTS"),
        }
        match &self.original_userprofile {
            Some(v) => env::set_var("USERPROFILE", v),
            None => env::remove_var("USERPROFILE"),
        }
        let _ = std::fs::remove_dir_all(&self.tmp);
    }
}

fn make_tmp_user_profile(prefix: &str) -> PathBuf {
    let mut p = env::temp_dir();
    p.push(format!(
        "dst-{prefix}-profile-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&p).unwrap();
    p
}
