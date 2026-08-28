use anyhow::{Context, Result, anyhow, bail};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::{NamedTempFile, TempPath};

pub struct WritePlan {
    pub path: PathBuf,
    pub original: Vec<u8>,
    pub replacement: Vec<u8>,
}

struct Prepared {
    plan: WritePlan,
    temporary: Option<TempPath>,
    backup: PathBuf,
}

/// Prepare every file first, then commit with rollback backups in each source directory.
pub fn apply_transaction(plans: Vec<WritePlan>) -> Result<()> {
    if plans.is_empty() {
        return Ok(());
    }
    let mut prepared = Vec::with_capacity(plans.len());
    for (sequence, plan) in plans.into_iter().enumerate() {
        reject_symlink(&plan.path, "transaction preparation")?;
        let current = fs::read(&plan.path)
            .with_context(|| format!("cannot re-read {}", plan.path.display()))?;
        if current != plan.original {
            bail!(
                "{} changed while it was being checked; no files were modified",
                plan.path.display()
            );
        }
        let parent = parent_directory(&plan.path);
        let mut temporary = NamedTempFile::new_in(parent).with_context(|| {
            format!(
                "cannot create a temporary file beside {}",
                plan.path.display()
            )
        })?;
        temporary.write_all(&plan.replacement)?;
        let permissions = fs::metadata(&plan.path)?.permissions();
        temporary.as_file().set_permissions(permissions)?;
        temporary.as_file_mut().sync_all()?;
        /* NOTE: Holding every NamedTempFile handle until commit makes the
         * prepare-before-commit guarantee consume one descriptor per file.
         * TempPath retains cleanup ownership while closing the descriptor. */
        let temporary = temporary.into_temp_path();
        let name = plan
            .path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("source");
        let backup = parent.join(format!(
            ".{name}.ocomment-rollback-{}-{sequence}",
            std::process::id()
        ));
        if backup.symlink_metadata().is_ok() {
            /* INVARIANT: The journal holds the file as it was before the interrupted
             * run, so deleting it unread can be the loss the rollback existed
             * to prevent. */
            bail!(
                "rollback path {} already exists; a previous ocomment run may have been \
                 interrupted — inspect and delete it before retrying",
                backup.display()
            );
        }
        prepared.push(Prepared {
            plan,
            temporary: Some(temporary),
            backup,
        });
    }

    for index in 0..prepared.len() {
        if let Err(error) = commit_one(&mut prepared[index]) {
            let failed_path = prepared[index].plan.path.clone();
            /* INVARIANT: Include the failing item: a rename may have created its backup
             * before installing or syncing the replacement failed. */
            let rollback_error = rollback(&prepared[..=index]);
            return Err(match rollback_error {
                Ok(()) => anyhow!(
                    "transaction failed at {} and was rolled back: {error}",
                    failed_path.display()
                ),
                Err(rollback) => anyhow!(
                    "transaction failed at {}; rollback also failed: {error}; {rollback}",
                    failed_path.display()
                ),
            });
        }
    }
    for item in &prepared {
        fs::remove_file(&item.backup)
            .with_context(|| format!("cannot remove rollback journal {}", item.backup.display()))?;
        sync_parent(&item.plan.path)?;
    }
    Ok(())
}

fn commit_one(item: &mut Prepared) -> Result<()> {
    reject_symlink(&item.plan.path, "transaction commit")?;
    let current = fs::read(&item.plan.path)
        .with_context(|| format!("cannot recheck {} before commit", item.plan.path.display()))?;
    if current != item.plan.original {
        bail!(
            "{} changed after transaction preparation",
            item.plan.path.display()
        );
    }
    fs::rename(&item.plan.path, &item.backup).with_context(|| {
        format!(
            "cannot create rollback backup for {}",
            item.plan.path.display()
        )
    })?;
    let temporary = item.temporary.take().expect("prepared temporary file");
    if let Err(error) = temporary.persist(&item.plan.path) {
        let _ = fs::rename(&item.backup, &item.plan.path);
        return Err(error.error).context("cannot atomically install transformed file");
    }
    if let Err(error) = sync_parent(&item.plan.path) {
        let _ = fs::remove_file(&item.plan.path);
        let _ = fs::rename(&item.backup, &item.plan.path);
        return Err(error).context("cannot sync transformed file directory");
    }
    Ok(())
}

fn rollback(items: &[Prepared]) -> Result<()> {
    for item in items.iter().rev() {
        if item.backup.symlink_metadata().is_ok() {
            if item.plan.path.symlink_metadata().is_ok() {
                fs::remove_file(&item.plan.path)?;
            }
            fs::rename(&item.backup, &item.plan.path)?;
            sync_parent(&item.plan.path)?;
        }
    }
    Ok(())
}

/// Refuse to turn a symbolic link into an ordinary file. Reads may opt into
/// following links, but an in-place rewrite would replace the directory entry
/// rather than atomically update its target, which is neither interpretation a
/// caller can safely assume.
fn reject_symlink(path: &Path, phase: &str) -> Result<()> {
    let metadata = path
        .symlink_metadata()
        .with_context(|| format!("cannot inspect {} during {phase}", path.display()))?;
    if metadata.file_type().is_symlink() {
        bail!(
            "refusing to rewrite symbolic link {} during {phase}; no files were modified",
            path.display()
        );
    }
    Ok(())
}

fn sync_parent(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        let directory = fs::File::open(parent_directory(path))?;
        directory.sync_all()?;
    }
    Ok(())
}

fn parent_directory(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn writes_and_preserves_permissions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("x.rs");
        fs::write(&path, b"old").unwrap();
        let permissions = fs::metadata(&path).unwrap().permissions();
        apply_transaction(vec![WritePlan {
            path: path.clone(),
            original: b"old".to_vec(),
            replacement: b"new".to_vec(),
        }])
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().readonly(),
            permissions.readonly()
        );
    }

    /// A journal left over from an interrupted run is the only thing standing
    /// between the caller and a retry, and it holds the pre-run contents of a
    /// file. The refusal has to say both: what the file is, and that reading
    /// it before deleting it is the point.
    ///
    /// The name carries this process's own id, so the test can plant exactly
    /// the journal the transaction is about to reach for.
    #[test]
    fn an_existing_rollback_journal_says_what_to_do_about_it() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("x.rs");
        fs::write(&path, b"old").unwrap();
        let journal = directory
            .path()
            .join(format!(".x.rs.ocomment-rollback-{}-0", std::process::id()));
        fs::write(&journal, b"interrupted").unwrap();

        let error = apply_transaction(vec![WritePlan {
            path: path.clone(),
            original: b"old".to_vec(),
            replacement: b"new".to_vec(),
        }])
        .unwrap_err();

        assert_eq!(
            error.to_string(),
            format!(
                "rollback path {} already exists; a previous ocomment run may have been \
                 interrupted — inspect and delete it before retrying",
                journal.display()
            )
        );
        assert_eq!(fs::read(&path).unwrap(), b"old", "the file was rewritten");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_aborts_the_whole_transaction_without_replacing_link_or_target() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().unwrap();
        let ordinary = directory.path().join("ordinary.rs");
        let target = directory.path().join("target.rs");
        let link = directory.path().join("link.rs");
        fs::write(&ordinary, b"ordinary old").unwrap();
        fs::write(&target, b"target old").unwrap();
        symlink("target.rs", &link).unwrap();

        let error = apply_transaction(vec![
            WritePlan {
                path: ordinary.clone(),
                original: b"ordinary old".to_vec(),
                replacement: b"ordinary new".to_vec(),
            },
            WritePlan {
                path: link.clone(),
                original: b"target old".to_vec(),
                replacement: b"target new".to_vec(),
            },
        ])
        .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("refusing to rewrite symbolic link")
        );
        assert_eq!(fs::read(&ordinary).unwrap(), b"ordinary old");
        assert_eq!(fs::read(&target).unwrap(), b"target old");
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    }
}
