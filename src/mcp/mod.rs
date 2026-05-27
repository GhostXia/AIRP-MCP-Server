//! MCP Server implementation for AIRP

use std::sync::Arc;
use tokio::sync::RwLock;
use rmcp::{
    handler::server::ServerHandler,
    model::*,
};
use tracing::{info, debug};

use crate::error::{AirpError, Result};
use crate::storage::*;

pub mod tools;
pub mod resources;
pub mod prompts;
pub mod decompose;
pub mod preset_regex;

pub use decompose::{CharacterDecomposer, PresetDecomposer, DecomposeConfig, DecomposeResult};

#[derive(Clone)]
pub struct AirpMcpServer {
    pub storage: Arc<Storage>,
    state_subs: Arc<RwLock<StateSubscriptions>>,
}

#[derive(Default)]
pub struct StateSubscriptions {
    pub subscribers: Vec<String>,
}

impl AirpMcpServer {
    pub fn new(data_dir: &str) -> Result<Self> {
        let storage = Arc::new(Storage::new(data_dir)?);
        let state_subs = Arc::new(RwLock::new(StateSubscriptions::default()));

        Ok(Self {
            storage,
            state_subs,
        })
    }

    pub async fn init(&self) -> Result<()> {
        self.storage.init().await
    }
}

fn to_schema(value: serde_json::Value) -> Arc<serde_json::Map<String, serde_json::Value>> {
    Arc::new(value.as_object().expect("schema must be JSON object").clone())
}

fn value_from_map(map: serde_json::Map<String, serde_json::Value>) -> serde_json::Value {
    serde_json::Value::Object(map)
}

impl ServerHandler for AirpMcpServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.protocol_version = ProtocolVersion::V_2025_03_26;
        info.capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .build();
        info.server_info = {
            let mut imp = Implementation::default();
            imp.name = "airp-mcp-server".into();
            imp.title = Some("AIRP MCP Server".into());
            imp.version = env!("CARGO_PKG_VERSION").into();
            imp.description = Some(
                "AIRP MCP Server - Roleplay data management for AI characters.\n\n\
                This server provides tools and resources for managing:\n\
                - Character cards (import, list, manage)\n\
                - Chat sessions (create, append messages, retrieve context)\n\
                - World books / Lorebooks (keyword-triggered knowledge)\n\
                - AI presets (system prompts, regex filters)\n\
                - Live state tracking (HP/MP/EXP etc.)".into(),
            );
            imp.icons = None;
            imp.website_url = None;
            imp
        };
        info.instructions = Some(
            "AIRP MCP Server - Roleplay data management for AI characters.\n\n\
            This server provides tools and resources for managing:\n\
            - Character cards (import, list, manage)\n\
            - Chat sessions (create, append messages, retrieve context)\n\
            - World books / Lorebooks (keyword-triggered knowledge)\n\
            - AI presets (system prompts, regex filters)\n\
            - Live state tracking (HP/MP/EXP etc.)\n\n\
            Use list_tools to see available operations.".to_string(),
        );
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            meta: None,
            next_cursor: None,
            tools: vec![
                import_card_tool(),
                list_characters_tool(),
                get_character_tool(),
                delete_character_tool(),
                start_session_tool(),
                list_sessions_tool(),
                append_message_tool(),
                get_recent_context_tool(),
                apply_lorebook_tool(),
                update_lorebook_tool(),
                update_state_tool(),
                get_live_state_tool(),
                seal_volume_tool(),
                list_presets_tool(),
                get_preset_tool(),
                decompose_character_tool(),
                decompose_preset_tool(),
                rollback_messages_tool(),
                analyze_card_tool(),
                get_gating_status_tool(),
                import_preset_tool(),
                write_preset_artifact_tool(),
                list_preset_regex_scripts_tool(),
                remove_preset_regex_script_tool(),
                set_preset_regex_enabled_tool(),
                create_scene_tool(),
                list_scenes_tool(),
                get_scene_tool(),
                add_character_to_scene_tool(),
                merge_lorebooks_tool(),
                build_scene_system_prompt_tool(),
            ],
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> std::result::Result<CallToolResult, ErrorData> {
        let tool_name = request.name.as_ref();
        let args = value_from_map(request.arguments.unwrap_or_default());

        debug!("Calling tool: {} with args: {:?}", tool_name, args);

        let result = match tool_name {
            "import_card" => self.handle_import_card(args).await,
            "list_characters" => self.handle_list_characters().await,
            "get_character" => self.handle_get_character(args).await,
            "delete_character" => self.handle_delete_character(args).await,
            "start_session" => self.handle_start_session(args).await,
            "list_sessions" => self.handle_list_sessions(args).await,
            "append_message" => self.handle_append_message(args).await,
            "get_recent_context" => self.handle_get_recent_context(args).await,
            "apply_lorebook" => self.handle_apply_lorebook(args).await,
            "update_lorebook" => self.handle_update_lorebook(args).await,
            "update_state" => self.handle_update_state(args).await,
            "get_live_state" => self.handle_get_live_state(args).await,
            "seal_volume" => self.handle_seal_volume(args).await,
            "list_presets" => self.handle_list_presets().await,
            "get_preset" => self.handle_get_preset(args).await,
            "decompose_character" => self.handle_decompose_character(args).await,
            "decompose_preset" => self.handle_decompose_preset(args).await,
            "rollback_messages" => self.handle_rollback_messages(args).await,
            "analyze_card" => self.handle_analyze_card(args).await,
            "get_gating_status" => self.handle_gating_status(args).await,
            "import_preset" => self.handle_import_preset(args).await,
            "write_preset_artifact" => self.handle_write_preset_artifact(args).await,
            "list_preset_regex_scripts" => self.handle_list_preset_regex_scripts(args).await,
            "remove_preset_regex_script" => self.handle_remove_preset_regex_script(args).await,
            "set_preset_regex_enabled" => self.handle_set_preset_regex_enabled(args).await,
            "create_scene" => self.handle_create_scene(args).await,
            "list_scenes" => self.handle_list_scenes().await,
            "get_scene" => self.handle_get_scene(args).await,
            "add_character_to_scene" => self.handle_add_character_to_scene(args).await,
            "merge_lorebooks" => self.handle_merge_lorebooks(args).await,
            "build_scene_system_prompt" => self.handle_build_scene_system_prompt(args).await,
            _ => Err(AirpError::Mcp(format!("Unknown tool: {}", tool_name))),
        };

        match result {
            Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!("Error: {}", e))])),
        }
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> std::result::Result<ListResourcesResult, ErrorData> {
        Ok(ListResourcesResult {
            meta: None,
            next_cursor: None,
            resources: vec![Resource {
                raw: RawResource::new("airp://characters", "Characters List"),
                annotations: None,
            }, Resource {
                raw: RawResource::new("airp://presets", "Presets List"),
                annotations: None,
            }, Resource {
                raw: RawResource::new("airp://scenes", "Scenes List"),
                annotations: None,
            }],
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> std::result::Result<ListResourceTemplatesResult, ErrorData> {
        let templates = vec![
            ("airp://characters/{character_id}/card", "Character Card"),
            ("airp://characters/{character_id}/greetings", "Character Greetings"),
            ("airp://characters/{character_id}/world/lorebook", "Character Lorebook"),
            ("airp://characters/{character_id}/state/live", "Live State"),
            ("airp://characters/{character_id}/memory/current", "Current Memory"),
            ("airp://characters/{character_id}/memory/index", "Memory Index"),
            ("airp://characters/{character_id}/memory/volumes/{volume_id}", "Archived Volume"),
            ("airp://presets/{preset_id}", "AI Preset"),
            ("airp://presets/{preset_id}/raw", "Preset Raw JSON"),
            ("airp://presets/{preset_id}/artifacts", "Preset Artifacts"),
            ("airp://presets/{preset_id}/regex", "Preset Regex Scripts"),
            ("airp://scenes/{scene_id}", "Scene Configuration"),
            ("airp://gating/{character_id}/checkpoints", "Gating Checkpoints"),
        ];

        Ok(ListResourceTemplatesResult {
            meta: None,
            next_cursor: None,
            resource_templates: templates
                .into_iter()
                .map(|(uri, name)| ResourceTemplate {
                    raw: RawResourceTemplate::new(uri, name),
                    annotations: None,
                })
                .collect(),
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> std::result::Result<ReadResourceResult, ErrorData> {
        let uri = request.uri.as_str();
        debug!("Reading resource: {}", uri);

        let result = self.dispatch_resource(uri).await;

        match result {
            Ok(content) => Ok(ReadResourceResult::new(vec![ResourceContents::TextResourceContents {
                uri: request.uri,
                mime_type: Some("application/json".to_string()),
                text: content,
                meta: None,
            }])),
            Err(e) => Err(ErrorData::invalid_request(
                format!("Failed to read resource: {}", e),
                None,
            )),
        }
    }

    async fn subscribe(
        &self,
        request: SubscribeRequestParams,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> std::result::Result<(), ErrorData> {
        let uri = request.uri.as_str();

        if uri.ends_with("/state/live") {
            let _subs = self.state_subs.write().await;
            info!("Client subscribed to state updates: {}", uri);
        }

        Ok(())
    }

    async fn unsubscribe(
        &self,
        request: UnsubscribeRequestParams,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> std::result::Result<(), ErrorData> {
        let uri = request.uri.as_str();

        if uri.ends_with("/state/live") {
            let _subs = self.state_subs.write().await;
            info!("Client unsubscribed from state updates: {}", uri);
        }

        Ok(())
    }

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> std::result::Result<ListPromptsResult, ErrorData> {
        let arg = |name: &str, desc: &str, required: bool| {
            let mut a = PromptArgument::new(name);
            a.description = Some(desc.into());
            a.required = Some(required);
            a
        };

        Ok(ListPromptsResult {
            meta: None,
            next_cursor: None,
            prompts: vec![
                Prompt::new("build_system_prompt", Some("Build system prompt for a character"), Some(vec![
                    arg("character_id", "Character ID", true),
                    arg("preset_id", "Optional preset ID", false),
                ])),
                Prompt::new("filter_text", Some("Apply regex filters to text"), Some(vec![
                    arg("text", "Text to filter", true),
                    arg("preset_id", "Preset with regex scripts", true),
                ])),
                Prompt::new("state_update_instruction", Some("Instruction for AI to update state"), None),
                Prompt::new("prompt_decompose_character", Some("Guide for decomposing character card into Markdown"), Some(vec![
                    arg("character_id", "Character ID", true),
                    arg("target_dir", "Target directory", true),
                ])),
                Prompt::new("prompt_enhance_analysis", Some("Guide for enhancing decomposed character analysis"), Some(vec![
                    arg("character_id", "Character ID", true),
                    arg("target_dir", "Decomposed files directory", true),
                ])),
                Prompt::new("prompt_build_session_context", Some("Guide for building session context from decomposed files"), Some(vec![
                    arg("character_id", "Character ID", true),
                    arg("session_id", "Session ID", true),
                    arg("decomposed_dir", "Decomposed files directory", true),
                ])),
                Prompt::new("seal_volume", Some("Instruction for sealing/archiving a volume"), Some(vec![
                    arg("character_id", "Character ID", true),
                    arg("session_id", "Session ID", true),
                ])),
                Prompt::new("analyze_preset", Some("Agent-driven preset analysis workflow"), Some(vec![
                    arg("preset_id", "Imported preset ID", true),
                ])),
                Prompt::new("build_scene", Some("Multi-character scene assembly guide"), Some(vec![
                    arg("scene_id", "Scene ID", true),
                ])),
                Prompt::new("validate_card", Some("Validate character card for unknown/malformed content"), Some(vec![
                    arg("character_id", "Character ID to validate", true),
                ])),
                Prompt::new("validate_preset", Some("Validate preset for broken regex/unknown macros/anomalies"), Some(vec![
                    arg("preset_id", "Preset ID to validate", true),
                ])),
            ],
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: rmcp::service::RequestContext<rmcp::service::RoleServer>,
    ) -> std::result::Result<GetPromptResult, ErrorData> {
        let prompt_name = request.name.as_str();
        let args = value_from_map(request.arguments.unwrap_or_default());

        let messages = match prompt_name {
            "build_system_prompt" => self.build_system_prompt_messages(args).await,
            "filter_text" => self.filter_text_messages(args).await,
            "state_update_instruction" => self.state_update_instruction_messages().await,
            "prompt_decompose_character" => self.prompt_decompose_character_messages(args).await,
            "prompt_enhance_analysis" => self.prompt_enhance_analysis_messages(args).await,
            "prompt_build_session_context" => self.prompt_build_session_context_messages(args).await,
            "seal_volume" => self.seal_volume_messages(args).await,
            "analyze_preset" => self.analyze_preset_messages(args).await,
            "build_scene" => self.build_scene_messages(args).await,
            "validate_card" => self.validate_card_messages(args).await,
            "validate_preset" => self.validate_preset_messages(args).await,
            _ => Err(AirpError::Mcp(format!("Unknown prompt: {}", prompt_name))),
        };

        match messages {
            Ok(msgs) => {
                let mut result = GetPromptResult::default();
                result.description = None;
                result.messages = msgs;
                Ok(result)
            }
            Err(e) => Err(ErrorData {
                code: rmcp::model::ErrorCode::INVALID_REQUEST,
                message: format!("Failed to get prompt: {}", e).into(),
                data: None,
            }),
        }
    }
}

// Tool definitions

fn import_card_tool() -> Tool {
    Tool::new(
        "import_card",
        "Import a character card from PNG data",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "png_base64": {
                    "type": "string",
                    "description": "Base64-encoded PNG character card"
                }
            },
            "required": ["png_base64"]
        })),
    )
}

fn list_characters_tool() -> Tool {
    Tool::new(
        "list_characters",
        "List all imported characters",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {}
        })),
    )
}

fn get_character_tool() -> Tool {
    Tool::new(
        "get_character",
        "Get character details by ID",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_id": {
                    "type": "string",
                    "description": "Character ID"
                }
            },
            "required": ["character_id"]
        })),
    )
}

fn delete_character_tool() -> Tool {
    Tool::new(
        "delete_character",
        "Delete a character and all associated data",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_id": {
                    "type": "string",
                    "description": "Character ID to delete"
                }
            },
            "required": ["character_id"]
        })),
    )
}

fn start_session_tool() -> Tool {
    Tool::new(
        "start_session",
        "Create a new chat session for a character with optional preset integration",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_id": {
                    "type": "string",
                    "description": "Character ID"
                },
                "session_id": {
                    "type": "string",
                    "description": "Optional session ID (auto-generated if not provided)"
                },
                "preset_id": {
                    "type": "string",
                    "description": "Optional preset ID for session configuration"
                }
            },
            "required": ["character_id"]
        })),
    )
}

fn list_sessions_tool() -> Tool {
    Tool::new(
        "list_sessions",
        "List all sessions for a character",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_id": {
                    "type": "string",
                    "description": "Character ID"
                }
            },
            "required": ["character_id"]
        })),
    )
}

fn append_message_tool() -> Tool {
    Tool::new(
        "append_message",
        "Append a message to a session",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_id": {
                    "type": "string",
                    "description": "Character ID"
                },
                "session_id": {
                    "type": "string",
                    "description": "Session ID"
                },
                "role": {
                    "type": "string",
                    "enum": ["user", "assistant", "system"],
                    "description": "Message role"
                },
                "content": {
                    "type": "string",
                    "description": "Message content"
                }
            },
            "required": ["character_id", "session_id", "role", "content"]
        })),
    )
}

fn get_recent_context_tool() -> Tool {
    Tool::new(
        "get_recent_context",
        "Get recent messages from a session",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_id": {
                    "type": "string",
                    "description": "Character ID"
                },
                "session_id": {
                    "type": "string",
                    "description": "Session ID"
                },
                "n": {
                    "type": "integer",
                    "description": "Number of recent messages (default: 10)",
                    "default": 10
                }
            },
            "required": ["character_id", "session_id"]
        })),
    )
}

// ── M_PR Preset tool definitions ──────────────────────────────────────

fn import_preset_tool() -> Tool {
    Tool::new(
        "import_preset",
        "Import a SillyTavern preset JSON. Writes to presets/{preset_id}/preset.json.",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "preset_id": {
                    "type": "string",
                    "description": "Preset ID (folder name)"
                },
                "preset_json": {
                    "type": "string",
                    "description": "Full SillyTavern preset JSON content"
                }
            },
            "required": ["preset_id", "preset_json"]
        })),
    )
}

fn write_preset_artifact_tool() -> Tool {
    Tool::new(
        "write_preset_artifact",
        "Write an Agent-generated artifact file into the preset directory",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "preset_id": {
                    "type": "string",
                    "description": "Preset ID"
                },
                "artifact_path": {
                    "type": "string",
                    "description": "Relative path within presets/{id}/, e.g. regex/display_layer.json"
                },
                "content": {
                    "type": "string",
                    "description": "File content (UTF-8)"
                }
            },
            "required": ["preset_id", "artifact_path", "content"]
        })),
    )
}

fn list_preset_regex_scripts_tool() -> Tool {
    Tool::new(
        "list_preset_regex_scripts",
        "List all regex scripts for a preset with full metadata",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "preset_id": {
                    "type": "string",
                    "description": "Preset ID"
                }
            },
            "required": ["preset_id"]
        })),
    )
}

fn remove_preset_regex_script_tool() -> Tool {
    Tool::new(
        "remove_preset_regex_script",
        "Delete a regex script file from preset",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "preset_id": {
                    "type": "string",
                    "description": "Preset ID"
                },
                "filename": {
                    "type": "string",
                    "description": "Script filename, e.g. hide_thoughts.json"
                }
            },
            "required": ["preset_id", "filename"]
        })),
    )
}

fn set_preset_regex_enabled_tool() -> Tool {
    Tool::new(
        "set_preset_regex_enabled",
        "Enable or disable a regex script in the preset",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "preset_id": {
                    "type": "string",
                    "description": "Preset ID"
                },
                "filename": {
                    "type": "string",
                    "description": "Script filename, e.g. hide_thoughts.json"
                },
                "enabled": {
                    "type": "boolean",
                    "description": "true = enable, false = disable"
                }
            },
            "required": ["preset_id", "filename", "enabled"]
        })),
    )
}

// ── M_MS Scene tool definitions ─────────────────────────────────────

fn create_scene_tool() -> Tool {
    Tool::new(
        "create_scene",
        "Create a multi-character scene for group roleplay",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "scene_id": { "type": "string", "description": "Unique scene ID" },
                "description": { "type": "string", "description": "Scene setting description" },
                "characters": {
                    "type": "array",
                    "description": "Characters in the scene",
                    "items": {
                        "type": "object",
                        "properties": {
                            "character_id": { "type": "string" },
                            "role": { "type": "string", "enum": ["primary", "npc"] },
                            "intro": { "type": "string", "description": "Character intro for this scene" }
                        }
                    }
                },
                "narrator_style": { "type": "string", "description": "e.g. third_person_limited" },
                "lorebook_merge": { "type": "string", "enum": ["union", "primary_only"] },
                "format_hint": { "type": "string", "description": "Dialogue formatting rule, e.g. 'Name: dialogue'" }
            },
            "required": ["scene_id", "characters"]
        })),
    )
}

fn list_scenes_tool() -> Tool {
    Tool::new("list_scenes", "List all created scenes", to_schema(serde_json::json!({"type": "object", "properties": {}})))
}

fn get_scene_tool() -> Tool {
    Tool::new(
        "get_scene",
        "Get scene configuration by ID",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": { "scene_id": { "type": "string" } },
            "required": ["scene_id"]
        })),
    )
}

fn add_character_to_scene_tool() -> Tool {
    Tool::new(
        "add_character_to_scene",
        "Add a character to an existing scene",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "scene_id": { "type": "string" },
                "character_id": { "type": "string" },
                "role": { "type": "string", "enum": ["primary", "npc"] },
                "intro": { "type": "string" }
            },
            "required": ["scene_id", "character_id"]
        })),
    )
}

fn merge_lorebooks_tool() -> Tool {
    Tool::new(
        "merge_lorebooks",
        "Merge lorebooks from multiple characters with dedup and priority sort (pure algorithm, no AI)",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_ids": {
                    "type": "array",
                    "description": "List of character IDs to merge lorebooks from",
                    "items": { "type": "string" }
                },
                "strategy": {
                    "type": "string",
                    "description": "union (default) — dedup all; primary_only — use first character only",
                    "enum": ["union", "primary_only"],
                    "default": "union"
                }
            },
            "required": ["character_ids"]
        })),
    )
}

fn build_scene_system_prompt_tool() -> Tool {
    Tool::new(
        "build_scene_system_prompt",
        "Auto-assemble a multi-character system prompt from scene config (pure template assembly, no AI)",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "scene_id": { "type": "string", "description": "Scene ID" },
                "user_name": { "type": "string", "description": "Name for the user/player character", "default": "User" },
                "preset_id": { "type": "string", "description": "Optional preset ID for style injection" }
            },
            "required": ["scene_id"]
        })),
    )
}

fn analyze_card_tool() -> Tool {
    Tool::new(
        "analyze_card",
        "Perform tiered analysis on a character card (Tier 0-3)",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_id": {
                    "type": "string",
                    "description": "Character ID to analyze"
                },
                "tier": {
                    "type": "integer",
                    "description": "Analysis depth: 0=basic, 1=greetings, 2=lorebook, 3=deep (default: 0)",
                    "default": 0
                }
            },
            "required": ["character_id"]
        })),
    )
}

fn get_gating_status_tool() -> Tool {
    Tool::new(
        "get_gating_status",
        "Get gating/checkpoint status for a character",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_id": {
                    "type": "string",
                    "description": "Character ID"
                }
            },
            "required": ["character_id"]
        })),
    )
}

fn apply_lorebook_tool() -> Tool {
    Tool::new(
        "apply_lorebook",
        "Apply lorebook entries matching the input text",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_id": {
                    "type": "string",
                    "description": "Character ID"
                },
                "text": {
                    "type": "string",
                    "description": "Text to scan for lorebook keywords"
                }
            },
            "required": ["character_id", "text"]
        })),
    )
}

fn update_lorebook_tool() -> Tool {
    Tool::new(
        "update_lorebook",
        "Update lorebook entries for a character",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_id": {
                    "type": "string",
                    "description": "Character ID"
                },
                "entries": {
                    "type": "array",
                    "description": "Lorebook entries",
                    "items": {
                        "type": "object"
                    }
                }
            },
            "required": ["character_id", "entries"]
        })),
    )
}

fn update_state_tool() -> Tool {
    Tool::new(
        "update_state",
        "Update live state for a character",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_id": {
                    "type": "string",
                    "description": "Character ID"
                },
                "state_delta": {
                    "type": "object",
                    "description": "State changes to apply"
                }
            },
            "required": ["character_id", "state_delta"]
        })),
    )
}

fn get_live_state_tool() -> Tool {
    Tool::new(
        "get_live_state",
        "Get current live state for a character",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_id": {
                    "type": "string",
                    "description": "Character ID"
                }
            },
            "required": ["character_id"]
        })),
    )
}

fn seal_volume_tool() -> Tool {
    Tool::new(
        "seal_volume",
        "Seal/archive current session volume",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_id": {
                    "type": "string",
                    "description": "Character ID"
                },
                "session_id": {
                    "type": "string",
                    "description": "Session ID"
                }
            },
            "required": ["character_id", "session_id"]
        })),
    )
}

fn list_presets_tool() -> Tool {
    Tool::new(
        "list_presets",
        "List all AI presets",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {}
        })),
    )
}

fn get_preset_tool() -> Tool {
    Tool::new(
        "get_preset",
        "Get preset details by ID",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "preset_id": {
                    "type": "string",
                    "description": "Preset ID"
                }
            },
            "required": ["preset_id"]
        })),
    )
}

fn decompose_character_tool() -> Tool {
    Tool::new(
        "decompose_character",
        "Decompose character card into Agent-friendly Markdown documents",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_id": {
                    "type": "string",
                    "description": "Character ID to decompose"
                },
                "target_dir": {
                    "type": "string",
                    "description": "Target directory for decomposed files (default: ./decomposed)"
                },
                "enhance": {
                    "type": "boolean",
                    "description": "Whether to perform enhanced analysis (default: true)"
                }
            },
            "required": ["character_id"]
        })),
    )
}

fn decompose_preset_tool() -> Tool {
    Tool::new(
        "decompose_preset",
        "Decompose preset into Agent-friendly Markdown documents",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "preset_id": {
                    "type": "string",
                    "description": "Preset ID to decompose"
                },
                "target_dir": {
                    "type": "string",
                    "description": "Target directory for decomposed files (default: ./decomposed)"
                }
            },
            "required": ["preset_id"]
        })),
    )
}

fn rollback_messages_tool() -> Tool {
    Tool::new(
        "rollback_messages",
        "Rollback (delete) the last N messages from a session",
        to_schema(serde_json::json!({
            "type": "object",
            "properties": {
                "character_id": {
                    "type": "string",
                    "description": "Character ID"
                },
                "session_id": {
                    "type": "string",
                    "description": "Session ID"
                },
                "n": {
                    "type": "integer",
                    "description": "Number of messages to rollback (default: 1)",
                    "default": 1
                }
            },
            "required": ["character_id", "session_id"]
        })),
    )
}
