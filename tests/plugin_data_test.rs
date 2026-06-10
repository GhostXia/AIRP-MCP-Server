//! M_PLUGIN_DATA integration tests — zero-schema plugin data primitives.
//!
//! Independent test target so these pass regardless of unrelated breakage in
//! the PNG-card path of `integration_test.rs`.

use serde_json::Value;

mod common;

#[tokio::test]
async fn test_plugin_kv_roundtrip() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    // get before set → present=false, value=null (no error)
    let out = server.handle_plugin_kv_get(serde_json::json!({
        "plugin_name": "dice-roller", "key": "config"
    })).await.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["present"], false);
    assert!(v["value"].is_null());

    // set arbitrary JSON object
    server.handle_plugin_kv_set(serde_json::json!({
        "plugin_name": "dice-roller", "key": "config",
        "value_json": "{\"sides\": 20, \"advantage\": true}"
    })).await.unwrap();
    assert!(ctx.data_dir.join("plugins").join("dice-roller").join("config.json").exists());

    // get after set → present=true, parsed object
    let out = server.handle_plugin_kv_get(serde_json::json!({
        "plugin_name": "dice-roller", "key": "config"
    })).await.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["present"], true);
    assert_eq!(v["value"]["sides"], 20);
}

#[tokio::test]
async fn test_plugin_kv_rejects_invalid_json_and_bad_names() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();
    // invalid JSON value
    assert!(server.handle_plugin_kv_set(serde_json::json!({
        "plugin_name": "p", "key": "k", "value_json": "not json"
    })).await.is_err());
    // plugin_name traversal
    assert!(server.handle_plugin_kv_set(serde_json::json!({
        "plugin_name": "../escape", "key": "k", "value_json": "{}"
    })).await.is_err());
    // key with separator
    assert!(server.handle_plugin_kv_get(serde_json::json!({
        "plugin_name": "p", "key": "a/b"
    })).await.is_err());
}

#[tokio::test]
async fn test_plugin_jsonl_append_and_read() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    // read missing file → empty, no error
    let out = server.handle_plugin_jsonl_read(serde_json::json!({
        "plugin_name": "tracker", "file": "events.jsonl"
    })).await.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["total_lines"], 0);

    // append 3 lines (multi-line JSON compacted to single line each)
    for i in 0..3 {
        server.handle_plugin_jsonl_append(serde_json::json!({
            "plugin_name": "tracker", "file": "events.jsonl",
            "line_json": format!("{{\n  \"event\": {}\n}}", i)
        })).await.unwrap();
    }
    let raw = std::fs::read_to_string(
        ctx.data_dir.join("plugins").join("tracker").join("events.jsonl")).unwrap();
    assert_eq!(raw.lines().count(), 3);

    // offset/limit paging
    let out = server.handle_plugin_jsonl_read(serde_json::json!({
        "plugin_name": "tracker", "file": "events.jsonl", "offset": 1, "limit": 1
    })).await.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["total_lines"], 3);
    assert_eq!(v["returned"], 1);
    assert_eq!(v["lines"][0]["event"], 1);
}

#[tokio::test]
async fn test_plugin_jsonl_subdir_and_traversal_guard() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();
    // subdirectory allowed
    server.handle_plugin_jsonl_append(serde_json::json!({
        "plugin_name": "p", "file": "logs/2026/run.jsonl", "line_json": "{\"ok\":true}"
    })).await.unwrap();
    assert!(ctx.data_dir.join("plugins").join("p").join("logs").join("2026").join("run.jsonl").exists());
    // .. traversal rejected
    assert!(server.handle_plugin_jsonl_append(serde_json::json!({
        "plugin_name": "p", "file": "../../characters/x.jsonl", "line_json": "{}"
    })).await.is_err());
}

#[tokio::test]
async fn test_plugin_blob_text_and_base64_roundtrip() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    // text write
    let out = server.handle_plugin_blob_write(serde_json::json!({
        "plugin_name": "map-maker", "rel_path": "notes/readme.md",
        "content_text": "# Map plugin\nhello"
    })).await.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["encoding"], "text");

    // read back as text
    let out = server.handle_plugin_blob_read(serde_json::json!({
        "plugin_name": "map-maker", "rel_path": "notes/readme.md", "as_text": true
    })).await.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["content_text"], "# Map plugin\nhello");

    // base64 write (binary), base64 read back
    use base64::Engine;
    let bin: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x00, 0xFF];
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bin);
    server.handle_plugin_blob_write(serde_json::json!({
        "plugin_name": "map-maker", "rel_path": "assets/tile.png", "content_base64": b64
    })).await.unwrap();
    let out = server.handle_plugin_blob_read(serde_json::json!({
        "plugin_name": "map-maker", "rel_path": "assets/tile.png", "as_text": false
    })).await.unwrap();
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["content_base64"], b64);
    assert_eq!(v["size"], 6);

    // binary as_text=true → error
    assert!(server.handle_plugin_blob_read(serde_json::json!({
        "plugin_name": "map-maker", "rel_path": "assets/tile.png", "as_text": true
    })).await.is_err());
}

#[tokio::test]
async fn test_plugin_blob_write_requires_exactly_one_content() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();
    // both → error
    assert!(server.handle_plugin_blob_write(serde_json::json!({
        "plugin_name": "p", "rel_path": "f.txt",
        "content_base64": "aGk=", "content_text": "hi"
    })).await.is_err());
    // neither → error
    assert!(server.handle_plugin_blob_write(serde_json::json!({
        "plugin_name": "p", "rel_path": "f.txt"
    })).await.is_err());
}

#[tokio::test]
async fn test_plugin_resources_list_and_data() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();
    server.handle_plugin_kv_set(serde_json::json!({
        "plugin_name": "stats", "key": "summary", "value_json": "{\"runs\": 7}"
    })).await.unwrap();

    // airp://plugins → namespace list
    let out = server.dispatch_resource("airp://plugins").await.unwrap();
    let names: Vec<String> = serde_json::from_str(&out).unwrap();
    assert_eq!(names, vec!["stats"]);

    // airp://plugins/stats/files → file list
    let out = server.dispatch_resource("airp://plugins/stats/files").await.unwrap();
    let files: Vec<String> = serde_json::from_str(&out).unwrap();
    assert_eq!(files, vec!["summary.json"]);

    // airp://plugins/stats/data/summary.json → content
    let out = server.dispatch_resource("airp://plugins/stats/data/summary.json").await.unwrap();
    assert_eq!(out, "{\"runs\": 7}");

    // traversal rejected
    assert!(server.dispatch_resource("airp://plugins/stats/data/../../settings.json").await.is_err());
}
