#[derive(Debug, Clone, serde::Serialize)]
pub struct ValidatedCodeBlock {
    pub code: String,
    pub syntax_ok: bool,
    pub syntax_err: String,
    pub safety_ok: bool,
    pub safety_violations: Vec<String>,
    /// 块内全部函数（兼容保留）
    pub functions: Vec<String>,
    /// 公共命令（小写函数名）——用户在终端真正可敲的命令
    pub exported_functions: Vec<String>,
    /// 内部辅助函数（大写开头，随命令写入但不作为命令汇报）
    pub helper_functions: Vec<String>,
    /// 首个公共命令的 .EXAMPLE 首行，供显式确认后试运行
    pub first_example: Option<String>,
    /// 质量验收警告
    pub quality_warnings: Vec<String>,
    pub category: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AiChatResult {
    pub reply: String,
    pub code_blocks: Vec<ValidatedCodeBlock>,
}
