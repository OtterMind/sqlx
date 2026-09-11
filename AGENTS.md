# SQLX development

Implement the database CLI described in docs/design.md and the local companion in docs/local-ui.md. Keep the main binary free of database driver dependencies. MySQL and PostgreSQL run in separate native workers; Oracle and SQL Server use the JDBC worker. Each invocation owns one connection; each SQL argument is one statement. Preserve full structured results and exact numeric values. Never replay uncertain writes.

Build frontend assets with npm --prefix ui ci and npm --prefix ui run build before workspace Rust commands. Use cargo fmt --check, cargo clippy --workspace --all-targets -- -D warnings, cargo test --workspace, and mvn -f java/jdbc/pom.xml verify as appropriate. Database tests use isolated test fixtures only. Never commit user data, credentials, downloaded drivers, or build outputs. Preserve LICENSE and NOTICE when adapting source.
