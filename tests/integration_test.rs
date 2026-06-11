//! Integration tests for AIRP MCP Server

use std::sync::Arc;
use tempfile::TempDir;
use serde_json::Value;

mod common;

#[tokio::test]
async fn test_storage_init() {
    let ctx = common::TestContext::new().await;
    assert!(ctx.data_dir.exists());
}

#[tokio::test]
async fn test_import_and_list_characters() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    // Start with empty list
    let result = server.handle_list_characters().await.unwrap();
    assert!(result.contains("No characters"));

    // Import a character
    let card = common::create_test_card();
    let png_base64 = common::card_to_base64(&card);

    let import_args = serde_json::json!({"png_base64": png_base64});
    let result = server.handle_import_card(import_args).await.unwrap();
    assert!(result.contains("Successfully imported"));
    assert!(result.contains("TestCharacter"));

    // List should now have one character
    let result = server.handle_list_characters().await.unwrap();
    assert!(result.contains("TestCharacter"));
}

#[tokio::test]
async fn test_session_lifecycle() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    // Import character first
    let card = common::create_test_card();
    let png_base64 = common::card_to_base64(&card);
    let import_args = serde_json::json!({"png_base64": png_base64});
    server.handle_import_card(import_args).await.unwrap();

    // Start session
    let start_args = serde_json::json!({"character_id": "testcharacter"});
    let result = server.handle_start_session(start_args).await.unwrap();
    assert!(result.contains("Session created"));
    assert!(result.contains("testcharacter"));
    assert!(result.contains("Lorebook loaded"));
    assert!(result.contains("Live state"));

    // Extract session ID from result
    let session_id = extract_session_id(&result);

    // Append messages
    let msg1 = serde_json::json!({
        "character_id": "testcharacter",
        "session_id": &session_id,
        "role": "user",
        "content": "Hello!"
    });
    server.handle_append_message(msg1).await.unwrap();

    let msg2 = serde_json::json!({
        "character_id": "testcharacter",
        "session_id": &session_id,
        "role": "assistant",
        "content": "Hi there!"
    });
    server.handle_append_message(msg2).await.unwrap();

    // Get recent context
    let ctx_args = serde_json::json!({
        "character_id": "testcharacter",
        "session_id": &session_id,
        "n": 10
    });
    let result = server.handle_get_recent_context(ctx_args).await.unwrap();
    let messages: Vec<Value> = serde_json::from_str(&result).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[0]["content"], "Hello!");

    // List sessions
    let list_args = serde_json::json!({"character_id": "testcharacter"});
    let result = server.handle_list_sessions(list_args).await.unwrap();
    assert!(result.contains(&session_id));
}

#[tokio::test]
async fn test_seal_volume_and_rollback() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    // Setup
    let card = common::create_test_card();
    let png_base64 = common::card_to_base64(&card);
    server.handle_import_card(serde_json::json!({"png_base64": png_base64})).await.unwrap();

    let start_args = serde_json::json!({"character_id": "testcharacter"});
    let result = server.handle_start_session(start_args).await.unwrap();
    let session_id = extract_session_id(&result);

    // Add messages
    for i in 1..=5 {
        server.handle_append_message(serde_json::json!({
            "character_id": "testcharacter",
            "session_id": &session_id,
            "role": "user",
            "content": format!("Message {}", i)
        })).await.unwrap();
    }

    // Seal volume
    let seal_args = serde_json::json!({
        "character_id": "testcharacter",
        "session_id": &session_id,
        "clear_session": true
    });
    let result = server.handle_seal_volume(seal_args.clone()).await.unwrap();
    assert!(result.contains("sealed successfully"));

    // After clear, session should be empty
    let ctx_args = serde_json::json!({
        "character_id": "testcharacter",
        "session_id": &session_id,
        "n": 10
    });
    let result = server.handle_get_recent_context(ctx_args).await.unwrap();
    let messages: Vec<Value> = serde_json::from_str(&result).unwrap();
    assert!(messages.is_empty());

    // Add 3 messages, then rollback 2
    for i in 1..=3 {
        server.handle_append_message(serde_json::json!({
            "character_id": "testcharacter",
            "session_id": &session_id,
            "role": "user",
            "content": format!("Msg {}", i)
        })).await.unwrap();
    }

    let rollback_args = serde_json::json!({
        "character_id": "testcharacter",
        "session_id": &session_id,
        "n": 2
    });
    let result = server.handle_rollback_messages(rollback_args).await.unwrap();
    assert!(result.contains("Rolled back 2 message(s)"));

    // Should have 1 message left
    let ctx_args = serde_json::json!({
        "character_id": "testcharacter",
        "session_id": &session_id,
        "n": 10
    });
    let result = server.handle_get_recent_context(ctx_args).await.unwrap();
    let messages: Vec<Value> = serde_json::from_str(&result).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0]["content"], "Msg 1");
}

#[tokio::test]
async fn test_state_tracking() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    let card = common::create_test_card();
    let png_base64 = common::card_to_base64(&card);
    server.handle_import_card(serde_json::json!({"png_base64": png_base64})).await.unwrap();

    // Update state
    let state_args = serde_json::json!({
        "character_id": "testcharacter",
        "state_delta": {
            "hp": {"value": 75, "max": 100},
            "mp": {"value": 30, "max": 50},
            "location": "Town Square"
        }
    });
    let result = server.handle_update_state(state_args).await.unwrap();
    assert!(result.contains("State updated"));

    // Get state
    let get_args = serde_json::json!({"character_id": "testcharacter"});
    let result = server.handle_get_live_state(get_args).await.unwrap();
    let state: Value = serde_json::from_str(&result).unwrap();
    assert!(state["values"]["hp"].is_object());
    assert!(state["values"]["location"].is_string());
}

#[tokio::test]
async fn test_lorebook() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    let card = common::create_test_card();
    let png_base64 = common::card_to_base64(&card);
    server.handle_import_card(serde_json::json!({"png_base64": png_base64})).await.unwrap();

    // Update lorebook
    let entries = serde_json::json!({
        "character_id": "testcharacter",
        "entries": [
            {
                "id": "town_square",
                "name": "Town Square",
                "keys": ["town square", "square", "plaza"],
                "content": "The town square is bustling with merchants",
                "enabled": true,
                "insertion_order": 0,
                "case_sensitive": false
            }
        ]
    });
    server.handle_update_lorebook(entries).await.unwrap();

    // Apply lorebook
    let apply_args = serde_json::json!({
        "character_id": "testcharacter",
        "text": "We walked into the town square"
    });
    let result = server.handle_apply_lorebook(apply_args).await.unwrap();
    assert!(result.contains("Town Square"));
    assert!(result.contains("bustling with merchants"));

    // Text that doesn't match
    let apply_args = serde_json::json!({
        "character_id": "testcharacter",
        "text": "Nothing relevant here"
    });
    let result = server.handle_apply_lorebook(apply_args).await.unwrap();
    assert!(result.contains("No lorebook entries matched"));
}

#[tokio::test]
async fn test_analyze_card() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    let card = common::create_test_card();
    let png_base64 = common::card_to_base64(&card);
    server.handle_import_card(serde_json::json!({"png_base64": png_base64})).await.unwrap();

    // Analyze at tier 2
    let analyze_args = serde_json::json!({
        "character_id": "testcharacter",
        "tier": 2
    });
    let result = server.handle_analyze_card(analyze_args).await.unwrap();
    assert!(result.contains("Analysis complete"));
    assert!(result.contains("Tier: 2"));
    assert!(result.contains("analysis/summary.md"));
}

#[tokio::test]
async fn test_decompose_character() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    let card = common::create_test_card();
    let png_base64 = common::card_to_base64(&card);
    server.handle_import_card(serde_json::json!({"png_base64": png_base64})).await.unwrap();

    let decompose_args = serde_json::json!({
        "character_id": "testcharacter",
        "target_dir": "./test_decomposed"
    });
    let result = server.handle_decompose_character(decompose_args).await.unwrap();
    assert!(result.contains("decomposed successfully"));
    assert!(result.contains("basic_info.md"));
    assert!(result.contains("personality.md"));

    // Cleanup
    let _ = std::fs::remove_dir_all("./test_decomposed");
}

#[tokio::test]
async fn test_preset_operations() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    // List presets (empty)
    let result = server.handle_list_presets().await.unwrap();
    assert!(result.contains("No presets"));

    // Create a preset manually via storage
    let preset_store = airp_mcp_server::storage::PresetStore::new(&ctx.storage);
    let preset = airp_mcp_server::models::Preset {
        id: airp_mcp_server::models::PresetId::new("test-preset").unwrap(),
        name: "Test Preset".to_string(),
        config: Default::default(),
    };
    preset_store.save(&preset).await.unwrap();

    // List should show it
    let result = server.handle_list_presets().await.unwrap();
    assert!(result.contains("Test Preset"));

    // Get preset
    let get_args = serde_json::json!({"preset_id": "test-preset"});
    let result = server.handle_get_preset(get_args).await.unwrap();
    let preset_data: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(preset_data["name"], "Test Preset");
}

fn extract_session_id(result: &str) -> String {
    let prefix = "Session created: ";
    let start = result.find(prefix).unwrap() + prefix.len();
    let end = result[start..].find(" for character").unwrap() + start;
    result[start..end].to_string()
}
