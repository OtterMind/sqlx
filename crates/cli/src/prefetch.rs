//! Download release components ahead of time so the first query or page does not wait.
use crate::components::{human_duration, platform, Components};
use anyhow::{bail, Result};
use serde_json::{json, Value};
use std::{path::Path, time::Instant};

/// Components accepted on the command line, in the order `all` downloads them.
pub(crate) const CHOICES: [&str; 15] = [
    "mysql",
    "mariadb",
    "tidb",
    "starrocks",
    "doris",
    "postgres",
    "cockroachdb",
    "yugabytedb",
    "oracle",
    "sqlserver",
    "clickhouse",
    "trino",
    "ui",
    "skill",
    "all",
];
/// Resolve one requested component into the manifest entries it needs.
fn expand(name: &str, platform: &str) -> Result<Vec<(String, String)>> {
    let entries = match name {
        // MariaDB, TiDB, StarRocks and Doris speak the MySQL protocol, and CockroachDB and
        // YugabyteDB the PostgreSQL protocol, so all of them reuse a native worker.
        "mysql" | "postgres" => vec![(name.to_owned(), platform.to_owned())],
        "mariadb" | "tidb" | "starrocks" | "doris" => {
            vec![("mysql".to_owned(), platform.to_owned())]
        }
        "cockroachdb" | "yugabytedb" => vec![("postgres".to_owned(), platform.to_owned())],
        // The JDBC databases need the shared runner and the pinned JRE as well.
        "oracle" | "sqlserver" | "clickhouse" | "trino" => vec![
            ("java".to_owned(), platform.to_owned()),
            ("jdbc".to_owned(), "any".to_owned()),
            (name.to_owned(), "any".to_owned()),
        ],
        "ui" => vec![
            ("ui".to_owned(), platform.to_owned()),
            ("ui-default".to_owned(), "any".to_owned()),
        ],
        "skill" => vec![("skill".to_owned(), "any".to_owned())],
        "all" => {
            let mut all = Vec::new();
            for target in [
                "mysql",
                "mariadb",
                "tidb",
                "starrocks",
                "doris",
                "postgres",
                "cockroachdb",
                "yugabytedb",
                "ui",
                "skill",
                "oracle",
                "sqlserver",
                "clickhouse",
                "trino",
            ] {
                all.extend(expand(target, platform)?);
            }
            all
        }
        other => bail!(
            "unknown component {other}; expected one of {}",
            CHOICES.join(", ")
        ),
    };
    Ok(entries)
}
/// Expand the requested components into a deduplicated download plan.
fn plan(requested: &[String], platform: &str) -> Result<Vec<(String, String)>> {
    let mut plan: Vec<(String, String)> = Vec::new();
    for name in requested {
        for entry in expand(name, platform)? {
            if !plan.contains(&entry) {
                plan.push(entry);
            }
        }
    }
    Ok(plan)
}
/// Download every requested component and report what happened.
///
/// The returned flag is false when at least one component could not be installed, so the
/// command exits non-zero while still reporting each component on stdout.
pub fn run(root: &Path, manifest: &str, requested: &[String]) -> Result<(Value, bool)> {
    let platform = platform()?;
    let plan = plan(requested, &platform)?;
    let manager = Components::new(root.to_path_buf(), manifest.to_owned());
    let manifest = manager.manifest(false)?;
    let started = Instant::now();
    let (mut downloaded, mut reused) = (0usize, 0usize);
    let mut failed = Vec::new();
    let mut results = Vec::new();
    for (name, platform) in &plan {
        let asset = match manager.asset(&manifest, name, platform) {
            Ok(asset) => asset,
            Err(error) => {
                eprintln!("{name} ({platform}) is unavailable: {error:#}");
                failed.push(format!("{name}:{platform}"));
                results.push(
                    json!({"component": name, "platform": platform, "status": "unavailable"}),
                );
                continue;
            }
        };
        match manager.ensure_with_status(name, platform, asset) {
            Ok((_, true)) => {
                downloaded += 1;
                results.push(json!({
                    "component": name,
                    "platform": platform,
                    "version": asset.version,
                    "status": "downloaded",
                }));
            }
            Ok((_, false)) => {
                reused += 1;
                eprintln!(
                    "{name} {} for {platform} is already installed",
                    asset.version
                );
                results.push(json!({
                    "component": name,
                    "platform": platform,
                    "version": asset.version,
                    "status": "already_installed",
                }));
            }
            Err(error) => {
                eprintln!("{name} for {platform} failed: {error:#}");
                failed.push(format!("{name}:{platform}"));
                results.push(json!({
                    "component": name,
                    "platform": platform,
                    "version": asset.version,
                    "status": "failed",
                    "error": format!("{error:#}"),
                }));
            }
        }
    }
    let elapsed = started.elapsed();
    if failed.is_empty() {
        eprintln!(
            "Prefetched {downloaded} component(s) in {} ({reused} already installed)",
            human_duration(elapsed)
        );
    } else {
        eprintln!(
            "{} component(s) failed; components that are already installed are reused, so re-run the same command to continue",
            failed.len()
        );
    }
    let report = json!({
        "downloaded": downloaded,
        "already_installed": reused,
        "failed": failed,
        "duration_seconds": elapsed.as_secs_f64(),
        "components": results,
    });
    Ok((report, failed.is_empty()))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn jdbc_targets_pull_the_shared_runtime() {
        let names: Vec<String> = expand("oracle", "macos-arm64")
            .unwrap()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert_eq!(names, ["java", "jdbc", "oracle"]);
        assert_eq!(
            expand("postgres", "macos-arm64").unwrap(),
            [("postgres".to_owned(), "macos-arm64".to_owned())]
        );
        assert_eq!(
            expand("ui", "linux-x64").unwrap(),
            [
                ("ui".to_owned(), "linux-x64".to_owned()),
                ("ui-default".to_owned(), "any".to_owned())
            ]
        );
    }
    #[test]
    fn all_covers_every_choice_once() {
        let all = plan(&["all".to_owned()], "macos-arm64").unwrap();
        for name in [
            "mysql",
            "postgres",
            "ui",
            "ui-default",
            "skill",
            "java",
            "jdbc",
            "oracle",
            "sqlserver",
        ] {
            assert!(all.iter().any(|(entry, _)| entry == name), "missing {name}");
        }
        let mut unique = all.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), all.len(), "duplicate plan entries");
        assert_eq!(
            plan(&["mysql".to_owned(), "all".to_owned()], "macos-arm64")
                .unwrap()
                .len(),
            all.len(),
            "naming all plus one component must not duplicate the plan"
        );
    }
    #[test]
    fn unknown_components_are_rejected() {
        let error = expand("sqlite", "macos-arm64").unwrap_err().to_string();
        assert!(error.contains("unknown component sqlite"), "{error}");
        assert!(error.contains("mysql"), "{error}");
    }
}
