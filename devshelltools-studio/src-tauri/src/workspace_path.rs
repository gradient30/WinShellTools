use crate::error::{DstError, DstResult};
use crate::workspace;
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspacePath {
    relative: PathBuf,
}

impl WorkspacePath {
    pub fn parse(rel: &str) -> DstResult<Self> {
        let trimmed = rel.trim();
        if trimmed.is_empty() {
            return Err(DstError::Other("工作区路径不能为空".into()));
        }
        if trimmed.starts_with('/') || trimmed.starts_with('\\') || trimmed.contains(':') {
            return Err(DstError::Other(format!("工作区路径必须为相对路径：{rel}")));
        }

        let normalized = trimmed.replace('\\', "/");
        let path = Path::new(&normalized);
        let mut clean = PathBuf::new();
        for component in path.components() {
            match component {
                Component::Normal(part) if !part.is_empty() => clean.push(part),
                Component::CurDir => {
                    return Err(DstError::Other(format!("工作区路径不得包含 . 组件：{rel}")))
                }
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(DstError::Other(format!("工作区路径越界：{rel}")))
                }
                _ => return Err(DstError::Other(format!("非法工作区路径：{rel}"))),
            }
        }
        if clean.as_os_str().is_empty() {
            return Err(DstError::Other("工作区路径不能为空".into()));
        }
        Ok(Self { relative: clean })
    }

    pub fn as_relative(&self) -> &Path {
        &self.relative
    }

    pub fn existing(&self) -> DstResult<PathBuf> {
        let root = canonical_root()?;
        let candidate = root.join(&self.relative);
        let resolved = candidate.canonicalize().map_err(|error| {
            DstError::Other(format!("工作区路径不存在：{}（{error}）", candidate.display()))
        })?;
        ensure_within(&root, &resolved)?;
        Ok(resolved)
    }

    pub fn for_write(&self) -> DstResult<PathBuf> {
        let root = canonical_root()?;
        let candidate = root.join(&self.relative);
        let mut ancestor = candidate.parent();
        while let Some(path) = ancestor {
            if path.exists() {
                let resolved_parent = path.canonicalize().map_err(|error| {
                    DstError::Other(format!("无法校验目标父目录：{}（{error}）", path.display()))
                })?;
                ensure_within(&root, &resolved_parent)?;
                return Ok(candidate);
            }
            ancestor = path.parent();
        }
        Err(DstError::Other("目标路径没有可校验的父目录".into()))
    }
}

pub fn validate_component(value: &str, kind: &str) -> DstResult<()> {
    if value.is_empty()
        || value.trim() != value
        || matches!(value, "." | "..")
        || value.contains('/')
        || value.contains('\\')
        || value.contains(':')
        || value.ends_with('.')
        || value.ends_with(' ')
        || value.chars().any(|ch| ch.is_control() || matches!(ch, '<' | '>' | '"' | '|' | '?' | '*'))
    {
        return Err(DstError::Other(format!("{kind}名称非法：{value}")));
    }
    Ok(())
}

fn canonical_root() -> DstResult<PathBuf> {
    workspace::workspace_root()
        .canonicalize()
        .map_err(|error| DstError::WorkspaceBroken(format!("工作区不可用：{error}")))
}

fn ensure_within(root: &Path, candidate: &Path) -> DstResult<()> {
    if candidate.starts_with(root) {
        Ok(())
    } else {
        Err(DstError::Other(format!(
            "工作区路径逃逸：{}",
            candidate.display()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_traversal_and_windows_prefixes() {
        for rel in [
            "../x.ps1",
            "C:x.ps1",
            r"\\server\share\x.ps1",
            "Public/a.ps1:ads",
            "/absolute/x.ps1",
        ] {
            assert!(WorkspacePath::parse(rel).is_err(), "{rel}");
        }
    }

    #[test]
    fn accepts_known_workspace_files() {
        assert_eq!(
            WorkspacePath::parse("Public/Files.ps1")
                .expect("public")
                .as_relative(),
            std::path::Path::new("Public/Files.ps1")
        );
        assert!(WorkspacePath::parse("Private/Common.ps1").is_ok());
        assert!(WorkspacePath::parse(r"Public\Files.ps1").is_ok());
    }

    #[test]
    fn component_validation_rejects_path_and_ads_syntax() {
        assert!(validate_component("Files.ps1", "分类文件").is_ok());
        assert!(validate_component("../Files.ps1", "分类文件").is_err());
        assert!(validate_component("Files.ps1:ads", "分类文件").is_err());
        assert!(validate_component("", "分类文件").is_err());
    }
}
