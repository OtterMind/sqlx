//! JDBC drivers the user provides for engines SQLX cannot redistribute.
//!
//! A bundled engine downloads its driver from the release. Some vendors do not allow their driver to
//! be redistributed, so nothing for those engines is published: the user copies the vendor jar into
//! the engine's driver directory and every later command loads it exactly like a bundled one.
use anyhow::{bail, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Directory holding the jars a user provided for one engine component.
///
/// It sits next to the released components (`<root>/drivers/<component>/<platform>/<version>/`), and
/// a jar here always wins over a released one.
pub fn directory(root: &Path, component: &str) -> PathBuf {
    root.join("drivers").join(component)
}

/// The jars a user provided for one engine, sorted; an absent directory reports none.
pub fn provided(root: &Path, component: &str) -> Result<Vec<PathBuf>> {
    let directory = directory(root, component);
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut jars: Vec<PathBuf> = fs::read_dir(&directory)
        .with_context(|| format!("cannot read {}", directory.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.is_file() && path.extension().is_some_and(|extension| extension == "jar")
        })
        .collect();
    jars.sort();
    Ok(jars)
}

/// Whether a jar carries the driver class the JDBC worker loads.
fn carries(jar: &Path, class: &str) -> Result<bool> {
    let file = fs::File::open(jar)
        .with_context(|| format!("cannot read the driver jar {}", jar.display()))?;
    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("{} is not a jar archive", jar.display()))?;
    let entry = format!("{}.class", class.replace('.', "/"));
    let carried = archive.by_name(&entry).is_ok();
    Ok(carried)
}

/// Copy the given jars into the engine directory, after checking each one is the driver.
///
/// A jar that does not carry the driver class is rejected instead of being stored: it would fail
/// later, inside the worker, with a far less useful message.
pub fn install(root: &Path, component: &str, class: &str, jars: &[PathBuf]) -> Result<Vec<String>> {
    if jars.is_empty() {
        bail!("at least one --jar is required");
    }
    let directory = directory(root, component);
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let mut stored = Vec::new();
    for jar in jars {
        if !jar.is_file() {
            bail!("the driver jar {} does not exist", jar.display());
        }
        if !carries(jar, class)? {
            bail!(
                "{} does not contain {class}; pass the vendor's JDBC driver jar",
                jar.display()
            );
        }
        let name = jar
            .file_name()
            .context("the driver jar has no file name")?
            .to_string_lossy()
            .into_owned();
        let target = directory.join(&name);
        if jar.canonicalize().ok().as_deref() != target.canonicalize().ok().as_deref() {
            fs::copy(jar, &target)
                .with_context(|| format!("cannot copy the driver into {}", target.display()))?;
        }
        stored.push(name);
    }
    stored.sort();
    Ok(stored)
}

/// Remove every jar a user provided for one engine; the released components stay untouched.
pub fn remove(root: &Path, component: &str) -> Result<Vec<String>> {
    let mut removed = Vec::new();
    for jar in provided(root, component)? {
        if let Some(name) = jar.file_name() {
            removed.push(name.to_string_lossy().into_owned());
        }
        fs::remove_file(&jar).with_context(|| format!("cannot remove {}", jar.display()))?;
    }
    removed.sort();
    Ok(removed)
}

/// Whether a released driver component is installed for this platform.
pub fn released(root: &Path, component: &str, platform: &str) -> bool {
    fs::read_dir(root.join("drivers").join(component).join(platform))
        .map(|entries| entries.flatten().any(|entry| entry.path().is_dir()))
        .unwrap_or(false)
}
