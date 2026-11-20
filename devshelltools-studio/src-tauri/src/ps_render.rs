use crate::error::{DstError, DstResult};

/// Render arbitrary text as a PowerShell single-quoted string literal.
pub fn quote_data(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// Category names are also used as identifiers in generated hashtables.
pub fn validate_category_name(name: &str) -> DstResult<()> {
    let mut chars = name.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    if valid {
        Ok(())
    } else {
        Err(DstError::Other(format!(
            "分类名称必须以小写字母开头且仅包含小写字母和数字：{name}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_literal_neutralizes_powershell_expansion() {
        let value = "x'; $(Write-Output injected); `whoami`";
        assert_eq!(
            quote_data(value),
            "'x''; $(Write-Output injected); `whoami`'"
        );
    }

    #[test]
    fn category_name_rejects_shell_syntax() {
        assert!(validate_category_name("files").is_ok());
        assert!(validate_category_name("files; whoami").is_err());
        assert!(validate_category_name("Files").is_err());
    }
}
