use serde_json::{json, Value};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn call(root: &std::path::Path, args: &[&str], input: Option<&Value>) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sqlx"));
    cmd.env_remove("SQLX_WORKER_DIR")
        .env_remove("SQLX_MANIFEST")
        .arg("--data-dir")
        .arg(root)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    if let Some(input) = input {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(input).unwrap())
            .unwrap();
    }
    child.wait_with_output().unwrap()
}
fn connection() -> Value {
    json!({"database_type":"mysql","host":"127.0.0.1","port":3306,"database":"fixture","username":"unique_private_username","password":"unique_private_password","tls":"disable"})
}
#[test]
fn datasource_crud_encrypts_and_redacts_credentials() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("data");
    let out = call(
        &root,
        &["datasource", "add", "--name", "demo", "--connection-stdin"],
        Some(&connection()),
    );
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    let id = value["data"]["id"].as_str().unwrap();
    let encrypted = std::fs::read(root.join("datasources.enc")).unwrap();
    assert!(!String::from_utf8_lossy(&encrypted).contains("unique_private"));
    assert!(!String::from_utf8_lossy(&out.stdout).contains("unique_private"));
    assert!(call(
        &root,
        &["datasource", "update", "--id", id, "--name", "renamed"],
        None
    )
    .status
    .success());
    let shown = call(&root, &["datasource", "show", "--id", "renamed"], None);
    let value: Value = serde_json::from_slice(&shown.stdout).unwrap();
    assert_eq!(value["data"]["id"], id);
    assert!(call(&root, &["datasource", "remove", "--id", id], None)
        .status
        .success());
    let list = call(&root, &["datasource", "list"], None);
    let value: Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(value["data"]["datasources"], json!([]));
}
#[test]
fn duplicate_name_and_invalid_update_preserve_existing_data() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("data");
    assert!(call(
        &root,
        &["datasource", "add", "--name", "demo", "--connection-stdin"],
        Some(&connection())
    )
    .status
    .success());
    let before = std::fs::read(root.join("datasources.enc")).unwrap();
    assert!(!call(
        &root,
        &["datasource", "add", "--name", "demo", "--connection-stdin"],
        Some(&connection())
    )
    .status
    .success());
    assert!(!call(
        &root,
        &["datasource", "update", "--id", "demo", "--port", "0"],
        None
    )
    .status
    .success());
    assert_eq!(before, std::fs::read(root.join("datasources.enc")).unwrap());
}
#[test]
fn concurrent_creates_do_not_lose_updates() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("data");
    assert!(call(&root, &["init"], None).status.success());
    let threads = (0..8)
        .map(|i| {
            let root = root.clone();
            std::thread::spawn(move || {
                call(
                    &root,
                    &[
                        "datasource",
                        "add",
                        "--name",
                        &format!("source-{i}"),
                        "--connection-stdin",
                    ],
                    Some(&connection()),
                )
                .status
                .success()
            })
        })
        .collect::<Vec<_>>();
    for t in threads {
        assert!(t.join().unwrap());
    }
    let list = call(&root, &["datasource", "list"], None);
    let value: Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(value["data"]["datasources"].as_array().unwrap().len(), 8);
}
