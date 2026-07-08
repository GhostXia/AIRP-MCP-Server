//! Integration tests for AIRP MCP Server

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
    server
        .handle_import_card(serde_json::json!({"png_base64": png_base64}))
        .await
        .unwrap();

    let start_args = serde_json::json!({"character_id": "testcharacter"});
    let result = server.handle_start_session(start_args).await.unwrap();
    let session_id = extract_session_id(&result);

    // Add messages
    for i in 1..=5 {
        server
            .handle_append_message(serde_json::json!({
                "character_id": "testcharacter",
                "session_id": &session_id,
                "role": "user",
                "content": format!("Message {}", i)
            }))
            .await
            .unwrap();
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
        server
            .handle_append_message(serde_json::json!({
                "character_id": "testcharacter",
                "session_id": &session_id,
                "role": "user",
                "content": format!("Msg {}", i)
            }))
            .await
            .unwrap();
    }

    let rollback_args = serde_json::json!({
        "character_id": "testcharacter",
        "session_id": &session_id,
        "n": 2
    });
    let result = server
        .handle_rollback_messages(rollback_args)
        .await
        .unwrap();
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
    server
        .handle_import_card(serde_json::json!({"png_base64": png_base64}))
        .await
        .unwrap();

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
    server
        .handle_import_card(serde_json::json!({"png_base64": png_base64}))
        .await
        .unwrap();

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

// ── Issue #28: lorebook_path — server-side file read ──────────────────

#[tokio::test]
async fn test_update_lorebook_via_path() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    let card = common::create_test_card();
    let png_base64 = common::card_to_base64(&card);
    server
        .handle_import_card(serde_json::json!({"png_base64": png_base64}))
        .await
        .unwrap();

    // SillyTavern world book format: entries as an object map keyed by ID.
    // This is the format users will point lorebook_path at.
    let lorebook_json = serde_json::json!({
        "entries": {
            "0": {
                "comment": "Castle",
                "keys": ["castle", "fortress"],
                "content": "The castle looms over the valley.",
                "enabled": true,
                "insertion_order": 0,
                "case_sensitive": false,
                "constant": false
            },
            "1": {
                "comment": "Always-On Lore",
                "keys": ["never_matches"],
                "content": "Background magic permeates everything.",
                "enabled": true,
                "insertion_order": 10,
                "case_sensitive": false,
                "constant": true
            }
        }
    })
    .to_string();

    let lb_file = ctx.data_dir.join("my_lorebook.json");
    std::fs::write(&lb_file, &lorebook_json).unwrap();

    let result = server
        .handle_update_lorebook(serde_json::json!({
            "character_id": "testcharacter",
            "lorebook_path": lb_file.to_string_lossy()
        }))
        .await
        .unwrap();
    // New JSON return format: {"entries": 2, "source": "lorebook_path", "bytes_read": N, ...}
    let resp: Value = serde_json::from_str(&result).expect("update_lorebook should return JSON");
    assert_eq!(resp["entries"], 2, "should report 2 entries: {result}");
    assert_eq!(resp["source"], "lorebook_path");
    assert!(
        resp["bytes_read"].as_u64().unwrap_or_default() > 0,
        "bytes_read should be non-zero"
    );

    // Verify the entries were imported: keyword-gated + constant.
    let result = server
        .handle_apply_lorebook(serde_json::json!({
            "character_id": "testcharacter",
            "text": "completely unrelated text"
        }))
        .await
        .unwrap();
    // Constant entry should be included even without keyword match.
    assert!(
        result.contains("Always-On Lore"),
        "constant entry via lorebook_path must be included: {result}"
    );
    // comment → name mapping should have worked.
    assert!(result.contains("Background magic permeates everything."));

    // Keyword-gated entry should match on relevant text.
    let result = server
        .handle_apply_lorebook(serde_json::json!({
            "character_id": "testcharacter",
            "text": "We approached the castle"
        }))
        .await
        .unwrap();
    assert!(result.contains("Castle"));
    assert!(result.contains("looms over the valley"));

    // Providing neither entries nor lorebook_path should fail.
    assert!(
        server
            .handle_update_lorebook(serde_json::json!({
                "character_id": "testcharacter"
            }))
            .await
            .is_err(),
        "must provide one of entries / lorebook_path"
    );

    // Providing both should also fail.
    assert!(
        server
            .handle_update_lorebook(serde_json::json!({
                "character_id": "testcharacter",
                "entries": [],
                "lorebook_path": lb_file.to_string_lossy()
            }))
            .await
            .is_err(),
        "must not provide both entries and lorebook_path"
    );
}

#[tokio::test]
async fn test_analyze_card() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    let card = common::create_test_card();
    let png_base64 = common::card_to_base64(&card);
    server
        .handle_import_card(serde_json::json!({"png_base64": png_base64}))
        .await
        .unwrap();

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
    server
        .handle_import_card(serde_json::json!({"png_base64": png_base64}))
        .await
        .unwrap();

    let decompose_args = serde_json::json!({
        "character_id": "testcharacter",
        "target_dir": "./test_decomposed"
    });
    let result = server
        .handle_decompose_character(decompose_args)
        .await
        .unwrap();
    assert!(result.contains("decomposed successfully"));
    // The summary reports a file count; the named files land on disk under
    // {target_dir}/characters/{id}/.
    let base = std::path::Path::new("./test_decomposed/characters/testcharacter");
    assert!(base.join("basic_info.md").exists());
    assert!(base.join("personality.md").exists());

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

// ── Issue #28 bug 1: constant (常驻) lorebook entries ──────────────────

#[tokio::test]
async fn test_lorebook_constant_entries_auto_included() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    let card = common::create_test_card();
    let png_base64 = common::card_to_base64(&card);
    server
        .handle_import_card(serde_json::json!({"png_base64": png_base64}))
        .await
        .unwrap();

    // Update lorebook: one keyword-gated + one constant (常驻) entry.
    server
        .handle_update_lorebook(serde_json::json!({
            "character_id": "testcharacter",
            "entries": [
                {
                    "id": "gated",
                    "name": "Gated Entry",
                    "keys": ["castle"],
                    "content": "The castle stands on a hill.",
                    "enabled": true,
                    "insertion_order": 0,
                    "case_sensitive": false,
                    "constant": false
                },
                {
                    "id": "always_on",
                    "name": "Always-On Entry",
                    "keys": ["never_matches"],
                    "content": "This is always included.",
                    "enabled": true,
                    "insertion_order": 10,
                    "case_sensitive": false,
                    "constant": true
                }
            ]
        }))
        .await
        .unwrap();

    // Text that does NOT match either entry's keys. Before the fix, this
    // returned "No lorebook entries matched." After the fix, the constant
    // entry is still included.
    let result = server
        .handle_apply_lorebook(serde_json::json!({
            "character_id": "testcharacter",
            "text": "completely unrelated text"
        }))
        .await
        .unwrap();
    assert!(
        result.contains("Always-On Entry"),
        "constant entry must be included even without keyword match: {result}"
    );
    assert!(
        result.contains("This is always included."),
        "constant entry content must be present: {result}"
    );
    assert!(
        !result.contains("Gated Entry"),
        "non-constant entry must NOT be included without keyword match: {result}"
    );

    // Text that DOES match the gated entry: both should appear.
    let result = server
        .handle_apply_lorebook(serde_json::json!({
            "character_id": "testcharacter",
            "text": "We approached the castle"
        }))
        .await
        .unwrap();
    assert!(result.contains("Gated Entry"));
    assert!(result.contains("Always-On Entry"));
}

// ── Issue #28 bug 2: preset import via preset_path ─────────────────────

#[tokio::test]
async fn test_import_preset_via_path() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    // Write a preset JSON to a temp file, then import via preset_path.
    let preset_json = serde_json::json!({
        "name": "PathPreset",
        "config": {
            "system_prompt_prefix": "prefix",
            "system_prompt_suffix": "suffix",
            "temperature": 0.8,
            "top_p": 0.95,
            "top_k": 50,
            "repetition_penalty": 1.1,
            "max_tokens": 4096,
            "stop_sequences": [],
            "regex_scripts": []
        }
    })
    .to_string();

    let preset_file = ctx.data_dir.join("my_preset.json");
    std::fs::write(&preset_file, &preset_json).unwrap();

    let result = server
        .handle_import_preset(serde_json::json!({
            "preset_id": "path-preset",
            "preset_path": preset_file.to_string_lossy()
        }))
        .await
        .unwrap();
    let v: Value = serde_json::from_str(&result).unwrap();
    assert_eq!(v["source"], "preset_path");
    assert_eq!(v["bytes_read"], preset_json.len());

    // Verify it was written to the presets dir (subdirectory layout: presets/<id>/preset.json).
    let written_path = ctx
        .data_dir
        .join("presets")
        .join("path-preset")
        .join("preset.json");
    assert!(
        written_path.exists(),
        "preset file should exist at {written_path:?}"
    );

    // Importing with neither preset_json nor preset_path should fail.
    assert!(
        server
            .handle_import_preset(serde_json::json!({
                "preset_id": "neither"
            }))
            .await
            .is_err(),
        "must provide one of preset_json / preset_path"
    );

    // Importing with both should also fail.
    assert!(
        server
            .handle_import_preset(serde_json::json!({
                "preset_id": "both",
                "preset_json": "{}",
                "preset_path": preset_file.to_string_lossy()
            }))
            .await
            .is_err(),
        "must not provide both preset_json and preset_path"
    );
}

// ── Issue #28 bug 3: V3 character card PNG import ──────────────────────

#[tokio::test]
async fn test_import_v3_character_card() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    let v3_card = common::create_test_v3_card();
    let png_base64 = common::v3_card_to_base64(&v3_card);

    let result = server
        .handle_import_card(serde_json::json!({"png_base64": png_base64}))
        .await
        .unwrap();
    // Validate the JSON response contract (not free-form text).
    let import_resp: Value = serde_json::from_str(&result).expect("import_card should return JSON");
    assert_eq!(import_resp["source"], "png_base64");
    assert!(import_resp["bytes_read"].as_u64().unwrap_or_default() > 0);
    assert!(
        import_resp["message"]
            .as_str()
            .unwrap_or_default()
            .contains("Successfully imported")
    );
    assert!(
        import_resp["message"]
            .as_str()
            .unwrap_or_default()
            .contains("V3TestCharacter")
    );

    // Verify the character was imported with V3 fields.
    let char_result = server
        .handle_get_character(serde_json::json!({"character_id": "v3testcharacter"}))
        .await
        .unwrap();
    let char_data: Value = serde_json::from_str(&char_result).unwrap();
    assert_eq!(char_data["card"]["name"], "V3TestCharacter");
    assert_eq!(char_data["card"]["description"], "A V3 test character");
    assert_eq!(char_data["card"]["creator"], "V3Creator");

    // Verify the character_book lorebook was extracted (2 entries).
    let lorebook_path = ctx
        .data_dir
        .join("characters")
        .join("v3testcharacter")
        .join("world")
        .join("lorebook.json");
    assert!(
        lorebook_path.exists(),
        "lorebook should be extracted from V3 card"
    );
    let lb_json = std::fs::read_to_string(&lorebook_path).unwrap();
    let lb: Value = serde_json::from_str(&lb_json).unwrap();
    let entries = lb["entries"]
        .as_array()
        .expect("entries should be an array");
    assert_eq!(entries.len(), 2, "V3 character_book should have 2 entries");

    // Find the constant entry and verify the `constant` flag was preserved.
    let constant_entry = entries
        .iter()
        .find(|e| e["constant"] == true)
        .expect("should have a constant entry");
    assert_eq!(
        constant_entry["content"],
        "This entry is always active (constant)."
    );

    // The `comment` field should have been mapped to `name` for display.
    assert_eq!(constant_entry["name"], "Always-on entry");

    // Verify the constant entry is auto-included via apply_lorebook with
    // non-matching text.
    let apply_result = server
        .handle_apply_lorebook(serde_json::json!({
            "character_id": "v3testcharacter",
            "text": "nothing relevant"
        }))
        .await
        .unwrap();
    assert!(
        apply_result.contains("always active"),
        "constant entry from V3 character_book should be auto-included: {apply_result}"
    );
}

#[tokio::test]
async fn test_lorebook_sillytavern_native_field_names() {
    // CodeRabbit audit: verify that SillyTavern's native field names
    // (key, keysecondary, disable, order, caseSensitive) deserialize
    // correctly via serde aliases + normalize_lorebook_entry's disable
    // → enabled inversion.
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    let card = common::create_test_card();
    let png_base64 = common::card_to_base64(&card);
    server
        .handle_import_card(serde_json::json!({"png_base64": png_base64}))
        .await
        .unwrap();

    // SillyTavern native field names: key (not keys), disable (not enabled),
    // order (not insertion_order), caseSensitive (not case_sensitive),
    // keysecondary (not secondary_keys).
    let lorebook_json = serde_json::json!({
        "entries": {
            "0": {
                "comment": "Dragon",
                "key": ["dragon", "wyrm"],
                "keysecondary": ["fire"],
                "content": "Dragons rule the skies.",
                "disable": false,
                "order": 5,
                "caseSensitive": false,
                "constant": false,
                "selective": true
            }
        }
    })
    .to_string();

    let lb_file = ctx.data_dir.join("st_lorebook.json");
    std::fs::write(&lb_file, &lorebook_json).unwrap();

    let result = server
        .handle_update_lorebook(serde_json::json!({
            "character_id": "testcharacter",
            "lorebook_path": lb_file.to_string_lossy()
        }))
        .await
        .unwrap();
    let resp: Value = serde_json::from_str(&result).expect("should return JSON");
    assert_eq!(resp["entries"], 1, "should parse 1 entry: {result}");

    // Verify the ST field names were mapped: key → keys should match "dragon".
    let apply_result = server
        .handle_apply_lorebook(serde_json::json!({
            "character_id": "testcharacter",
            "text": "A dragon appeared"
        }))
        .await
        .unwrap();
    assert!(
        apply_result.contains("Dragon"),
        "ST `key` field should map to `keys` and match: {apply_result}"
    );
    assert!(
        apply_result.contains("Dragons rule the skies."),
        "content should be present: {apply_result}"
    );

    // Verify `disable: false` → `enabled: true` (entry matched, so it's enabled).
    // If disable had been true, the entry would NOT match.
    let no_match_result = server
        .handle_apply_lorebook(serde_json::json!({
            "character_id": "testcharacter",
            "text": "completely unrelated"
        }))
        .await
        .unwrap();
    assert!(
        !no_match_result.contains("Dragon"),
        "Non-matching text should not include keyword-gated entry"
    );
}
