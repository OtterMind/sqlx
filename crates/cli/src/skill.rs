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
        Some("codex") => home.join(".agents/skills/sqlx"),
        Some("claude") => home.join(".claude/skills/sqlx"),
        _ => bail!("supported targets: codex, claude; use --path for other agents"),
    })
}
pub fn status(manager: &Components) -> Result<Value> {
    Ok(
        json!({"installations":records(manager)?.into_iter().map(|i|json!({"path":i.path,"version":i.version,"intact":files(&i.path).is_ok_and(|f|f==i.files)})).collect::<Vec<_>>()}),
    )
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
