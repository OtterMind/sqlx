//! Map a MongoDB command reply onto the shared result events.
//!
//! The rules live here so they can be tested without a server. A reply with a cursor is followed
//! to the end, or to [`MAX_DOCUMENTS`], whichever comes first; a reply with a `value` becomes one
//! row; a write reply only reports how many documents it touched. Numbers keep their exact text,
//! and nested documents and arrays keep canonical extended JSON text.
use anyhow::{anyhow, bail, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use mongodb::bson::{Bson, Document};
use serde_json::Value as Json;
use sqlx_protocol::Column;

/// The worker stops following a cursor after this many documents.
pub const MAX_DOCUMENTS: usize = 10_000;

/// Fetches the next batch of a cursor, the way the worker sends
/// `{ "getMore": <cursor_id>, "collection": <collection> }`.
pub trait BatchFetcher {
    fn fetch(
        &self,
        cursor_id: i64,
        collection: &str,
    ) -> impl std::future::Future<Output = Result<Document>>;
}

/// A command the server rejected, with the code and message the worker reports.
pub struct Rejected {
    pub code: String,
    pub message: String,
}

/// One mapped result. `affected_rows` is set only when the reply reported a write count.
pub struct Mapping {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Json>>,
    pub affected_rows: Option<String>,
}

/// Map one reply, following a cursor with `fetcher` when the reply carries one.
pub async fn map_reply<F: BatchFetcher>(reply: &Document, fetcher: &F) -> Result<Mapping> {
    if let Some(cursor) = reply.get("cursor") {
        let documents = follow(cursor, fetcher).await?;
        return Ok(documents_mapping(&documents, None));
    }
    if let Some(value) = reply.get("value") {
        return Ok(match value {
            Bson::Document(document) => documents_mapping(std::slice::from_ref(document), None),
            _ => empty_mapping(None),
        });
    }
    Ok(empty_mapping(affected_rows(reply)))
}

/// A rejection that still arrives as a successful reply: `ok` other than 1, a `writeErrors`
/// entry, or a write concern error.
pub fn rejection(reply: &Document) -> Option<Rejected> {
    if !accepted(reply) {
        return Some(rejected(reply));
    }
    if let Some(document) = reply
        .get_array("writeErrors")
        .ok()
        .and_then(|errors| errors.iter().find_map(Bson::as_document))
    {
        return Some(rejected(document));
    }
    reply.get_document("writeConcernError").ok().map(rejected)
}

/// `ok` is 1, or absent, when the server accepted the command.
fn accepted(reply: &Document) -> bool {
    reply.get("ok").is_none_or(|ok| match ok {
        Bson::Boolean(flag) => *flag,
        other => integer(Some(other)) != Some(0),
    })
}

fn rejected(document: &Document) -> Rejected {
    Rejected {
        code: server_code(document).unwrap_or_else(|| "mongodb.execution_failed".into()),
        message: document
            .get_str("errmsg")
            .unwrap_or("the server rejected the command")
            .to_owned(),
    }
}

/// `mongodb.<codeName>` when the server named the code, else `mongodb.<code>`.
fn server_code(document: &Document) -> Option<String> {
    if let Ok(name) = document.get_str("codeName") {
        if !name.is_empty() {
            return Some(format!("mongodb.{name}"));
        }
    }
    match document.get("code") {
        Some(Bson::Int32(code)) => Some(format!("mongodb.{code}")),
        Some(Bson::Int64(code)) => Some(format!("mongodb.{code}")),
        _ => None,
    }
}

/// Follow a cursor to the end, or to [`MAX_DOCUMENTS`], whichever comes first.
async fn follow<F: BatchFetcher>(cursor: &Bson, fetcher: &F) -> Result<Vec<Document>> {
    let cursor = cursor
        .as_document()
        .ok_or_else(|| anyhow!("the cursor field is not a document"))?;
    let mut id = integer(cursor.get("id")).unwrap_or(0);
    let collection = collection(cursor.get("ns"));
    let mut documents = batch(cursor.get("firstBatch"))?;
    while id != 0 && documents.len() < MAX_DOCUMENTS {
        let reply = fetcher.fetch(id, &collection).await?;
        let cursor = reply
            .get("cursor")
            .and_then(Bson::as_document)
            .ok_or_else(|| anyhow!("the getMore reply has no cursor"))?;
        id = integer(cursor.get("id")).unwrap_or(0);
        documents.extend(batch(cursor.get("nextBatch"))?);
    }
    documents.truncate(MAX_DOCUMENTS);
    Ok(documents)
}

/// The documents of one `firstBatch` or `nextBatch`.
fn batch(value: Option<&Bson>) -> Result<Vec<Document>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Bson::Array(items) = value else {
        bail!("the cursor batch is not an array");
    };
    items
        .iter()
        .map(|item| {
            item.as_document()
                .cloned()
                .ok_or_else(|| anyhow!("the cursor batch holds a value that is not a document"))
        })
        .collect()
}

/// The collection of a cursor namespace such as `sqlx_probe.recipe_values`.
fn collection(namespace: Option<&Bson>) -> String {
    namespace
        .and_then(Bson::as_str)
        .map(|namespace| {
            namespace
                .split_once('.')
                .map_or(namespace, |(_, collection)| collection)
                .to_owned()
        })
        .unwrap_or_default()
}

/// The columns are the union of the top-level field names of every document, in first-appearance
/// order with `_id` first; a column that holds a nested document or array in any row is JSON text.
fn documents_mapping(documents: &[Document], affected: Option<String>) -> Mapping {
    let mut names: Vec<&str> = Vec::new();
    let mut nested: Vec<bool> = Vec::new();
    for document in documents {
        for (name, value) in document {
            match names.iter().position(|known| *known == name.as_str()) {
                Some(index) => nested[index] |= nested_value(value),
                None => {
                    names.push(name.as_str());
                    nested.push(nested_value(value));
                }
            }
        }
    }
    if let Some(index) = names.iter().position(|name| *name == "_id") {
        let id = names.remove(index);
        names.insert(0, id);
        let id = nested.remove(index);
        nested.insert(0, id);
    }
    let columns = names
        .iter()
        .zip(&nested)
        .map(|(name, nested)| column(name, if *nested { "json" } else { "string" }))
        .collect();
    let rows = documents
        .iter()
        .map(|document| {
            names
                .iter()
                .map(|name| cell(document.get(*name)))
                .collect()
        })
        .collect();
    Mapping {
        columns,
        rows,
        affected_rows: affected,
    }
}

fn empty_mapping(affected: Option<String>) -> Mapping {
    Mapping {
        columns: Vec::new(),
        rows: Vec::new(),
        affected_rows: affected,
    }
}

fn column(name: &str, encoding: &str) -> Column {
    Column {
        name: name.to_owned(),
        database_type: "bson".to_owned(),
        encoding: encoding.to_owned(),
    }
}

/// Whether a value is a nested document or array, which a column keeps as JSON text.
fn nested_value(value: &Bson) -> bool {
    matches!(value, Bson::Document(_) | Bson::Array(_))
}

/// One cell. Numbers stay text so large integers survive, and nested values become canonical
/// extended JSON text so `$oid`, `$numberLong` and `$date` survive.
fn cell(value: Option<&Bson>) -> Json {
    let Some(value) = value else {
        return Json::Null;
    };
    match value {
        Bson::Null | Bson::Undefined => Json::Null,
        Bson::Boolean(flag) => Json::Bool(*flag),
        Bson::String(text) => Json::String(text.clone()),
        Bson::Int32(number) => Json::String(number.to_string()),
        Bson::Int64(number) => Json::String(number.to_string()),
        Bson::Double(number) => Json::String(number.to_string()),
        Bson::Decimal128(number) => Json::String(number.to_string()),
        Bson::ObjectId(id) => Json::String(id.to_hex()),
        Bson::DateTime(date) => Json::String(rfc3339_millis(date.timestamp_millis())),
        Bson::Binary(binary) => Json::String(STANDARD.encode(&binary.bytes)),
        other => Json::String(other.clone().into_canonical_extjson().to_string()),
    }
}

/// RFC 3339 in UTC with milliseconds, for example `2024-05-06T07:08:09.123Z`.
fn rfc3339_millis(millis: i64) -> String {
    let days = millis.div_euclid(86_400_000);
    let time = millis.rem_euclid(86_400_000);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        time / 3_600_000,
        (time / 60_000) % 60,
        (time / 1_000) % 60,
        time % 1_000
    )
}

/// The proleptic Gregorian date of a day number since 1970-01-01, after Howard Hinnant's
/// `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let shift = days + 719_468;
    let era = shift.div_euclid(146_097);
    let day_of_era = shift.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_position = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_position + 2) / 5 + 1;
    let month = if month_position < 10 {
        month_position + 3
    } else {
        month_position - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// The reply's `n`, the number of documents a write touched.
fn affected_rows(reply: &Document) -> Option<String> {
    match reply.get("n")? {
        Bson::Int32(number) => Some(number.to_string()),
        Bson::Int64(number) => Some(number.to_string()),
        Bson::Double(number) => Some(number.to_string()),
        Bson::Decimal128(number) => Some(number.to_string()),
        _ => None,
    }
}

/// The integer value of a BSON number, for cursor ids and counts.
fn integer(value: Option<&Bson>) -> Option<i64> {
    match value? {
        Bson::Int32(number) => Some(i64::from(*number)),
        Bson::Int64(number) => Some(*number),
        Bson::Double(number) => Some(*number as i64),
        Bson::Decimal128(number) => number.to_string().parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mongodb::bson::{doc, oid::ObjectId, Binary, DateTime, Decimal128};
    use serde_json::json;
    use std::cell::Cell;

    /// A cursor loop that never touches a server: every call answers with another batch, until the
    /// configured last page reports cursor id 0.
    struct FakeFetcher {
        pages: usize,
        per_page: usize,
        calls: Cell<usize>,
    }
    impl FakeFetcher {
        fn new(pages: usize, per_page: usize) -> Self {
            Self {
                pages,
                per_page,
                calls: Cell::new(0),
            }
        }
    }
    impl BatchFetcher for FakeFetcher {
        async fn fetch(&self, cursor_id: i64, collection: &str) -> Result<Document> {
            assert_eq!(collection, "coll");
            let call = self.calls.get() + 1;
            self.calls.set(call);
            let documents: Vec<Bson> = (0..self.per_page)
                .map(|index| Bson::Document(doc! { "i": (call * self.per_page + index) as i32 }))
                .collect();
            Ok(doc! {
                "cursor": {
                    "id": if call == self.pages { 0_i64 } else { cursor_id },
                    "ns": "db.coll",
                    "nextBatch": documents,
                },
                "ok": 1,
            })
        }
    }
    fn first_page() -> Document {
        doc! {
            "cursor": { "id": 7_i64, "ns": "sqlx_probe.coll", "firstBatch": [doc! {"_id": 1_i32}] },
            "ok": 1,
        }
    }

    #[test]
    fn unions_fields_with_id_first_and_marks_nested_columns() {
        let documents = vec![
            doc! {"label": "a", "big": 9007199254740993_i64, "nested": {"a": [1_i32, 2_i32]}},
            doc! {"_id": 1_i32, "label": "b", "extra": true},
        ];
        let mapping = documents_mapping(&documents, None);
        let names: Vec<&str> = mapping.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["_id", "label", "big", "nested", "extra"]);
        let encodings: Vec<&str> = mapping.columns.iter().map(|c| c.encoding.as_str()).collect();
        assert_eq!(encodings, ["string", "string", "string", "json", "string"]);
        assert!(mapping
            .columns
            .iter()
            .all(|column| column.database_type == "bson"));
        assert_eq!(
            mapping.rows[0],
            vec![
                Json::Null,
                json!("a"),
                json!("9007199254740993"),
                json!(r#"{"a":[{"$numberInt":"1"},{"$numberInt":"2"}]}"#),
                Json::Null,
            ]
        );
        assert_eq!(
            mapping.rows[1],
            vec![json!("1"), json!("b"), Json::Null, Json::Null, json!(true)]
        );
    }

    #[test]
    fn renders_object_id_date_time_and_binary() {
        let object_id = ObjectId::parse_str("507f1f77bcf86cd799439011").unwrap();
        let documents = vec![doc! {
            "oid": object_id,
            "date": DateTime::from_millis(1_700_000_000_123),
            "binary": Binary { subtype: mongodb::bson::spec::BinarySubtype::Generic, bytes: vec![0xff, 0x00, 0xfe] },
            "decimal": "123.45".parse::<Decimal128>().unwrap(),
            "double": 1.5_f64,
        }];
        let mapping = documents_mapping(&documents, None);
        assert_eq!(
            mapping.rows[0],
            vec![
                json!("507f1f77bcf86cd799439011"),
                json!("2023-11-14T22:13:20.123Z"),
                json!("/wD+"),
                json!("123.45"),
                json!("1.5"),
            ]
        );
    }

    #[test]
    fn formats_milliseconds_in_utc() {
        assert_eq!(rfc3339_millis(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(rfc3339_millis(-1), "1969-12-31T23:59:59.999Z");
        assert_eq!(rfc3339_millis(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }

    #[tokio::test]
    async fn write_and_empty_replies_report_affected_rows() {
        let fetcher = FakeFetcher::new(0, 0);
        let write = map_reply(&doc! {"n": 3_i32, "ok": 1}, &fetcher).await.unwrap();
        assert!(write.columns.is_empty());
        assert!(write.rows.is_empty());
        assert_eq!(write.affected_rows.as_deref(), Some("3"));
        let empty = map_reply(&doc! {"ok": 1}, &fetcher).await.unwrap();
        assert_eq!(empty.affected_rows, None);
        assert_eq!(fetcher.calls.get(), 0);
    }

    #[tokio::test]
    async fn find_and_modify_value_becomes_one_row_or_nothing() {
        let fetcher = FakeFetcher::new(0, 0);
        let modified = map_reply(
            &doc! {
                "lastErrorObject": {"n": 1_i32, "updatedExisting": true},
                "value": {"_id": 1_i32, "label": "a"},
                "ok": 1,
            },
            &fetcher,
        )
        .await
        .unwrap();
        assert_eq!(modified.rows.len(), 1);
        assert_eq!(modified.columns[0].name, "_id");
        assert_eq!(modified.affected_rows, None);

        let missing = map_reply(&doc! {"value": Bson::Null, "ok": 1}, &fetcher)
            .await
            .unwrap();
        assert!(missing.columns.is_empty());
        assert!(missing.rows.is_empty());
    }

    #[tokio::test]
    async fn follows_a_cursor_until_the_server_reports_zero() {
        let fetcher = FakeFetcher::new(2, 1);
        let mapping = map_reply(&first_page(), &fetcher).await.unwrap();
        assert_eq!(mapping.rows.len(), 3);
        assert_eq!(fetcher.calls.get(), 2);
    }

    #[tokio::test]
    async fn caps_a_cursor_at_ten_thousand_documents() {
        let fetcher = FakeFetcher::new(usize::MAX, 2_500);
        let mapping = map_reply(&first_page(), &fetcher).await.unwrap();
        assert_eq!(mapping.rows.len(), MAX_DOCUMENTS);
        // The cursor never reported id 0, so the cap is what stopped the loop.
        assert_eq!(fetcher.calls.get(), 4);
    }

    #[test]
    fn reports_server_rejections() {
        let write_error = doc! {
            "n": 0_i32,
            "writeErrors": [
                doc! {"index": 0_i32, "code": 11000_i32, "codeName": "DuplicateKey", "errmsg": "E11000 duplicate key error"},
            ],
            "ok": 1,
        };
        let rejected = rejection(&write_error).unwrap();
        assert_eq!(rejected.code, "mongodb.DuplicateKey");
        assert_eq!(rejected.message, "E11000 duplicate key error");
        assert!(rejection(&doc! {"ok": 1, "n": 1_i32}).is_none());

        let not_ok = doc! {"ok": 0_i32, "code": 59_i32, "errmsg": "no such command: 'bogus'"};
        let rejected = rejection(&not_ok).unwrap();
        assert_eq!(rejected.code, "mongodb.59");
        assert_eq!(rejected.message, "no such command: 'bogus'");
    }

    #[test]
    fn maps_namespaces_and_integers() {
        assert_eq!(collection(Some(&Bson::String("db.coll".into()))), "coll");
        assert_eq!(collection(Some(&Bson::String("coll".into()))), "coll");
        assert_eq!(collection(None), "");
        assert_eq!(integer(Some(&Bson::Int64(-2))), Some(-2));
        assert_eq!(integer(Some(&Bson::Double(2.0))), Some(2));
        assert_eq!(integer(Some(&Bson::Null)), None);
    }
}
