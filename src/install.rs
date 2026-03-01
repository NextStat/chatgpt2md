use clap::Args;
use std::path::PathBuf;

/// Install chatgpt2md MCP server into Claude Desktop and/or Claude Code.
#[derive(Args, Debug)]
pub struct InstallArgs {
    /// Path to the search index directory
    #[arg(long)]
    pub index: PathBuf,

    /// Path to the chats directory
    #[arg(long)]
    pub chats: PathBuf,

    /// Only install for Claude Desktop
    #[arg(long, default_value_t = false)]
    pub desktop_only: bool,

    /// Only install for Claude Code
    #[arg(long, default_value_t = false)]
    pub code_only: bool,
}

pub fn run(args: InstallArgs) {
    let binary = std::env::current_exe().unwrap_or_else(|e| {
        eprintln!("Failed to determine binary path: {}", e);
        std::process::exit(1);
    });

    let index_path = std::fs::canonicalize(&args.index).unwrap_or_else(|e| {
        eprintln!("Failed to resolve index path {:?}: {}", args.index, e);
        std::process::exit(1);
    });

    let chats_path = std::fs::canonicalize(&args.chats).unwrap_or_else(|e| {
        eprintln!("Failed to resolve chats path {:?}: {}", args.chats, e);
        std::process::exit(1);
    });

    let binary_str = binary.to_string_lossy().to_string();
    let index_str = index_path.to_string_lossy().to_string();
    let chats_str = chats_path.to_string_lossy().to_string();

    let install_desktop = !args.code_only;
    let install_code = !args.desktop_only;

    if install_desktop {
        match install_claude_desktop(&binary_str, &index_str, &chats_str) {
            Ok(()) => eprintln!("Claude Desktop: MCP server configured successfully."),
            Err(e) => eprintln!("Claude Desktop: {}", e),
        }
    }

    if install_code {
        match install_claude_code(&binary_str, &index_str, &chats_str) {
            Ok(()) => eprintln!("Claude Code: MCP server configured successfully."),
            Err(e) => eprintln!("Claude Code: {}", e),
        }
    }

    eprintln!("\nRestart Claude Desktop / Claude Code for changes to take effect.");
}

fn install_claude_desktop(binary: &str, index: &str, chats: &str) -> Result<(), String> {
    let config_path = claude_desktop_config_path()
        .ok_or_else(|| "Could not determine Claude Desktop config path for this OS.".to_string())?;

    // Read existing config or start fresh
    let mut config: serde_json::Value = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse config: {}", e))?
    } else {
        serde_json::json!({})
    };

    // Ensure mcpServers object exists
    if config.get("mcpServers").is_none() {
        config["mcpServers"] = serde_json::json!({});
    }

    // Add our server entry
    config["mcpServers"]["chatgpt-history"] = serde_json::json!({
        "command": binary,
        "args": ["serve", "--index", index, "--chats", chats]
    });

    // Write back with pretty-print
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
    }

    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    std::fs::write(&config_path, json)
        .map_err(|e| format!("Failed to write config: {}", e))?;

    eprintln!("  Config written to: {}", config_path.display());
    Ok(())
}

fn install_claude_code(binary: &str, index: &str, chats: &str) -> Result<(), String> {
    // Check if `claude` CLI is available
    let status = std::process::Command::new("claude")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    match status {
        Ok(s) if s.success() => {}
        _ => {
            return Err("'claude' CLI not found in PATH. Install Claude Code first.".to_string());
        }
    }

    let output = std::process::Command::new("claude")
        .args([
            "mcp", "add",
            "--transport", "stdio",
            "--scope", "user",
            "chatgpt-history",
            "--",
            binary,
            "serve",
            "--index", index,
            "--chats", chats,
        ])
        .output()
        .map_err(|e| format!("Failed to run 'claude mcp add': {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("'claude mcp add' failed: {}", stderr));
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn claude_desktop_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| {
        h.join("Library")
            .join("Application Support")
            .join("Claude")
            .join("claude_desktop_config.json")
    })
}

#[cfg(target_os = "windows")]
fn claude_desktop_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|c| {
        c.join("Claude")
            .join("claude_desktop_config.json")
    })
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn claude_desktop_config_path() -> Option<PathBuf> {
    // Linux: XDG config
    dirs::config_dir().map(|c| {
        c.join("Claude")
            .join("claude_desktop_config.json")
    })
}
