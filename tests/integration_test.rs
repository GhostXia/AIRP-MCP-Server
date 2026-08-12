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
async fn test_import_post_idat_character_metadata() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    let mut text_card = common::create_test_card();
    text_card["name"] = Value::String("PostIdatText".to_string());
    server
        .handle_import_card(serde_json::json!({
            "png_base64": common::post_idat_text_card_to_base64(&text_card)
        }))
        .await
        .expect("post-IDAT tEXt card should import");

    let mut ztxt_card = common::create_test_card();
    ztxt_card["name"] = Value::String("PostIdatZtxt".to_string());
    server
        .handle_import_card(serde_json::json!({
            "png_base64": common::post_idat_ztxt_card_to_base64(&ztxt_card)
        }))
        .await
        .expect("post-IDAT zTXt card should import");

    let text_result = server
        .handle_get_character(serde_json::json!({"character_id": "postidattext"}))
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&text_result).unwrap()["card"]["name"],
        "PostIdatText"
    );

    let ztxt_result = server
        .handle_get_character(serde_json::json!({"character_id": "postidatztxt"}))
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&ztxt_result).unwrap()["card"]["name"],
        "PostIdatZtxt"
    );
}

#[tokio::test]
async fn test_ccv3_takes_precedence_over_chara() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    let ccv3 = common::create_test_v3_card();
    let mut chara = common::create_test_card();
    chara["name"] = Value::String("CharaFallback".to_string());

    server
        .handle_import_card(serde_json::json!({
            "png_base64": common::post_idat_dual_card_to_base64(&ccv3, &chara)
        }))
        .await
        .expect("dual card should import");

    let result = server
        .handle_get_character(serde_json::json!({"character_id": "v3testcharacter"}))
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&result).unwrap()["card"]["name"],
        "V3TestCharacter"
    );

    let fallback = server
        .handle_get_character(serde_json::json!({"character_id": "charafallback"}))
        .await;
    assert!(fallback.is_err(), "chara must not win over ccv3");
}

#[tokio::test]
async fn test_oversized_ztxt_is_rejected_before_decode() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    let err = server
        .handle_import_card(serde_json::json!({
            "png_base64": common::oversized_ztxt_card_to_base64()
        }))
        .await
        .expect_err("high-ratio zTXt must be rejected");
    let message = err.to_string();
    assert!(
        message.contains("decompressed limit"),
        "unexpected error for oversized zTXt: {message}"
    );
}

#[tokio::test]
async fn test_post_idat_metadata_with_bad_crc_is_rejected() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();
    let card = common::create_test_card();

    for png_base64 in [
        common::post_idat_text_card_with_bad_crc_to_base64(&card),
        common::post_idat_ztxt_card_with_bad_crc_to_base64(&card),
    ] {
        let err = server
            .handle_import_card(serde_json::json!({"png_base64": png_base64}))
            .await
            .expect_err("post-IDAT metadata with a bad CRC must be rejected");
        assert!(
            err.to_string().to_ascii_lowercase().contains("crc"),
            "unexpected CRC error: {err}"
        );
    }
}

#[tokio::test]
async fn test_trailing_block_after_iend_is_rejected() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();

    let err = server
        .handle_import_card(serde_json::json!({
            "png_base64": common::post_idat_card_with_trailing_block_to_base64(
                &common::create_test_card()
            )
        }))
        .await
        .expect_err("trailing data after IEND must be rejected");
    assert!(
        err.to_string().contains("Trailing data after IEND"),
        "unexpected trailing-data error: {err}"
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

#[tokio::test]
async fn test_preset_raw_bytes_and_legacy_roundtrip() {
    let ctx = common::TestContext::new().await;
    let store = airp_mcp_server::storage::PresetStore::new(&ctx.storage);
    let id = airp_mcp_server::models::PresetId::new("raw-roundtrip").unwrap();

    // Keep formatting, BOM, and unknown fields exactly as supplied. The
    // parser provides only the compact AIRP view; it must not rewrite source
    // JSON on import.
    let raw = b"\xEF\xBB\xBF{\n  \"name\": \"Raw source\",\n  \"config\": {\"temperature\": 0.8},\n  \"prompts\": [\"untouched\"],\n  \"unknown\": {\"keep\": true}\n}\n";
    store.save_raw(&id, raw).await.unwrap();
    assert_eq!(
        std::fs::read(ctx.storage.preset_json_path(id.as_ref())).unwrap(),
        raw,
        "raw preset bytes must survive import unchanged"
    );
    assert_eq!(store.get(&id).await.unwrap().name, "Raw source");

    let st_id = airp_mcp_server::models::PresetId::new("st-prompt-order").unwrap();
    let st_raw = br#"{"name":"ST source","system_prompt_prefix":"prefix","system_prompt_suffix":"suffix","prompts":[{"identifier":"system","content":"raw prompt"}],"prompt_order":[[{"identifier":"system","enabled":true}]]}"#;
    store.save_raw(&st_id, st_raw).await.unwrap();
    let st = store.get(&st_id).await.unwrap();
    assert_eq!(
        std::fs::read(ctx.storage.preset_json_path(st_id.as_ref())).unwrap(),
        st_raw,
        "prompt_order and prompts must remain in the raw source"
    );
    assert_eq!(
        st.build_system_prompt("character"),
        "character",
        "SillyTavern prompts and prompt_order must stay opaque to AIRP"
    );

    let legacy = airp_mcp_server::models::Preset {
        id: airp_mcp_server::models::PresetId::new("legacy-save").unwrap(),
        name: "Legacy Save".to_string(),
        config: airp_mcp_server::models::PresetConfig {
            system_prompt_prefix: "prefix".to_string(),
            system_prompt_suffix: "suffix".to_string(),
            temperature: 0.75,
            ..Default::default()
        },
    };
    store.save(&legacy).await.unwrap();
    let loaded = store.get(&legacy.id).await.unwrap();
    assert_eq!(loaded.id.as_ref(), legacy.id.as_ref());
    assert_eq!(loaded.name, legacy.name);
    assert_eq!(loaded.config.system_prompt_prefix, "prefix");
    assert_eq!(loaded.config.system_prompt_suffix, "suffix");
    assert!((loaded.config.temperature - 0.75).abs() < f32::EPSILON);
    assert_eq!(
        loaded.build_system_prompt("character"),
        "prefix\n\ncharacter\n\nsuffix",
        "legacy AIRP config anchors remain supported"
    );
}

#[tokio::test]
async fn test_preset_validation_and_failed_write_preserves_old_file() {
    let ctx = common::TestContext::new().await;
    let store = airp_mcp_server::storage::PresetStore::new(&ctx.storage);
    let id = airp_mcp_server::models::PresetId::new("validation").unwrap();
    let old = br#"{"name":"old","config":{"temperature":0.7}}"#;
    store.save_raw(&id, old).await.unwrap();

    for invalid in [
        br#"{"name":"missing schema"}"#.as_slice(),
        br#"{"name":"bad number","config":{"temperature":"hot"}}"#.as_slice(),
        br#"[]"#.as_slice(),
    ] {
        assert!(
            store.save_raw(&id, invalid).await.is_err(),
            "invalid preset input must be rejected"
        );
        assert_eq!(
            std::fs::read(ctx.storage.preset_json_path(id.as_ref())).unwrap(),
            old,
            "failed validation must not replace an existing preset"
        );
    }

    for invalid_id in ["", ".hidden", "trailing.", "a..b", "a/b", "a\\b", "a:b"] {
        assert!(
            airp_mcp_server::models::PresetId::new(invalid_id).is_err(),
            "invalid preset ID should be rejected: {invalid_id:?}"
        );
    }
}

#[tokio::test]
async fn test_preset_list_skips_invalid_and_accepts_flat_migration() {
    let ctx = common::TestContext::new().await;
    let presets_dir = ctx.data_dir.join("presets");
    let flat_path = presets_dir.join("flat-preset.json");
    let flat_raw = br#"{"name":"Flat preset","config":{}}"#;
    std::fs::write(&flat_path, flat_raw).unwrap();
    std::fs::write(presets_dir.join("broken.json"), b"not json").unwrap();

    let store = airp_mcp_server::storage::PresetStore::new(&ctx.storage);
    let list = store.list().await.unwrap();
    assert!(
        list.iter()
            .any(|preset| preset.id.as_ref() == "flat-preset")
    );
    assert!(!list.iter().any(|preset| preset.id.as_ref() == "broken"));

    ctx.storage.migrate_legacy_presets().await.unwrap();
    assert!(!flat_path.exists());
    assert!(presets_dir.join("flat-preset").join("preset.json").exists());
}

#[tokio::test]
async fn test_unicode_preset_id_roundtrip_without_normalization() {
    let ctx = common::TestContext::new().await;
    let store = airp_mcp_server::storage::PresetStore::new(&ctx.storage);
    let composed = airp_mcp_server::models::PresetId::new("角色.預設_1").unwrap();
    let other_unicode = airp_mcp_server::models::PresetId::new("é").unwrap();
    assert_ne!(composed.as_ref(), other_unicode.as_ref());

    let raw = br#"{"name":"Unicode preset","config":{}}"#;
    store.save_raw(&composed, raw).await.unwrap();
    assert_eq!(store.get(&composed).await.unwrap().name, "Unicode preset");
    assert!(
        store
            .list()
            .await
            .unwrap()
            .iter()
            .any(|preset| preset.id == composed)
    );
    store.delete(&composed).await.unwrap();
    assert!(store.get(&composed).await.is_err());

    for invalid in [
        "a b",
        "a/b",
        "a\\b",
        ".hidden",
        "trailing.",
        "a..b",
        "\u{0}",
    ] {
        assert!(airp_mcp_server::models::PresetId::new(invalid).is_err());
    }
}

#[tokio::test]
async fn test_preset_collision_prefers_canonical_and_delete_removes_both() {
    let ctx = common::TestContext::new().await;
    let presets_dir = ctx.data_dir.join("presets");
    let canonical_dir = presets_dir.join("collision");
    let canonical_path = canonical_dir.join("preset.json");
    let flat_path = presets_dir.join("collision.json");
    std::fs::create_dir_all(&canonical_dir).unwrap();
    std::fs::write(&canonical_path, br#"{"name":"Canonical","config":{}}"#).unwrap();
    std::fs::write(&flat_path, br#"{"name":"Legacy","config":{}}"#).unwrap();

    ctx.storage.migrate_legacy_presets().await.unwrap();
    assert_eq!(
        std::fs::read_to_string(&canonical_path).unwrap(),
        r#"{"name":"Canonical","config":{}}"#
    );
    assert!(
        flat_path.exists(),
        "migration collision must not overwrite data"
    );

    let store = airp_mcp_server::storage::PresetStore::new(&ctx.storage);
    let listed: Vec<_> = store
        .list()
        .await
        .unwrap()
        .into_iter()
        .filter(|preset| preset.id.as_ref() == "collision")
        .collect();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "Canonical");

    let id = airp_mcp_server::models::PresetId::new("collision").unwrap();
    store.delete(&id).await.unwrap();
    assert!(!canonical_dir.exists());
    assert!(!flat_path.exists());
    assert!(store.list().await.unwrap().is_empty());

    let artifact_only_dir = presets_dir.join("legacy-visible");
    std::fs::create_dir_all(&artifact_only_dir).unwrap();
    std::fs::write(artifact_only_dir.join("notes.md"), "artifact").unwrap();
    std::fs::write(
        presets_dir.join("legacy-visible.json"),
        br#"{"name":"Visible legacy","config":{}}"#,
    )
    .unwrap();
    let visible = store.list().await.unwrap();
    assert_eq!(
        visible
            .iter()
            .find(|preset| preset.id.as_ref() == "legacy-visible")
            .unwrap()
            .name,
        "Visible legacy"
    );
}

#[tokio::test]
async fn test_preset_artifact_cannot_overwrite_authoritative_source() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();
    let store = airp_mcp_server::storage::PresetStore::new(&ctx.storage);
    let id = airp_mcp_server::models::PresetId::new("artifact-guard").unwrap();
    let raw = br#"{"name":"Authoritative","config":{}}"#;
    store.save_raw(&id, raw).await.unwrap();
    let path = ctx.storage.preset_json_path(id.as_ref());
    let revision =
        airp_mcp_server::storage::preset_store::raw_revision(&std::fs::metadata(&path).unwrap());

    for artifact_path in [
        "preset.json",
        "PRESET.JSON",
        "notes/../preset.json",
        "preset.json.",
        "preset.json ",
        "preset.json::$DATA",
        ".preset.json.tmp-123-4",
        ".preset.json.tmp-123-4.",
        ".PRESET.JSON.TMP-123-4",
    ] {
        let error = server
            .handle_write_preset_artifact(serde_json::json!({
                "preset_id": id.as_ref(),
                "artifact_path": artifact_path,
                "content": "corrupt"
            }))
            .await
            .expect_err("reserved artifact path must be rejected");
        assert!(error.to_string().contains("reserved"));
        assert_eq!(std::fs::read(&path).unwrap(), raw);
        assert_eq!(
            airp_mcp_server::storage::preset_store::raw_revision(
                &std::fs::metadata(&path).unwrap()
            ),
            revision
        );
    }
}

#[tokio::test]
async fn test_preset_decompose_uses_one_raw_snapshot() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();
    let store = airp_mcp_server::storage::PresetStore::new(&ctx.storage);
    let id = airp_mcp_server::models::PresetId::new("snapshot").unwrap();
    let raw = br#"{"name":"Snapshot A","config":{"system_prompt_prefix":"from-a"},"unknown":{"version":"a"}}"#;
    store.save_raw(&id, raw).await.unwrap();

    let target = ctx.data_dir.join("snapshot-output");
    let result = server
        .handle_decompose_preset(serde_json::json!({
            "preset_id": id.as_ref(),
            "target_dir": target.to_string_lossy()
        }))
        .await
        .unwrap();
    assert!(result.contains("Snapshot A"));
    let output_dir = target.join("presets").join(id.as_ref());
    assert_eq!(
        std::fs::read(output_dir.join("preset_raw.json")).unwrap(),
        raw
    );
    assert!(
        std::fs::read_to_string(output_dir.join("system_prompt.md"))
            .unwrap()
            .contains("from-a")
    );
    let manifest: Value =
        serde_json::from_slice(&std::fs::read(output_dir.join("manifest.json")).unwrap()).unwrap();
    assert_eq!(manifest["preset_id"], id.as_ref());
    assert!(
        manifest["top_level_keys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|key| key == "unknown")
    );
}

#[tokio::test]
async fn test_png_base64_limit_is_checked_before_decode_allocation() {
    use base64::Engine;

    let ctx = common::TestContext::new().await;
    let server = ctx.server();
    let cap = 10 * 1024 * 1024;

    let at_limit = base64::engine::general_purpose::STANDARD.encode(vec![0_u8; cap]);
    let at_limit_error = server
        .handle_import_card(serde_json::json!({"png_base64": at_limit}))
        .await
        .expect_err("zero bytes are not a PNG");
    assert!(!at_limit_error.to_string().contains("PNG too large"));

    let over_limit = base64::engine::general_purpose::STANDARD.encode(vec![0_u8; cap + 1]);
    let over_limit_error = server
        .handle_import_card(serde_json::json!({"png_base64": over_limit}))
        .await
        .expect_err("decoded content over the cap must be rejected");
    assert!(over_limit_error.to_string().contains("PNG too large"));
}

#[tokio::test]
async fn test_preset_decompose_accepts_dot_id() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();
    let store = airp_mcp_server::storage::PresetStore::new(&ctx.storage);
    let id = airp_mcp_server::models::PresetId::new("lunareclipse_2.0.1").unwrap();
    store
        .save(&airp_mcp_server::models::Preset {
            id: id.clone(),
            name: "Dot ID".to_string(),
            config: Default::default(),
        })
        .await
        .unwrap();

    let target = ctx.data_dir.join("decomposed");
    server
        .handle_decompose_preset(serde_json::json!({
            "preset_id": id.as_ref(),
            "target_dir": target.to_string_lossy()
        }))
        .await
        .unwrap();
    assert!(
        target
            .join("presets")
            .join(id.as_ref())
            .join("system_prompt.md")
            .exists()
    );
}

#[tokio::test]
async fn test_preset_path_rejects_oversized_file_from_metadata() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();
    let path = ctx.data_dir.join("oversized-preset.json");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(32 * 1024 * 1024 + 1).unwrap();

    let err = server
        .handle_import_preset(serde_json::json!({
            "preset_id": "oversized",
            "preset_path": path.to_string_lossy()
        }))
        .await
        .expect_err("preset_path over the safety limit must be rejected");
    assert!(err.to_string().contains("too large"));
}

#[tokio::test]
async fn test_preset_raw_resource_reads_bounded_prefix() {
    let ctx = common::TestContext::new().await;
    let path = ctx
        .data_dir
        .join("presets")
        .join("resource-large")
        .join("preset.json");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut raw = br#"{"name":"Resource","config":{}}"#.to_vec();
    raw.extend(std::iter::repeat_n(b' ', 64 * 1024));
    std::fs::write(&path, raw).unwrap();

    let result = ctx
        .server()
        .dispatch_resource("airp://presets/resource-large/raw")
        .await
        .unwrap();
    assert!(result.starts_with("[PARTIAL:"));
    assert!(result.contains("total="));
}

#[tokio::test]
async fn test_preset_raw_pages_and_structured_reconstruction() {
    let ctx = common::TestContext::new().await;
    let server = ctx.server();
    let store = airp_mcp_server::storage::PresetStore::new(&ctx.storage);
    let id = airp_mcp_server::models::PresetId::new("paged").unwrap();
    let raw = format!(
        "\u{feff}{{\"prompts\":[{{\"identifier\":\"system\",\"content\":\"猫咪\"}}],\"prompt_order\":[[{{\"identifier\":\"system\",\"enabled\":true}}]],\"unknown\":{{\"nested\":{{\"flag\":true}},\"array\":[1,\"猫\"]}},\"padding\":\"{}\"}}",
        "x".repeat(512)
    )
    .into_bytes();
    store.save_raw(&id, &raw).await.unwrap();

    let mut offset = 0_u64;
    let mut revision: Option<String> = None;
    let mut rebuilt = Vec::new();
    loop {
        let mut args = serde_json::json!({
            "preset_id": id.as_ref(),
            "offset": offset,
            "max_bytes": 256
        });
        if let Some(value) = &revision {
            args["expected_revision"] = Value::String(value.clone());
        }
        let encoded_page = server.handle_read_preset_raw(args).await.unwrap();
        assert!(
            encoded_page.len() <= 256,
            "encoded page exceeds requested budget"
        );
        let page: Value = serde_json::from_str(&encoded_page).unwrap();
        assert!(page["content"].as_str().unwrap().len() <= 256);
        revision = Some(page["revision"].as_str().unwrap().to_string());
        rebuilt.extend_from_slice(page["content"].as_str().unwrap().as_bytes());
        if !page["has_more"].as_bool().unwrap() {
            assert_eq!(page["next_offset"], page["total_bytes"]);
            break;
        }
        offset = page["next_offset"].as_u64().unwrap();
    }
    assert_eq!(
        rebuilt, raw,
        "raw pages must reconstruct exact BOM/UTF-8 bytes"
    );

    assert!(
        server
            .handle_read_preset_raw(serde_json::json!({
                "preset_id": id.as_ref(), "offset": 1, "max_bytes": 256
            }))
            .await
            .is_err()
    );
    assert!(
        server
            .handle_read_preset_raw(serde_json::json!({
                "preset_id": id.as_ref(), "offset": u64::MAX, "max_bytes": 256
            }))
            .await
            .is_err()
    );

    let root: Value = serde_json::from_str(
        &server
            .handle_read_preset_structure(serde_json::json!({
                "preset_id": id.as_ref(), "pointer": "", "offset": 0, "limit": 20, "max_bytes": 2048
            }))
            .await
            .unwrap(),
    )
    .unwrap();
    let root_items = root["items"].as_array().unwrap();
    assert!(
        root_items
            .iter()
            .any(|item| item["key"] == "prompts" && item["type"] == "array")
    );
    assert!(root_items.iter().any(|item| item["key"] == "prompt_order"));
    assert!(root_items.iter().any(|item| item["key"] == "unknown"));

    let prompts: Value = serde_json::from_str(
        &server
            .handle_read_preset_structure(serde_json::json!({
                "preset_id": id.as_ref(), "pointer": "/prompts", "limit": 10, "max_bytes": 2048,
                "expected_revision": revision.clone().unwrap()
            }))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(prompts["items"][0]["index"], 0);
    assert_eq!(prompts["items"][0]["value"]["identifier"], "system");

    let nested: Value = serde_json::from_str(
        &server
            .handle_read_preset_structure(serde_json::json!({
                "preset_id": id.as_ref(), "pointer": "/unknown/nested", "limit": 10, "max_bytes": 1024,
                "expected_revision": revision.unwrap()
            }))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(nested["items"][0]["key"], "flag");
    assert_eq!(nested["items"][0]["type"], "boolean");
    assert!(nested.to_string().len() <= 1024);
}
