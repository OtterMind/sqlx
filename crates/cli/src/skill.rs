use crate::{
    components::{files, Components},
    storage::{atomic_write, open_private},
};
use anyhow::{bail, Context, Result};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize, Deserialize)]
struct Installation {
    path: PathBuf,
    version: String,
    files: BTreeMap<String, String>,
}
fn records(manager: &Components) -> Result<Vec<Installation>> {
    let path = manager.root.join("skill-installs.json");
    if path.exists() {
        Ok(serde_json::from_slice(&fs::read(path)?)?)
    } else {
        Ok(vec![])
    }
}
pub fn target_path(target: Option<String>, path: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(path) = path {
        return Ok(path);
    }
    let home = dirs::home_dir().context("cannot locate user home")?;
    Ok(match target.as_deref() {
        // Codex and dsh both discover skills in the shared Agent Skills directory.
        Some("codex" | "dsh") => home.join(".agents/skills/sqlx"),
        Some("claude") => home.join(".claude/skills/sqlx"),
        Some("pi") => home.join(".pi/agent/skills/sqlx"),
        _ => bail!("supported targets: codex, claude, dsh, pi; use --path for other agents"),
    })
}
pub fn status(manager: &Components) -> Result<Value> {
    Ok(
        json!({"installations":records(manager)?.into_iter().map(|i|{let missing=!i.path.exists();json!({"path":i.path,"version":i.version,"missing":missing,"intact":!missing&&files(&i.path).is_ok_and(|f|f==i.files)})}).collect::<Vec<_>>()}),
    )
}
/// Canonicalize the nearest existing ancestor and re-append the missing tail, so a
/// record stays addressable after its directory was deleted (for example under /tmp).
fn resolved_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    let mut missing = Vec::new();
    let mut current = path;
    loop {
        match current.canonicalize() {
            Ok(base) => {
                let mut resolved = base;
                for part in missing.iter().rev() {
                    resolved.push(part);
                }
                return resolved;
            }
            Err(_) => match (current.file_name(), current.parent()) {
                (Some(name), Some(parent)) => {
                    missing.push(name.to_os_string());
                    current = parent;
                }
                _ => return path.to_path_buf(),
            },
        }
    }
}
/// Stop managing a recorded Skill installation. Its files are never deleted here.
pub fn remove(manager: &Components, path: PathBuf) -> Result<Value> {
    let lock = open_private(&manager.root.join("skill-installs.lock"))?;
    lock.lock_exclusive()?;
    let mut installs = records(manager)?;
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    let target = resolved_path(&absolute);
    let before = installs.len();
    installs.retain(|i| i.path != target && i.path != absolute);
    if installs.len() == before {
        bail!(
            "no SQLX-managed Skill is recorded at {}; run `sqlx skill status` to list them",
            absolute.display()
        );
    }
    atomic_write(
        &manager.root.join("skill-installs.json"),
        &serde_json::to_vec_pretty(&installs)?,
    )?;
    Ok(json!({"removed":true,"path":target,"directory_exists":target.exists()}))
}
pub fn install(manager: &Components, path: PathBuf) -> Result<Value> {
    let manifest = manager.manifest(false)?;
    let asset = manager.asset(&manifest, "skill", "any")?;
    let entry = manager.ensure("skill", "any", asset)?;
    let lock = open_private(&manager.root.join("skill-installs.lock"))?;
    lock.lock_exclusive()?;
    let mut installs = records(manager)?;
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };
    let parent = absolute.parent().context("skill path has no parent")?;
    fs::create_dir_all(parent)?;
    let target = parent
        .canonicalize()?
        .join(absolute.file_name().context("invalid skill target")?);
    let previous = installs.iter().find(|i| i.path == target);
    if target.exists() {
        if fs::symlink_metadata(&target)?.file_type().is_symlink() {
            bail!("skill target must not be a symlink");
        }
        if let Some(old) = previous {
            if files(&target)? != old.files {
                bail!("installed skill has local changes; existing files were preserved");
            }
        } else {
            bail!("skill destination already exists and is not managed by SQLX");
        }
    }
    let stage = tempfile::tempdir_in(target.parent().unwrap())?;
    copy_tree(entry.parent().unwrap(), stage.path())?;
    if !stage.path().join("SKILL.md").is_file() {
        bail!("skill package has no SKILL.md");
    }
    let installed = Installation {
        path: target.clone(),
        version: asset.version.clone(),
        files: files(stage.path())?,
    };
    let backup = target.with_file_name(format!(".sqlx-backup-{}", uuid::Uuid::new_v4()));
    let existed = target.exists();
    if existed {
        fs::rename(&target, &backup)?;
    }
    if let Err(e) = fs::rename(stage.path(), &target) {
        if existed {
            fs::rename(&backup, &target)?;
        }
        return Err(e.into());
    }
    installs.retain(|i| i.path != target);
    installs.push(installed);
    if let Err(e) = atomic_write(
        &manager.root.join("skill-installs.json"),
        &serde_json::to_vec_pretty(&installs)?,
    ) {
        fs::remove_dir_all(&target)?;
        if existed {
            fs::rename(&backup, &target)?;
        }
        return Err(e);
    }
    if existed {
        fs::remove_dir_all(backup)?;
    }
    Ok(json!({"path":target,"version":asset.version,"installed":true}))
}
pub fn update(manager: &Components) -> Result<Value> {
    manager.manifest(true)?;
    let targets = records(manager)?;
    let mut updated = Vec::new();
    for target in targets {
        updated.push(install(manager, target.path)?);
    }
    Ok(json!({"updated":updated}))
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    for item in fs::read_dir(source)? {
        let item = item?;
        if item.file_name() == ".receipt.json" {
            continue;
        }
        let ty = item.file_type()?;
        let dest = target.join(item.file_name());
        if ty.is_symlink() {
            bail!("skill package must not contain symlinks");
        }
        if ty.is_dir() {
            fs::create_dir(&dest)?;
            copy_tree(&item.path(), &dest)?;
        } else if ty.is_file() {
            fs::copy(item.path(), dest)?;
        } else {
            bail!("unsupported skill file");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_targets_map_to_agent_directories() {
        let home = dirs::home_dir().unwrap();
        for (target, expected) in [
            ("codex", home.join(".agents/skills/sqlx")),
            // dsh discovers the shared Agent Skills directory as well.
            ("dsh", home.join(".agents/skills/sqlx")),
            ("claude", home.join(".claude/skills/sqlx")),
            ("pi", home.join(".pi/agent/skills/sqlx")),
        ] {
            assert_eq!(
                target_path(Some(target.to_owned()), None).unwrap(),
                expected,
                "{target}"
            );
        }
    }
    #[test]
    fn explicit_path_wins_and_unknown_targets_fail() {
        let path = PathBuf::from("/tmp/sqlx-skill");
        assert_eq!(
            target_path(Some("codex".to_owned()), Some(path.clone())).unwrap(),
            path
        );
        let error = target_path(Some("other".to_owned()), None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("codex, claude, dsh, pi"), "{error}");
    }
    #[test]
    fn remove_forgets_one_record_and_keeps_its_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let manager = Components::new(root.clone(), "unused".into());
        let recorded = root.join("skills/sqlx");
        let other = root.join("other/sqlx");
        fs::create_dir_all(&recorded).unwrap();
        fs::create_dir_all(&other).unwrap();
        let entry = |path: PathBuf| Installation {
            path,
            version: "0.1.6".into(),
            files: BTreeMap::new(),
        };
        atomic_write(
            &root.join("skill-installs.json"),
            &serde_json::to_vec_pretty(&[entry(recorded.clone()), entry(other.clone())]).unwrap(),
        )
        .unwrap();
        let value = remove(&manager, recorded.clone()).unwrap();
        assert_eq!(value["removed"], json!(true));
        assert_eq!(value["directory_exists"], json!(true));
        assert!(recorded.exists(), "removal must not delete files");
        let left = records(&manager).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].path, other);
        let error = remove(&manager, recorded).unwrap_err().to_string();
        assert!(
            error.contains("no SQLX-managed Skill is recorded"),
            "{error}"
        );
    }
    #[test]
    fn remove_resolves_a_path_whose_directory_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let manager = Components::new(root.clone(), "unused".into());
        let recorded = root.join("gone/sqlx");
        fs::create_dir_all(&recorded).unwrap();
        // The installer records the canonical path; remove must still find it after deletion.
        let canonical = recorded.canonicalize().unwrap();
        atomic_write(
            &root.join("skill-installs.json"),
            &serde_json::to_vec_pretty(&[Installation {
                path: canonical,
                version: "0.1.6".into(),
                files: BTreeMap::new(),
            }])
            .unwrap(),
        )
        .unwrap();
        fs::remove_dir_all(&recorded).unwrap();
        let value = remove(&manager, recorded).unwrap();
        assert_eq!(value["removed"], json!(true));
        assert_eq!(value["directory_exists"], json!(false));
        assert!(records(&manager).unwrap().is_empty());
    }
    #[test]
    fn status_reports_a_missing_installation() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let manager = Components::new(root.clone(), "unused".into());
        let gone = root.join("gone/sqlx");
        atomic_write(
            &root.join("skill-installs.json"),
            &serde_json::to_vec_pretty(&[Installation {
                path: gone.clone(),
                version: "0.1.6".into(),
                files: BTreeMap::new(),
            }])
            .unwrap(),
        )
        .unwrap();
        let value = status(&manager).unwrap();
        assert_eq!(value["installations"][0]["missing"], json!(true));
        assert_eq!(value["installations"][0]["intact"], json!(false));
    }
}
