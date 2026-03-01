use chrono::{DateTime, Utc};
use clap::Args;
use indicatif::{ProgressBar, ProgressStyle};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::index::{self, ConversationMeta};

/// Convert ChatGPT export to organized Markdown files.
///
/// Accepts either a .zip export file or an extracted conversations.json.
/// Output is organized into folders by year/month with one .md file per conversation.
#[derive(Args, Debug)]
pub struct ConvertArgs {
    /// Path to ChatGPT export .zip file or conversations.json
    #[arg()]
    pub input: PathBuf,

    /// Output directory (default: ./chatgpt_chats)
    #[arg(short, long, default_value = "chatgpt_chats")]
    pub output: PathBuf,

    /// Flatten output (no year/month subdirectories)
    #[arg(long, default_value_t = false)]
    pub flat: bool,

    /// Include system/tool messages
    #[arg(long, default_value_t = false)]
    pub include_system: bool,

    /// Skip building the search index
    #[arg(long, default_value_t = false)]
    pub no_index: bool,
}

/// Top-level conversation object from conversations.json
#[derive(Deserialize, Debug)]
pub(crate) struct Conversation {
    pub title: Option<String>,
    pub create_time: Option<f64>,
    pub update_time: Option<f64>,
    pub mapping: HashMap<String, Node>,
    pub current_node: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
}

/// A node in the conversation tree
#[derive(Deserialize, Debug)]
pub(crate) struct Node {
    pub message: Option<Message>,
    pub parent: Option<String>,
    #[allow(dead_code)]
    pub children: Option<Vec<String>>,
}

/// A message within a node
#[derive(Deserialize, Debug)]
pub(crate) struct Message {
    pub author: Option<Author>,
    pub content: Option<Content>,
    pub create_time: Option<f64>,
    pub metadata: Option<Value>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct Author {
    pub role: Option<String>,
    #[allow(dead_code)]
    pub name: Option<String>,
}

#[derive(Deserialize, Debug)]
pub(crate) struct Content {
    pub content_type: Option<String>,
    pub parts: Option<Vec<Value>>,
}

/// Extracted message ready for rendering
pub(crate) struct ExtractedMessage {
    pub role: String,
    pub text: String,
    pub timestamp: Option<DateTime<Utc>>,
}

#[derive(Default)]
struct Stats {
    converted: usize,
    skipped: usize,
    total_messages: usize,
}

pub fn run(args: ConvertArgs) {
    let json_data = load_conversations(&args.input);
    let conversations: Vec<Conversation> = match serde_json::from_str(&json_data) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error parsing conversations.json: {}", e);
            std::process::exit(1);
        }
    };

    eprintln!(
        "Found {} conversations. Converting to Markdown...",
        conversations.len()
    );

    let pb = ProgressBar::new(conversations.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("█▓░"),
    );

    let mut stats = Stats::default();
    let mut metas: Vec<ConversationMeta> = Vec::new();

    for conv in &conversations {
        let title = conv
            .title
            .as_deref()
            .unwrap_or("Untitled")
            .to_string();

        pb.set_message(truncate(&title, 40));

        let messages = extract_messages(conv, args.include_system);
        if messages.is_empty() {
            stats.skipped += 1;
            pb.inc(1);
            continue;
        }

        let create_time = conv
            .create_time
            .and_then(|t| DateTime::from_timestamp(t as i64, 0));

        let update_time = conv
            .update_time
            .and_then(|t| DateTime::from_timestamp(t as i64, 0));

        let markdown = render_markdown(&title, create_time, update_time, &messages);

        let out_path = build_output_path(&args.output, &title, create_time, args.flat);

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|e| {
                eprintln!("Failed to create directory {:?}: {}", parent, e);
            });
        }

        fs::write(&out_path, &markdown).unwrap_or_else(|e| {
            eprintln!("Failed to write {:?}: {}", out_path, e);
        });

        // Collect body text for indexing
        let body: String = messages.iter().map(|m| m.text.as_str()).collect::<Vec<_>>().join("\n");

        let conv_id = conv
            .id
            .clone()
            .unwrap_or_else(|| {
                // Fallback: use sanitized title + date as ID
                let date_str = create_time
                    .map(|d| d.format("%Y%m%d").to_string())
                    .unwrap_or_default();
                format!("{}_{}", date_str, sanitize_filename::sanitize(&title))
            });

        metas.push(ConversationMeta {
            id: conv_id,
            title: title.clone(),
            body,
            date: create_time.map(|d| d.format("%Y-%m-%d").to_string()),
            year: create_time.map(|d| d.format("%Y").to_string()),
            month: create_time.map(|d| d.format("%m").to_string()),
            message_count: messages.len() as u64,
            file_path: out_path.to_string_lossy().to_string(),
        });

        stats.converted += 1;
        stats.total_messages += messages.len();
        pb.inc(1);
    }

    pb.finish_and_clear();

    eprintln!("\nDone!");
    eprintln!("   Conversations converted: {}", stats.converted);
    eprintln!("   Conversations skipped (empty): {}", stats.skipped);
    eprintln!("   Total messages: {}", stats.total_messages);
    eprintln!("   Output directory: {}", args.output.display());

    // Build search index
    if !args.no_index && !metas.is_empty() {
        let index_path = args.output.join(".index");
        eprintln!("   Building search index...");
        match index::build_index(&index_path, &metas) {
            Ok(count) => eprintln!("   Indexed {} conversations in {}", count, index_path.display()),
            Err(e) => eprintln!("   Warning: failed to build index: {}", e),
        }
    }
}

/// Load JSON from either a .zip file or a plain .json file
pub(crate) fn load_conversations(path: &Path) -> String {
    if path.extension().map_or(false, |ext| ext == "zip") {
        load_from_zip(path)
    } else {
        fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("Failed to read {:?}: {}", path, e);
            std::process::exit(1);
        })
    }
}

fn load_from_zip(path: &Path) -> String {
    let file = fs::File::open(path).unwrap_or_else(|e| {
        eprintln!("Failed to open zip {:?}: {}", path, e);
        std::process::exit(1);
    });

    let mut archive = zip::ZipArchive::new(file).unwrap_or_else(|e| {
        eprintln!("Failed to read zip archive: {}", e);
        std::process::exit(1);
    });

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        if entry.name().ends_with("conversations.json") {
            let mut contents = String::new();
            entry.read_to_string(&mut contents).unwrap_or_else(|e| {
                eprintln!("Failed to read conversations.json from zip: {}", e);
                std::process::exit(1);
            });
            return contents;
        }
    }

    eprintln!("conversations.json not found in zip archive");
    std::process::exit(1);
}

/// Walk the conversation tree from current_node up to root, then reverse
pub(crate) fn extract_messages(conv: &Conversation, include_system: bool) -> Vec<ExtractedMessage> {
    let mut messages = Vec::new();
    let mut current = conv.current_node.clone();

    while let Some(node_id) = current {
        if let Some(node) = conv.mapping.get(&node_id) {
            if let Some(ref message) = node.message {
                if let Some(extracted) = process_message(message, include_system) {
                    messages.push(extracted);
                }
            }
            current = node.parent.clone();
        } else {
            break;
        }
    }

    messages.reverse();
    messages
}

fn process_message(message: &Message, include_system: bool) -> Option<ExtractedMessage> {
    let role = message
        .author
        .as_ref()
        .and_then(|a| a.role.as_deref())
        .unwrap_or("unknown");

    if role == "system" {
        if !include_system {
            let is_user_system = message
                .metadata
                .as_ref()
                .and_then(|m| m.get("is_user_system_message"))
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !is_user_system {
                return None;
            }
        }
    }

    if role == "tool" && !include_system {
        return None;
    }

    let content = message.content.as_ref()?;
    let content_type = content.content_type.as_deref().unwrap_or("");

    let text = match content_type {
        "text" => extract_text_parts(content),
        "code" => extract_code_content(content),
        "multimodal_text" => extract_multimodal_parts(content),
        _ => return None,
    };

    let text = text.trim().to_string();
    if text.is_empty() {
        return None;
    }

    let timestamp = message
        .create_time
        .and_then(|t| DateTime::from_timestamp(t as i64, 0));

    let display_role = match role {
        "user" => "User".to_string(),
        "assistant" => "Assistant".to_string(),
        "system" => "System".to_string(),
        "tool" => "Tool".to_string(),
        other => other.to_string(),
    };

    Some(ExtractedMessage {
        role: display_role,
        text,
        timestamp,
    })
}

fn extract_text_parts(content: &Content) -> String {
    content
        .parts
        .as_ref()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| {
                    if let Some(s) = part.as_str() {
                        Some(s.to_string())
                    } else if part.is_object() {
                        extract_asset_reference(part)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn extract_multimodal_parts(content: &Content) -> String {
    content
        .parts
        .as_ref()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| {
                    if let Some(s) = part.as_str() {
                        Some(s.to_string())
                    } else if let Some(obj) = part.as_object() {
                        if obj.contains_key("content_type") {
                            let ct = obj
                                .get("content_type")
                                .and_then(|v| v.as_str())
                                .unwrap_or("");
                            if ct == "image_asset_pointer" {
                                Some("*[Image]*".to_string())
                            } else {
                                Some(format!("*[{}]*", ct))
                            }
                        } else {
                            extract_asset_reference(part)
                        }
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn extract_code_content(content: &Content) -> String {
    content
        .parts
        .as_ref()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|p| p.as_str().map(|s| format!("```\n{}\n```", s)))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn extract_asset_reference(value: &Value) -> Option<String> {
    if let Some(obj) = value.as_object() {
        if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
            Some(format!("*[File: {}]*", name))
        } else if obj.contains_key("asset_pointer") {
            Some("*[Image]*".to_string())
        } else {
            None
        }
    } else {
        None
    }
}

/// Render a conversation as a Markdown document
fn render_markdown(
    title: &str,
    create_time: Option<DateTime<Utc>>,
    update_time: Option<DateTime<Utc>>,
    messages: &[ExtractedMessage],
) -> String {
    let mut md = String::with_capacity(4096);

    md.push_str("---\n");
    md.push_str(&format!("title: \"{}\"\n", title.replace('"', "\\\"")));
    md.push_str("source: chatgpt\n");
    if let Some(dt) = create_time {
        md.push_str(&format!("created: {}\n", dt.format("%Y-%m-%d %H:%M:%S UTC")));
    }
    if let Some(dt) = update_time {
        md.push_str(&format!("updated: {}\n", dt.format("%Y-%m-%d %H:%M:%S UTC")));
    }
    md.push_str(&format!("message_count: {}\n", messages.len()));
    md.push_str("---\n\n");

    md.push_str(&format!("# {}\n\n", title));

    for msg in messages {
        let timestamp_str = msg
            .timestamp
            .map(|t| format!(" _{}_", t.format("%Y-%m-%d %H:%M")))
            .unwrap_or_default();

        md.push_str(&format!("## {}{}\n\n", msg.role, timestamp_str));
        md.push_str(&msg.text);
        md.push_str("\n\n---\n\n");
    }

    md
}

/// Build the output file path
fn build_output_path(
    base: &Path,
    title: &str,
    create_time: Option<DateTime<Utc>>,
    flat: bool,
) -> PathBuf {
    let safe_title = sanitize_filename::sanitize_with_options(
        title,
        sanitize_filename::Options {
            truncate: true,
            windows: true,
            replacement: "_",
        },
    );

    let safe_title = if safe_title.len() > 120 {
        safe_title[..120].to_string()
    } else {
        safe_title
    };

    let filename = if let Some(dt) = create_time {
        format!("{}_{}.md", dt.format("%Y-%m-%d"), safe_title)
    } else {
        format!("{}.md", safe_title)
    };

    if flat {
        base.join(&filename)
    } else if let Some(dt) = create_time {
        base.join(dt.format("%Y").to_string())
            .join(dt.format("%m").to_string())
            .join(&filename)
    } else {
        base.join("undated").join(&filename)
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.min(s.len())])
    }
}
