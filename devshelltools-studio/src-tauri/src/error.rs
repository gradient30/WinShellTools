use thiserror::Error;

#[derive(Debug, Error)]
pub enum DstError {
    #[error("工作区不存在：{0}")]
    WorkspaceNotFound(String),
    #[error("工作区已存在：{0}")]
    WorkspaceExists(String),
    #[error("工作区校验失败：{0}")]
    WorkspaceBroken(String),
    #[error("文件未找到：{0}")]
    FileNotFound(String),
    #[error("IO 错误：{0}")]
    Io(#[from] std::io::Error),
    #[error("JSON 错误：{0}")]
    Json(#[from] serde_json::Error),
    #[error("PowerShell 解析错误：{0}")]
    PsParse(String),
    #[error("安全规则拦截：{0}")]
    SafetyBlocked(String),
    #[error("AI 调用错误：{0}")]
    AiClient(String),
    #[error("凭证错误：{0}")]
    Keyring(String),
    #[error("HTTP 错误：{0}")]
    Http(#[from] reqwest::Error),
    #[error("{0}")]
    Other(String),
}

pub type DstResult<T> = Result<T, DstError>;

impl serde::Serialize for DstError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.to_string().as_ref())
    }
}
