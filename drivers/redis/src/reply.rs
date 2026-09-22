//! Map a Redis command line and its reply onto the shared result events.
//!
//! A command line is split the way `redis-cli` splits one, so quoting behaves as users expect. A
//! reply becomes a table: scalars and flat arrays fill one `value` column, arrays of arrays fill
//! `c1..cN`, mixed arrays fill one row, and the commands that answer with field/value pairs fill
//! `field` and `value`.
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::Value as Json;
use sqlx_protocol::Column;

/// Commands whose flat reply alternates between a field and its value.
const PAIR_COMMANDS: [&str; 5] = ["HGETALL", "CONFIG", "HRANDFIELD", "ZRANGE", "ZREVRANGE"];

/// Split a command line into arguments, honouring single quotes, double quotes and backslash escapes.
pub fn split_command(line: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut started = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' | '\n' | '\r' if !started || !current.is_empty() || !started => {
                if !current.is_empty() || started {
                    args.push(std::mem::take(&mut current));
                    started = false;
                }
            }
            ' ' | '\t' | '\n' | '\r' => {}
            '\'' => {
                started = true;
                for inner in chars.by_ref() {
                    if inner == '\'' {
                        break;
                    }
                    current.push(inner);
                }
            }
            '"' => {
                started = true;
                while let Some(inner) = chars.next() {
                    match inner {
                        '"' => break,
                        '\\' => match chars.next() {
                            Some('n') => current.push('\n'),
                            Some('r') => current.push('\r'),
                            Some('t') => current.push('\t'),
                            Some('x') => {
                                let digits: String = chars.by_ref().take(2).collect();
                                match u8::from_str_radix(&digits, 16) {
                                    Ok(byte) => current.push(byte as char),
                                    Err(_) => return Err(format!("invalid hex escape \\x{digits}")),
                                }
                            }
                            Some(other) => current.push(other),
                            None => return Err("unterminated escape".into()),
                        },
                        other => current.push(other),
                    }
                }
            }
            other => {
                started = true;
                current.push(other);
            }
        }
    }
    if started {
        args.push(current);
    }
    if args.is_empty() {
        return Err("a command is required".into());
    }
    Ok(args)
}

/// Whether this command answers with a flat list of field/value pairs.
fn pairs_with(command: &str, arity: usize) -> bool {
    let name = command.to_ascii_uppercase();
    if !PAIR_COMMANDS.contains(&name.as_str()) {
        return false;
    }
    // CONFIG GET and the WITHSCORES variants are the pair-shaped forms; plain ZRANGE is not.
    arity % 2 == 0
        && (name == "HGETALL" || name == "CONFIG" || name == "HRANDFIELD" || name == "ZRANGE" || name == "ZREVRANGE")
}

/// Render one reply element as a cell. Nested structures keep their JSON text.
fn cell(value: &RedisCell, base64: bool) -> Json {
    match value {
        RedisCell::Nil => Json::Null,
        RedisCell::Text(text) => {
            if base64 {
                Json::String(STANDARD.encode(text.as_bytes()))
            } else {
                Json::String(text.clone())
            }
        }
        RedisCell::Bytes(bytes) => match std::str::from_utf8(bytes) {
            Ok(text) if !base64 => Json::String(text.to_owned()),
            _ => Json::String(STANDARD.encode(bytes)),
        },
        RedisCell::Integer(number) => Json::String(number.to_string()),
        RedisCell::Array(items) => Json::String(render_json(items)),
        RedisCell::Status(text) => Json::String(text.clone()),
    }
}

fn render_json(items: &[RedisCell]) -> String {
    let rendered: Vec<Json> = items.iter().map(|item| cell(item, false)).collect();
    Json::Array(rendered).to_string()
}

/// A reply element, independent of the Redis crate's own value type so the rules stay testable.
#[derive(Clone, Debug, PartialEq)]
pub enum RedisCell {
    Nil,
    Text(String),
    Bytes(Vec<u8>),
    Integer(i64),
    Status(String),
    Array(Vec<RedisCell>),
}

/// A reply mapped onto the event stream.
pub struct Mapping {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Json>>,
}

fn column(name: &str, database_type: &str, encoding: &str) -> Column {
    Column {
        name: name.to_owned(),
        database_type: database_type.to_owned(),
        encoding: encoding.to_owned(),
    }
}

/// Whether a flat reply needs base64, because at least one string is not valid UTF-8.
fn needs_base64(cells: &[RedisCell]) -> bool {
    cells.iter().any(|item| match item {
        RedisCell::Bytes(bytes) => std::str::from_utf8(bytes).is_err(),
        _ => false,
    })
}

/// Map a reply onto columns and rows using the rules documented in `references/redis.md`.
pub fn map_reply(command: &str, reply: &RedisCell) -> Mapping {
    match reply {
        RedisCell::Nil => Mapping {
            columns: vec![column("value", "nil", "string")],
            rows: vec![vec![Json::Null]],
        },
        RedisCell::Integer(number) => Mapping {
            columns: vec![column("value", "integer", "string")],
            rows: vec![vec![Json::String(number.to_string())]],
        },
        RedisCell::Status(text) | RedisCell::Text(text) => Mapping {
            columns: vec![column("value", "string", "string")],
            rows: vec![vec![Json::String(text.clone())]],
        },
        RedisCell::Bytes(bytes) => {
            let base64 = std::str::from_utf8(bytes).is_err();
            Mapping {
                columns: vec![column("value", "string", if base64 { "base64" } else { "string" })],
                rows: vec![vec![cell(reply, base64)]],
            }
        }
        RedisCell::Array(items) => {
            if items.is_empty() {
                return Mapping {
                    columns: vec![column("value", "string", "string")],
                    rows: vec![],
                };
            }
            if items.iter().all(|item| matches!(item, RedisCell::Array(_))) {
                let width = items
                    .iter()
                    .map(|item| match item {
                        RedisCell::Array(inner) => inner.len(),
                        _ => 0,
                    })
                    .max()
                    .unwrap_or(0);
                let columns: Vec<Column> = (1..=width)
                    .map(|index| column(&format!("c{index}"), "string", "string"))
                    .collect();
                let rows = items
                    .iter()
                    .map(|item| match item {
                        RedisCell::Array(inner) => (0..width)
                            .map(|index| inner.get(index).map(|value| cell(value, false)).unwrap_or(Json::Null))
                            .collect(),
                        _ => vec![Json::Null; width],
                    })
                    .collect();
                return Mapping { columns, rows };
            }
            if items.iter().any(|item| matches!(item, RedisCell::Array(_))) {
                let columns: Vec<Column> = (1..=items.len())
                    .map(|index| column(&format!("c{index}"), "string", "string"))
                    .collect();
                let row: Vec<Json> = items.iter().map(|item| cell(item, false)).collect();
                return Mapping { columns, rows: vec![row] };
            }
            let base64 = needs_base64(items);
            let mut columns = vec![column("value", "string", if base64 { "base64" } else { "string" })];
            if pairs_with(command, items.len()) {
                columns = vec![
                    column("field", "string", "string"),
                    column("value", "string", if base64 { "base64" } else { "string" }),
                ];
                let rows = items
                    .chunks(2)
                    .map(|pair| vec![cell(&pair[0], false), cell(&pair[1], base64)])
                    .collect();
                return Mapping { columns, rows };
            }
            let rows = items.iter().map(|item| vec![cell(item, base64)]).collect();
            Mapping { columns, rows }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn splits_plain_and_quoted_arguments() {
        assert_eq!(split_command("GET foo").unwrap(), ["GET", "foo"]);
        assert_eq!(
            split_command("SET greeting 'hello world'").unwrap(),
            ["SET", "greeting", "hello world"]
        );
        assert_eq!(
            split_command(r#"SET key "line\nbreak""#).unwrap(),
            ["SET", "key", "line\nbreak"]
        );
        assert!(split_command("   ").is_err());
    }

    #[test]
    fn maps_scalars_and_arrays() {
        let scalar = map_reply("GET", &RedisCell::Text("hello".into()));
        assert_eq!(scalar.columns[0].name, "value");
        assert_eq!(scalar.rows, vec![vec![json!("hello")]]);

        let array = map_reply(
            "LRANGE",
            &RedisCell::Array(vec![RedisCell::Text("a".into()), RedisCell::Nil]),
        );
        assert_eq!(array.rows, vec![vec![json!("a")], vec![Json::Null]]);

        let count = map_reply("DEL", &RedisCell::Integer(3));
        assert_eq!(count.columns[0].database_type, "integer");
        assert_eq!(count.rows, vec![vec![json!("3")]]);
    }

    #[test]
    fn maps_pairs_nested_and_mixed_replies() {
        let pairs = map_reply(
            "HGETALL",
            &RedisCell::Array(vec![
                RedisCell::Text("name".into()),
                RedisCell::Text("ada".into()),
                RedisCell::Text("role".into()),
                RedisCell::Text("engineer".into()),
            ]),
        );
        assert_eq!(pairs.columns.len(), 2);
        assert_eq!(pairs.columns[0].name, "field");
        assert_eq!(pairs.rows.len(), 2);

        let nested = map_reply(
            "XRANGE",
            &RedisCell::Array(vec![
                RedisCell::Array(vec![RedisCell::Text("1-1".into()), RedisCell::Text("a".into())]),
                RedisCell::Array(vec![RedisCell::Text("2-2".into())]),
            ]),
        );
        assert_eq!(nested.columns.len(), 2);
        assert_eq!(nested.rows.len(), 2);
        assert_eq!(nested.rows[1], vec![json!("2-2"), Json::Null]);

        let scan = map_reply(
            "SCAN",
            &RedisCell::Array(vec![
                RedisCell::Text("0".into()),
                RedisCell::Array(vec![RedisCell::Text("a".into())]),
            ]),
        );
        assert_eq!(scan.rows.len(), 1);
        assert_eq!(scan.rows[0][0], json!("0"));
        assert_eq!(scan.rows[0][1], json!("[\"a\"]"));
    }

    #[test]
    fn encodes_binary_strings_as_base64() {
        let binary = RedisCell::Bytes(vec![0xff, 0x00, 0xfe]);
        let mapping = map_reply("GET", &binary);
        assert_eq!(mapping.columns[0].encoding, "base64");
        assert_eq!(mapping.rows[0][0], json!("/wD+"));
    }
}
