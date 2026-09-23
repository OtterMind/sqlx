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
#[test]
fn import_merges_documents_reports_skips_and_keeps_credentials_secret() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("data");
    let document = json!({"version":1,"mode":"merge","datasources":[
        {"name":"imported","connection":connection()},
        {"name":"hive","connection":{"database_type":"hive","host":"h","port":10000}},
        {"name":"","connection":connection()}
    ]});
    let out = call(&root, &["datasource", "import", "--stdin"], Some(&document));
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let value: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["success"], true);
    assert_eq!(value["data"]["added"], 1);
    assert_eq!(value["data"]["updated"], 0);
    assert_eq!(value["data"]["skipped"][0]["name"], "hive");
    assert_eq!(value["data"]["skipped"][0]["reason"], "invalid_connection");
    assert_eq!(value["data"]["skipped"][1]["reason"], "invalid_name");
    // Credentials stay out of the report and off the disk in clear text.
    assert!(!String::from_utf8_lossy(&out.stdout).contains("unique_private"));
    let encrypted = std::fs::read(root.join("datasources.enc")).unwrap();
    assert!(!String::from_utf8_lossy(&encrypted).contains("unique_private"));
    // Importing the same document again changes nothing.
    let again = call(&root, &["datasource", "import", "--stdin"], Some(&document));
    let value: Value = serde_json::from_slice(&again.stdout).unwrap();
    assert_eq!(value["data"]["added"], 0);
    assert_eq!(value["data"]["updated"], 0);
    assert_eq!(value["data"]["unchanged"], 1);
    // A dry run reports without storing.
    let dry = call(
        &root,
        &["datasource", "import", "--stdin", "--dry-run"],
        Some(&json!({"version":1,"datasources":[{"name":"later","connection":connection()}]})),
    );
    let value: Value = serde_json::from_slice(&dry.stdout).unwrap();
    assert_eq!(value["data"]["dry_run"], true);
    assert_eq!(value["data"]["added"], 1);
    // A strict import refuses the whole document and keeps the stored data unchanged.
    let before = std::fs::read(root.join("datasources.enc")).unwrap();
    let strict = call(
        &root,
        &["datasource", "import", "--stdin", "--strict"],
        Some(&json!({"version":1,"datasources":[
            {"name":"fine","connection":connection()},
            {"name":"broken","connection":{"database_type":"postgresql","port":5432}}
        ]})),
    );
    assert!(!strict.status.success());
    assert_eq!(before, std::fs::read(root.join("datasources.enc")).unwrap());
    // A file document imports the same way for callers that cannot pipe stdin.
    let path = temp.path().join("connections.json");
    std::fs::write(
        &path,
        serde_json::to_vec(
            &json!({"version":1,"datasources":[{"name":"from-file","connection":connection()}]}),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(call(
        &root,
        &["datasource", "import", "--file", path.to_str().unwrap()],
        None
    )
    .status
    .success());
    let list = call(&root, &["datasource", "list"], None);
    let value: Value = serde_json::from_slice(&list.stdout).unwrap();
    let names = value["data"]["datasources"]
        .as_array()
        .unwrap()
        .iter()
        .map(|source| source["name"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["imported".to_string(), "from-file".to_string()]);
}

#[test]
fn documented_import_example_stays_valid() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("data");
    let example = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
        .join("import-connections.json");
    let example = example.to_str().unwrap();
    assert!(std::path::Path::new(example).is_file(), "{example}");

    // The published example must validate and must not store anything on a dry run.
    let dry = call(
        &root,
        &["datasource", "import", "--file", example, "--dry-run"],
        None,
    );
    assert!(
        dry.status.success(),
        "{}",
        String::from_utf8_lossy(&dry.stdout)
    );
    let value: Value = serde_json::from_slice(&dry.stdout).unwrap();
    assert_eq!(value["data"]["dry_run"], true);
    assert_eq!(value["data"]["added"], 3);
    assert_eq!(value["data"]["skipped"], json!([]));
    let list = call(&root, &["datasource", "list"], None);
    let value: Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(value["data"]["datasources"], json!([]));

    // Importing it for real adds every entry and resolves the file path.
    assert!(
        call(&root, &["datasource", "import", "--file", example], None)
            .status
            .success()
    );
    let list = call(&root, &["datasource", "list"], None);
    let value: Value = serde_json::from_slice(&list.stdout).unwrap();
    let sources = value["data"]["datasources"].as_array().unwrap();
    assert_eq!(sources.len(), 3);
    let sqlite = sources
        .iter()
        .find(|source| source["name"] == "local-sqlite")
        .unwrap();
    let path = sqlite["connection"]["database"].as_str().unwrap();
    assert!(std::path::Path::new(path).is_absolute(), "{path}");
    assert!(path.ends_with("data/app.db"), "{path}");
}
