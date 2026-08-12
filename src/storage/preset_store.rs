//! Preset storage operations.
//!
//! `preset.json` is deliberately kept as the raw source document.  The
//! `Preset` value returned by this module is a small AIRP view used by the
//! runtime; it is never written back when a SillyTavern document is read.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt, SeekFrom};
use tracing::{info, warn};

use super::Storage;
use crate::error::{AirpError, Result};
use crate::models::preset::RegexScript;
use crate::models::*;

const DEFAULT_TEMPERATURE: f32 = 0.7;
const DEFAULT_TOP_P: f32 = 0.9;
const DEFAULT_TOP_K: i32 = 40;
const DEFAULT_REPETITION_PENALTY: f32 = 1.0;
const DEFAULT_MAX_TOKENS: i32 = 2048;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Preset storage operations.
pub struct PresetStore<'a> {
    storage: &'a Storage,
}

impl<'a> PresetStore<'a> {
    pub fn new(storage: &'a Storage) -> Self {
        Self { storage }
    }

    /// Create or update a canonical AIRP preset.
    ///
    /// This API is retained for callers that construct an AIRP `Preset`
    /// directly.  Imported SillyTavern documents use `save_raw` below so
    /// their complete source JSON remains authoritative.
    pub async fn save(&self, preset: &Preset) -> Result<()> {
        let id = PresetId::new(preset.id.as_ref().to_string())?;
        let preset_path = self.preset_path(&id);
        let json = serde_json::to_vec_pretty(preset)?;
        atomic_write(&preset_path, &json).await?;

        info!("Saved preset: {} ({})", preset.name, preset.id.as_ref());
        Ok(())
    }

    /// Validate and atomically save raw preset JSON without normalizing it.
    pub async fn save_raw(&self, id: &PresetId, raw: &[u8]) -> Result<()> {
        // Parse directly from the caller's byte slice.  In particular, do
        // not create a `String`/`Vec` copy of a path import before building
        // the serde_json DOM: the raw source bytes must remain authoritative,
        // while a second full-sized copy would needlessly multiply the peak
        // memory used by large SillyTavern presets.
        let json_bytes = raw.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(raw);
        if std::str::from_utf8(json_bytes).is_err() {
            return Err(AirpError::Validation(
                "preset content is not valid UTF-8".into(),
            ));
        }
        let value: serde_json::Value = serde_json::from_slice(json_bytes)
            .map_err(|e| AirpError::Validation(format!("preset is not valid JSON: {}", e)))?;
        parse_raw_preset(id, &value)?;
        // Release the validation DOM before the atomic writer starts.  The
        // caller's raw slice remains the sole source buffer to retain.
        drop(value);

        let preset_path = self.preset_path(id);
        atomic_write(&preset_path, raw).await?;
        Ok(())
    }

    /// Get the AIRP view of a preset while leaving the source JSON untouched.
    pub async fn get(&self, id: &PresetId) -> Result<Preset> {
        let preset_path = self.preset_path(id);

        if !preset_path.exists() {
            return Err(AirpError::PresetNotFound(id.as_ref().to_string()));
        }

        let json = fs::read(&preset_path).await?;
        let json_bytes = json.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&json);
        let raw: serde_json::Value = serde_json::from_slice(json_bytes)?;
        parse_raw_preset(id, &raw)
    }

    /// Read one UTF-8-aligned page from the authoritative raw source.
    ///
    /// The page reader deliberately seeks to `offset` and never loads the
    /// complete source document.  The BOM, when present, is part of the byte
    /// stream and is therefore returned on the first page rather than
    /// stripped.  `expected_revision` protects a multi-page read from an
    /// atomic replacement between calls.
    pub async fn read_raw_page(
        &self,
        id: &PresetId,
        offset: u64,
        max_bytes: usize,
        expected_revision: Option<&str>,
    ) -> Result<PresetRawPage> {
        let path = self.preset_path(id);
        let metadata = fs::metadata(&path)
            .await
            .map_err(|_| AirpError::PresetNotFound(id.as_ref().to_string()))?;
        let total_bytes = metadata.len();
        let revision = raw_revision(&metadata);
        if let Some(expected) = expected_revision {
            if expected != revision {
                return Err(AirpError::Validation(format!(
                    "preset revision changed: expected {}, current {}",
                    expected, revision
                )));
            }
        }
        if offset > total_bytes {
            return Err(AirpError::Validation(format!(
                "offset {} exceeds preset size {}",
                offset, total_bytes
            )));
        }
        if max_bytes == 0 {
            return Err(AirpError::Validation(
                "max_bytes must be greater than zero".into(),
            ));
        }

        let mut file = fs::File::open(&path).await?;
        validate_utf8_boundary(&mut file, offset, total_bytes).await?;
        file.seek(SeekFrom::Start(offset)).await?;
        let mut bytes = Vec::with_capacity(max_bytes.min(64 * 1024));
        file.take(max_bytes as u64).read_to_end(&mut bytes).await?;

        let available = total_bytes.saturating_sub(offset);
        let has_more_raw = (bytes.len() as u64) < available;
        let returned_bytes = match std::str::from_utf8(&bytes) {
            Ok(_) => bytes.len(),
            Err(err) if err.error_len().is_none() && has_more_raw => err.valid_up_to(),
            Err(err) if err.error_len().is_none() => {
                return Err(AirpError::Validation(format!(
                    "preset raw content ends with an incomplete UTF-8 sequence at byte {}",
                    offset + err.valid_up_to() as u64
                )));
            }
            Err(_) => {
                return Err(AirpError::Validation(
                    "preset raw content is not valid UTF-8".into(),
                ));
            }
        };
        if returned_bytes == 0 && has_more_raw {
            return Err(AirpError::Validation(
                "max_bytes is too small to return the next UTF-8 code point; use at least 4 bytes"
                    .into(),
            ));
        }
        bytes.truncate(returned_bytes);

        // Detect a replacement that happened while the bounded read was in
        // flight.  This is cheap metadata-only validation; no second full
        // source read is needed.
        let after = fs::metadata(&path).await?;
        let after_revision = raw_revision(&after);
        if after_revision != revision {
            return Err(AirpError::Validation(format!(
                "preset revision changed during read: {} -> {}",
                revision, after_revision
            )));
        }

        let next_offset = offset + returned_bytes as u64;
        let has_more = next_offset < total_bytes;
        let content = String::from_utf8(bytes)
            .map_err(|_| AirpError::Validation("preset raw page is not valid UTF-8".into()))?;
        Ok(PresetRawPage {
            revision,
            total_bytes,
            offset,
            returned_bytes,
            next_offset,
            has_more,
            content,
        })
    }

    /// List presets with a compact AIRP view.  A corrupt or unsupported item
    /// is skipped and logged instead of aborting the entire listing.
    pub async fn list(&self) -> Result<Vec<Preset>> {
        let presets_dir = self.storage.presets_dir();

        if !presets_dir.exists() {
            return Ok(vec![]);
        }

        let mut entries = fs::read_dir(&presets_dir).await?;
        let mut candidates: BTreeMap<String, (bool, PathBuf)> = BTreeMap::new();

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let (id_text, preset_path, canonical) = if path.is_dir() {
                let Some(id_text) = path.file_name().and_then(|n| n.to_str()) else {
                    warn!(path = ?path, "Skipping preset with non-UTF8 directory name");
                    continue;
                };
                let preset_path = path.join("preset.json");
                if !preset_path.is_file() {
                    continue;
                }
                (id_text.to_string(), preset_path, true)
            } else if path.is_file() && path.extension().is_some_and(|e| e == "json") {
                // Flat files are accepted during migration/upgrade windows.
                let Some(id_text) = path.file_stem().and_then(|n| n.to_str()) else {
                    warn!(path = ?path, "Skipping preset with non-UTF8 file name");
                    continue;
                };
                (id_text.to_string(), path.clone(), false)
            } else {
                continue;
            };

            let id = match PresetId::new(id_text.clone()) {
                Ok(id) => id,
                Err(err) => {
                    warn!(path = ?path, err = %err, "Skipping preset with invalid ID");
                    continue;
                }
            };
            match candidates.entry(id.as_ref().to_string()) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert((canonical, preset_path));
                }
                std::collections::btree_map::Entry::Occupied(mut entry) if canonical => {
                    entry.insert((true, preset_path));
                }
                std::collections::btree_map::Entry::Occupied(_) => {}
            }
        }

        let mut presets = Vec::with_capacity(candidates.len());
        for (id_text, (_, preset_path)) in candidates {
            let id = PresetId::new(id_text)?;
            let json = match fs::read(&preset_path).await {
                Ok(json) => json,
                Err(err) => {
                    warn!(path = ?preset_path, err = %err, "Skipping unreadable preset");
                    continue;
                }
            };
            let json_bytes = json.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&json);
            let raw: serde_json::Value = match serde_json::from_slice(json_bytes) {
                Ok(raw) => raw,
                Err(err) => {
                    warn!(path = ?preset_path, err = %err, "Skipping preset with invalid JSON");
                    continue;
                }
            };
            match parse_raw_preset(&id, &raw) {
                Ok(preset) => presets.push(preset),
                Err(err) => warn!(path = ?preset_path, err = %err, "Skipping unsupported preset"),
            }
        }

        Ok(presets)
    }

    /// Delete preset (including artifacts stored beside preset.json).
    pub async fn delete(&self, id: &PresetId) -> Result<()> {
        let preset_dir = self.storage.presets_dir().join(id.as_ref());
        let flat_path = self
            .storage
            .presets_dir()
            .join(format!("{}.json", id.as_ref()));

        let mut found = false;
        if preset_dir.exists() {
            fs::remove_dir_all(&preset_dir).await?;
            found = true;
        }
        if flat_path.exists() {
            fs::remove_file(&flat_path).await?;
            found = true;
        }
        if !found {
            return Err(AirpError::PresetNotFound(id.as_ref().to_string()));
        }
        info!("Deleted preset: {}", id.as_ref());
        Ok(())
    }

    fn preset_path(&self, id: &PresetId) -> PathBuf {
        self.storage
            .presets_dir()
            .join(id.as_ref())
            .join("preset.json")
    }
}

/// Bounded raw source page returned to an Agent.  The fields intentionally
/// describe byte offsets (rather than character offsets) so concatenating all
/// pages reconstructs the exact source bytes, including a leading BOM.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PresetRawPage {
    pub revision: String,
    pub total_bytes: u64,
    pub offset: u64,
    pub returned_bytes: usize,
    pub next_offset: u64,
    pub has_more: bool,
    pub content: String,
}

/// A metadata revision is stable across page calls and changes on the normal
/// atomic-replace path used by `save_raw`.  It is intentionally computed from
/// metadata only so asking for a page never reads the whole source document.
pub fn raw_revision(metadata: &std::fs::Metadata) -> String {
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| format!("{}:{}", duration.as_secs(), duration.subsec_nanos()))
        .unwrap_or_else(|| "unknown".to_string());
    let created = metadata
        .created()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| format!("{}:{}", duration.as_secs(), duration.subsec_nanos()))
        .unwrap_or_else(|| "unknown".to_string());
    #[cfg(unix)]
    let identity = {
        use std::os::unix::fs::MetadataExt;
        format!("{}:{}", metadata.dev(), metadata.ino())
    };
    #[cfg(not(unix))]
    let identity = created.clone();
    format!("{}:{}:{}:{}", metadata.len(), modified, created, identity)
}

async fn validate_utf8_boundary(file: &mut fs::File, offset: u64, total_bytes: u64) -> Result<()> {
    if offset == 0 {
        return Ok(());
    }
    if offset < total_bytes {
        file.seek(SeekFrom::Start(offset)).await?;
        let mut next = [0_u8; 1];
        file.read_exact(&mut next).await?;
        if next[0] & 0b1100_0000 == 0b1000_0000 {
            return Err(AirpError::Validation(format!(
                "offset {} is not a UTF-8 boundary",
                offset
            )));
        }
        return Ok(());
    }

    // EOF is a valid boundary only when the final code point is complete.
    let start = total_bytes.saturating_sub(4);
    file.seek(SeekFrom::Start(start)).await?;
    let mut tail = Vec::new();
    file.take(total_bytes - start)
        .read_to_end(&mut tail)
        .await?;
    if std::str::from_utf8(&tail).is_err() {
        return Err(AirpError::Validation(format!(
            "offset {} is not a UTF-8 boundary",
            offset
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PresetSchema {
    LegacyAirp,
    SillyTavern,
}

/// Parse a raw preset document into the small AIRP runtime view.  This is the
/// single schema discriminator used by import, get and list.
pub fn parse_raw_preset(id: &PresetId, raw: &serde_json::Value) -> Result<Preset> {
    let object = raw
        .as_object()
        .ok_or_else(|| AirpError::Validation("preset JSON must be a top-level object".into()))?;

    // If present, these identity fields are structural and must not silently
    // change type.  The path ID remains authoritative for the returned view.
    if let Some(raw_id) = object.get("id") {
        if !raw_id.is_string() {
            return Err(AirpError::Validation("preset id must be a string".into()));
        }
    }
    let name = match object.get("name") {
        Some(value) => value
            .as_str()
            .ok_or_else(|| AirpError::Validation("preset name must be a string".into()))?
            .to_string(),
        None => id.as_ref().to_string(),
    };

    let schema = classify_schema(object)?;
    let config = match schema {
        PresetSchema::LegacyAirp => {
            let config = object
                .get("config")
                .and_then(|v| v.as_object())
                .ok_or_else(|| AirpError::Validation("preset config must be an object".into()))?;
            parse_config(config, false)?
        }
        PresetSchema::SillyTavern => parse_config(object, true)?,
    };

    Ok(Preset {
        id: id.clone(),
        name,
        config,
    })
}

fn classify_schema(object: &serde_json::Map<String, serde_json::Value>) -> Result<PresetSchema> {
    if let Some(config) = object.get("config") {
        if !config.is_object() {
            return Err(AirpError::Validation(
                "preset config must be an object".into(),
            ));
        }
        return Ok(PresetSchema::LegacyAirp);
    }

    // These fields cover current SillyTavern exports and the official fixture
    // variants.  A bare `{ "name": ... }` is intentionally not accepted:
    // import must establish which schema it is preserving.
    const ST_FIELDS: &[&str] = &[
        "prompts",
        "prompt_order",
        "temperature",
        "top_p",
        "top_k",
        "repetition_penalty",
        "openai_max_tokens",
        "max_tokens",
        "stop_sequences",
        "extensions",
        "new_chat_prompt",
        "new_example_chat",
        "assistant_prefill",
        "system_prompt_prefix",
        "system_prompt_suffix",
        "model",
        "api_url",
        "chat_completion_source",
    ];

    if ST_FIELDS.iter().any(|field| object.contains_key(*field)) {
        validate_st_shape(object)?;
        Ok(PresetSchema::SillyTavern)
    } else {
        Err(AirpError::Validation(
            "unrecognized preset schema (expected AIRP config or SillyTavern fields)".into(),
        ))
    }
}

fn validate_st_shape(object: &serde_json::Map<String, serde_json::Value>) -> Result<()> {
    for field in ["prompts", "prompt_order"] {
        if let Some(value) = object.get(field) {
            if !value.is_array() {
                return Err(AirpError::Validation(format!(
                    "preset {} must be an array",
                    field
                )));
            }
        }
    }
    Ok(())
}

fn parse_config(
    object: &serde_json::Map<String, serde_json::Value>,
    sillytavern: bool,
) -> Result<PresetConfig> {
    // SillyTavern's `prompts` / `prompt_order` are opaque source data.  AIRP
    // does not emulate ST Prompt Manager and must never turn those fields (or
    // ST's similarly named knobs) into AIRP prefix/suffix injections.  Only
    // the legacy nested AIRP `config` schema owns these runtime anchors.
    let (prefix, suffix) = if sillytavern {
        (String::new(), String::new())
    } else {
        (
            optional_string(object, "system_prompt_prefix", String::new())?,
            optional_string(object, "system_prompt_suffix", String::new())?,
        )
    };

    let temperature = optional_f32(object, "temperature", DEFAULT_TEMPERATURE)?;
    let top_p = optional_f32(object, "top_p", DEFAULT_TOP_P)?;
    let top_k = optional_i32(object, "top_k", DEFAULT_TOP_K)?;
    let repetition_penalty =
        optional_f32(object, "repetition_penalty", DEFAULT_REPETITION_PENALTY)?;
    let max_tokens = if sillytavern {
        if object.contains_key("openai_max_tokens") {
            optional_i32(object, "openai_max_tokens", DEFAULT_MAX_TOKENS)?
        } else {
            optional_i32(object, "max_tokens", DEFAULT_MAX_TOKENS)?
        }
    } else {
        optional_i32(object, "max_tokens", DEFAULT_MAX_TOKENS)?
    };
    let stop_sequences = optional_string_array(object, "stop_sequences")?;
    let regex_scripts = parse_regex_scripts(object, sillytavern)?;

    Ok(PresetConfig {
        system_prompt_prefix: prefix,
        system_prompt_suffix: suffix,
        temperature,
        top_p,
        top_k,
        repetition_penalty,
        max_tokens,
        stop_sequences,
        regex_scripts,
    })
}

fn optional_string(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    default: String,
) -> Result<String> {
    match object.get(field) {
        None => Ok(default),
        Some(value) => value
            .as_str()
            .map(ToOwned::to_owned)
            .ok_or_else(|| AirpError::Validation(format!("preset {} must be a string", field))),
    }
}

fn optional_f32(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    default: f32,
) -> Result<f32> {
    let Some(value) = object.get(field) else {
        return Ok(default);
    };
    let number = value
        .as_f64()
        .ok_or_else(|| AirpError::Validation(format!("preset {} must be numeric", field)))?;
    if !number.is_finite() {
        return Err(AirpError::Validation(format!(
            "preset {} must be finite",
            field
        )));
    }
    let converted = number as f32;
    if !converted.is_finite() {
        return Err(AirpError::Validation(format!(
            "preset {} is outside f32 range",
            field
        )));
    }
    Ok(converted)
}

fn optional_i32(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    default: i32,
) -> Result<i32> {
    let Some(value) = object.get(field) else {
        return Ok(default);
    };
    let number = value
        .as_i64()
        .ok_or_else(|| AirpError::Validation(format!("preset {} must be an integer", field)))?;
    i32::try_from(number)
        .map_err(|_| AirpError::Validation(format!("preset {} is outside i32 range", field)))
}

fn optional_string_array(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<Vec<String>> {
    let Some(value) = object.get(field) else {
        return Ok(Vec::new());
    };
    let array = value
        .as_array()
        .ok_or_else(|| AirpError::Validation(format!("preset {} must be an array", field)))?;
    array
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            item.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                AirpError::Validation(format!("preset {}[{}] must be a string", field, idx))
            })
        })
        .collect()
}

/// Extract regex scripts from either a canonical AIRP config or SillyTavern
/// extension bindings.  Malformed individual entries are skipped with a
/// warning so one bad optional script does not hide an otherwise usable
/// preset; malformed container types still fail schema validation.
fn parse_regex_scripts(
    object: &serde_json::Map<String, serde_json::Value>,
    sillytavern: bool,
) -> Result<Vec<RegexScript>> {
    let regexes = if sillytavern {
        let Some(extensions) = object.get("extensions") else {
            return Ok(Vec::new());
        };
        let extensions = extensions
            .as_object()
            .ok_or_else(|| AirpError::Validation("preset extensions must be an object".into()))?;
        extensions
            .get("SPreset")
            .and_then(|v| v.as_object())
            .and_then(|v| v.get("RegexBinding"))
            .or_else(|| extensions.get("RegexBinding"))
            .and_then(|v| v.as_object())
            .and_then(|v| v.get("regexes"))
    } else {
        object.get("regex_scripts")
    };

    let Some(regexes) = regexes else {
        return Ok(Vec::new());
    };
    let array = regexes
        .as_array()
        .ok_or_else(|| AirpError::Validation("preset regexes must be an array".into()))?;

    let mut scripts = Vec::with_capacity(array.len().min(64));
    for (idx, item) in array.iter().enumerate() {
        let Some(item) = item.as_object() else {
            warn!(index = idx, "Skipping malformed preset regex entry");
            continue;
        };
        let id = item.get("id").and_then(|v| v.as_str());
        let name = item
            .get(if sillytavern { "scriptName" } else { "name" })
            .and_then(|v| v.as_str());
        let find = item
            .get(if sillytavern { "findRegex" } else { "find" })
            .and_then(|v| v.as_str());
        let replace = item
            .get(if sillytavern {
                "replaceString"
            } else {
                "replace"
            })
            .and_then(|v| v.as_str());

        let (Some(id), Some(name), Some(find), Some(replace)) = (id, name, find, replace) else {
            warn!(
                index = idx,
                "Skipping preset regex entry with invalid string fields"
            );
            continue;
        };
        let enabled = if sillytavern {
            match item.get("disabled").and_then(|v| v.as_bool()) {
                Some(disabled) => !disabled,
                None => {
                    warn!(
                        index = idx,
                        "Skipping preset regex entry with invalid disabled field"
                    );
                    continue;
                }
            }
        } else {
            item.get("enabled")
                .and_then(|v| v.as_bool())
                .unwrap_or(true)
        };

        scripts.push(RegexScript {
            id: id.to_string(),
            name: name.to_string(),
            find: find.to_string(),
            replace: replace.to_string(),
            enabled,
        });
    }
    Ok(scripts)
}

async fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AirpError::Storage("preset path has no parent".into()))?;
    fs::create_dir_all(parent).await?;

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| AirpError::Storage("preset path has invalid filename".into()))?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let mut temp_path = None;
    for _ in 0..16 {
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = parent.join(format!(".{file_name}.tmp-{stamp}-{counter}"));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
            .await
        {
            Ok(mut file) => {
                let result = async {
                    file.write_all(bytes).await?;
                    file.flush().await?;
                    file.sync_all().await?;
                    Ok::<(), io::Error>(())
                }
                .await;
                drop(file);
                if let Err(err) = result {
                    let _ = fs::remove_file(&candidate).await;
                    return Err(err.into());
                }
                temp_path = Some(candidate);
                break;
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err.into()),
        }
    }

    let temp_path = temp_path.ok_or_else(|| {
        AirpError::Storage("could not allocate unique preset temporary file".into())
    })?;

    if let Err(err) = replace_file(&temp_path, path).await {
        let _ = fs::remove_file(&temp_path).await;
        return Err(err.into());
    }

    // A same-directory rename gives readers an atomic old-or-new view.  On
    // Unix, syncing the containing directory also persists the rename itself
    // across a crash.  Platforms without a portable directory-fsync API
    // still get the atomic rename (and Windows uses WRITE_THROUGH above), but
    // cannot provide the same directory-entry durability guarantee.
    #[cfg(unix)]
    {
        let dir = fs::File::open(parent).await?;
        dir.sync_all().await?;
    }
    Ok(())
}

#[cfg(not(windows))]
async fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to).await
}

#[cfg(windows)]
async fn replace_file(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
    }

    let mut from_wide: Vec<u16> = from.as_os_str().encode_wide().collect();
    from_wide.push(0);
    let mut to_wide: Vec<u16> = to.as_os_str().encode_wide().collect();
    to_wide.push(0);
    let status = unsafe {
        MoveFileExW(
            from_wide.as_ptr(),
            to_wide.as_ptr(),
            0x0000_0001 | 0x0000_0008, // REPLACE_EXISTING | WRITE_THROUGH
        )
    };
    if status == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
