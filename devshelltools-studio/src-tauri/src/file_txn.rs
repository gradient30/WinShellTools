use crate::error::{DstError, DstResult};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JournalEntry {
    target: PathBuf,
    temp: PathBuf,
    backup: Option<PathBuf>,
    original_present: bool,
    installed: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum Phase {
    Prepared,
    Applying,
    Committed,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Journal {
    phase: Phase,
    entries: Vec<JournalEntry>,
}

pub struct FileTransaction {
    transaction_dir: PathBuf,
    journal_path: PathBuf,
    allowed_root: PathBuf,
    entries: Vec<JournalEntry>,
    #[cfg(test)]
    fail_after_apply: Option<usize>,
}

impl FileTransaction {
    pub fn new(transaction_root: &Path) -> DstResult<Self> {
        std::fs::create_dir_all(transaction_root)?;
        let transaction_root = transaction_root.canonicalize()?;
        let allowed_root = match transaction_root
            .parent()
            .filter(|parent| parent.file_name().is_some_and(|name| name == ".studio"))
            .and_then(Path::parent)
        {
            Some(root) => root.to_path_buf(),
            None => transaction_root
                .parent()
                .ok_or_else(|| DstError::Other("事务目录没有可用根路径".into()))?
                .to_path_buf(),
        }
        .canonicalize()?;
        let transaction_dir = transaction_root.join(format!(
            "txn-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&transaction_dir)?;
        let journal_path = transaction_dir.join("journal.json");
        Ok(Self {
            transaction_dir,
            journal_path,
            allowed_root,
            entries: vec![],
            #[cfg(test)]
            fail_after_apply: None,
        })
    }

    pub fn stage(&mut self, target: &Path, bytes: &[u8]) -> DstResult<()> {
        let parent = target
            .parent()
            .ok_or_else(|| DstError::Other("事务目标没有父目录".into()))?;
        std::fs::create_dir_all(parent)?;
        let resolved_parent = parent.canonicalize()?;
        if !resolved_parent.starts_with(&self.allowed_root) {
            return Err(DstError::Other(format!(
                "事务目标越出允许目录：{}",
                target.display()
            )));
        }
        let index = self.entries.len();
        let temp = self.transaction_dir.join(format!("new-{index}"));
        {
            let mut file = std::fs::File::create(&temp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
        }
        self.entries.push(JournalEntry {
            target: target.to_path_buf(),
            temp,
            backup: Some(self.transaction_dir.join(format!("backup-{index}"))),
            original_present: target.exists(),
            installed: false,
        });
        Ok(())
    }

    pub fn replace_all(&mut self) -> DstResult<()> {
        if self.entries.is_empty() {
            let _ = std::fs::remove_dir_all(&self.transaction_dir);
            return Ok(());
        }
        let mut journal = Journal {
            phase: Phase::Prepared,
            entries: self.entries.clone(),
        };
        write_journal(&self.journal_path, &journal)?;
        journal.phase = Phase::Applying;
        write_journal(&self.journal_path, &journal)?;

        for index in 0..journal.entries.len() {
            let result = apply_entry(&mut journal.entries[index]);
            if let Err(error) = result {
                return fail_and_rollback(
                    &self.journal_path,
                    &self.transaction_dir,
                    journal,
                    error,
                );
            }
            write_journal(&self.journal_path, &journal)?;
            #[cfg(test)]
            if self.fail_after_apply == Some(index + 1) {
                return fail_and_rollback(
                    &self.journal_path,
                    &self.transaction_dir,
                    journal,
                    DstError::Other("测试注入：事务替换失败".into()),
                );
            }
        }

        journal.phase = Phase::Committed;
        write_journal(&self.journal_path, &journal)?;
        std::fs::remove_dir_all(&self.transaction_dir)?;
        Ok(())
    }
}

pub fn recover_pending(transaction_root: &Path) -> DstResult<Vec<PathBuf>> {
    if !transaction_root.exists() {
        return Ok(vec![]);
    }
    let mut recovered = vec![];
    for entry in std::fs::read_dir(transaction_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let transaction_dir = entry.path();
        let journal_path = transaction_dir.join("journal.json");
        if !journal_path.exists() {
            let _ = std::fs::remove_dir_all(&transaction_dir);
            continue;
        }
        let journal: Journal = serde_json::from_slice(&std::fs::read(&journal_path)?)
            .map_err(|error| DstError::Other(format!("事务日志损坏：{error}")))?;
        if journal.phase == Phase::Committed {
            std::fs::remove_dir_all(&transaction_dir)?;
            recovered.push(transaction_dir);
            continue;
        }
        rollback_entries(&journal.entries)?;
        std::fs::remove_dir_all(&transaction_dir)?;
        recovered.push(transaction_dir);
    }
    Ok(recovered)
}

fn apply_entry(entry: &mut JournalEntry) -> DstResult<()> {
    if !entry.temp.exists() {
        return Err(DstError::Other(format!(
            "事务暂存文件不存在：{}",
            entry.temp.display()
        )));
    }
    if entry.target.exists() && !entry.target.is_file() {
        return Err(DstError::Other(format!(
            "事务目标不是文件：{}",
            entry.target.display()
        )));
    }
    if entry.original_present {
        let backup = entry
            .backup
            .as_ref()
            .ok_or_else(|| DstError::Other("缺少事务备份路径".into()))?;
        if backup.exists() {
            std::fs::remove_file(backup)?;
        }
        std::fs::rename(&entry.target, backup)?;
    }
    if let Err(error) = std::fs::rename(&entry.temp, &entry.target) {
        if entry.original_present {
            if let Some(backup) = &entry.backup {
                let _ = std::fs::rename(backup, &entry.target);
            }
        }
        return Err(error.into());
    }
    entry.installed = true;
    Ok(())
}

fn fail_and_rollback(
    journal_path: &Path,
    transaction_dir: &Path,
    mut journal: Journal,
    original_error: DstError,
) -> DstResult<()> {
    journal.phase = Phase::RolledBack;
    if let Err(rollback_error) = rollback_entries(&journal.entries) {
        let _ = write_journal(journal_path, &journal);
        return Err(DstError::Other(format!(
            "事务失败且回滚未完成：{original_error}；{rollback_error}"
        )));
    }
    let _ = write_journal(journal_path, &journal);
    std::fs::remove_dir_all(transaction_dir)?;
    Err(original_error)
}

fn rollback_entries(entries: &[JournalEntry]) -> DstResult<()> {
    for entry in entries.iter().rev() {
        if let Some(backup) = &entry.backup {
            if backup.exists() {
                if entry.target.exists() {
                    if !entry.target.is_file() {
                        return Err(DstError::Other(format!(
                            "回滚目标不是文件：{}",
                            entry.target.display()
                        )));
                    }
                    std::fs::remove_file(&entry.target)?;
                }
                std::fs::rename(backup, &entry.target)?;
                continue;
            }
        }
        if !entry.original_present && !entry.temp.exists() && entry.target.exists() {
            std::fs::remove_file(&entry.target)?;
        }
        if entry.temp.exists() {
            let _ = std::fs::remove_file(&entry.temp);
        }
    }
    Ok(())
}

fn write_journal(path: &Path, journal: &Journal) -> DstResult<()> {
    let bytes = serde_json::to_vec_pretty(journal)?;
    let temp = path.with_extension("json.tmp");
    {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    if path.exists() {
        let backup = path.with_extension("json.bak");
        let _ = std::fs::remove_file(&backup);
        std::fs::rename(path, &backup)?;
        if let Err(error) = std::fs::rename(&temp, path) {
            let _ = std::fs::rename(&backup, path);
            return Err(error.into());
        }
        let _ = std::fs::remove_file(backup);
    } else {
        std::fs::rename(temp, path)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: std::path::PathBuf,
        transactions: std::path::PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "dst-file-txn-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos()
            ));
            let transactions = root.join(".studio").join("transactions");
            std::fs::create_dir_all(&transactions).expect("transactions");
            std::fs::write(root.join("a"), "old-a").expect("a");
            std::fs::write(root.join("b"), "old-b").expect("b");
            Self { root, transactions }
        }

        fn read(&self, name: &str) -> String {
            std::fs::read_to_string(self.root.join(name)).expect("read fixture")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn failed_second_replace_restores_first_file() {
        let fixture = Fixture::new();
        let mut transaction = FileTransaction::new(&fixture.transactions).expect("transaction");
        transaction
            .stage(&fixture.root.join("a"), b"new-a")
            .expect("stage a");
        transaction
            .stage(&fixture.root.join("b"), b"new-b")
            .expect("stage b");
        transaction.fail_after_apply = Some(1);

        assert!(transaction.replace_all().is_err());
        assert_eq!(fixture.read("a"), "old-a");
        assert_eq!(fixture.read("b"), "old-b");
    }

    #[test]
    fn pending_journal_is_recovered() {
        let fixture = Fixture::new();
        let tx_dir = fixture.transactions.join("pending");
        std::fs::create_dir_all(&tx_dir).expect("tx dir");
        let target = fixture.root.join("a");
        let backup = tx_dir.join("backup-0");
        std::fs::rename(&target, &backup).expect("backup");
        std::fs::write(&target, "partial-new").expect("partial target");
        let journal = Journal {
            phase: Phase::Applying,
            entries: vec![JournalEntry {
                target,
                temp: tx_dir.join("new-0"),
                backup: Some(backup),
                original_present: true,
                installed: true,
            }],
        };
        write_journal(&tx_dir.join("journal.json"), &journal).expect("journal");

        let recovered = recover_pending(&fixture.transactions).expect("recover");
        assert_eq!(recovered.len(), 1);
        assert_eq!(fixture.read("a"), "old-a");
        assert!(!tx_dir.exists());
    }
}
