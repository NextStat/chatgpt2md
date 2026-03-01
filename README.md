# chatgpt2md

> Take your ChatGPT history with you to Claude. Full-text search included.

A fast CLI tool that converts your ChatGPT export into organized Markdown files, builds a search index, and provides an MCP server so Claude can search and read your conversation history natively.

**Key features:**
- Converts ChatGPT export (ZIP or JSON) to clean Markdown files
- Built-in full-text search powered by [Tantivy](https://github.com/quickwit-oss/tantivy)
- MCP server with 3 tools for Claude integration
- One-command auto-install for Claude Desktop and Claude Code
- Works on macOS and Windows
- Single binary, no runtime dependencies

## Table of Contents

- [Quick Start](#quick-start)
- [Step 1: Export Your ChatGPT Data](#step-1-export-your-chatgpt-data)
- [Step 2: Install chatgpt2md](#step-2-install-chatgpt2md)
- [Step 3: Convert Your Export](#step-3-convert-your-export)
- [Step 4: Connect to Claude](#step-4-connect-to-claude)
- [Step 5: Use It!](#step-5-use-it)
- [Commands Reference](#commands-reference)
- [Manual MCP Configuration](#manual-mcp-configuration)
- [Output Structure](#output-structure)
- [Building from Source](#building-from-source)
- [License](#license)

## Quick Start

```bash
# Install
cargo install --git https://github.com/NextStat/chatgpt2md

# Convert your ChatGPT export
chatgpt2md export.zip

# Install MCP into Claude (auto-configures everything)
chatgpt2md install --index ./chatgpt_chats/.index --chats ./chatgpt_chats

# Restart Claude — done!
```

## Step 1: Export Your ChatGPT Data

1. Go to [chatgpt.com](https://chatgpt.com)
2. Click your profile icon (bottom-left) > **Settings**
3. Go to **Data controls**
4. Click **Export data** > **Confirm export**
5. Wait for the email from OpenAI (usually arrives within a few minutes)
6. Download the ZIP file from the link in the email

The ZIP file contains `conversations.json` with all your chat history.

> **Tip:** The export includes ALL your conversations — this may take a while if you have thousands of chats.

## Step 2: Install chatgpt2md

### Option A: Pre-built binaries (recommended)

Download the latest release for your platform from the [Releases page](https://github.com/NextStat/chatgpt2md/releases):

| Platform | File |
|----------|------|
| macOS (Apple Silicon) | `chatgpt2md-aarch64-apple-darwin.tar.gz` |
| macOS (Intel) | `chatgpt2md-x86_64-apple-darwin.tar.gz` |
| Windows | `chatgpt2md-x86_64-pc-windows-msvc.zip` |
| Linux | `chatgpt2md-x86_64-unknown-linux-gnu.tar.gz` |

```bash
# macOS / Linux: extract and move to PATH
tar xzf chatgpt2md-*.tar.gz
sudo mv chatgpt2md /usr/local/bin/
```

### Option B: Install from source

Requires [Rust](https://rustup.rs/) 1.85+:

```bash
cargo install --git https://github.com/NextStat/chatgpt2md
```

### Option C: Build locally

```bash
git clone https://github.com/NextStat/chatgpt2md
cd chatgpt2md
cargo build --release
# Binary is at ./target/release/chatgpt2md
```

## Step 3: Convert Your Export

```bash
# Basic usage — just pass the ZIP file
chatgpt2md export.zip
```

This will:
1. Extract `conversations.json` from the ZIP
2. Convert each conversation to a `.md` file
3. Organize files by year/month folders
4. Build a full-text search index

Output goes to `./chatgpt_chats/` by default. You can change this:

```bash
# Custom output directory
chatgpt2md export.zip -o ~/Documents/my_chatgpt_history

# Flat structure (no year/month folders)
chatgpt2md export.zip --flat

# Include system/tool messages
chatgpt2md export.zip --include-system

# Skip search index (just convert to Markdown)
chatgpt2md export.zip --no-index
```

**Expected output:**

```
Found 1847 conversations. Converting to Markdown...
 ████████████████████████████████████████ 1847/1847

Done!
   Conversations converted: 1823
   Conversations skipped (empty): 24
   Total messages: 41562
   Output directory: chatgpt_chats
   Building search index...
   Indexed 1823 conversations in chatgpt_chats/.index
```

## Step 4: Connect to Claude

### Automatic (recommended)

```bash
chatgpt2md install --index ./chatgpt_chats/.index --chats ./chatgpt_chats
```

This configures both **Claude Desktop** and **Claude Code** automatically.

> After running install, **restart Claude Desktop** for changes to take effect.

**Options:**

```bash
# Only Claude Desktop
chatgpt2md install --index ./chatgpt_chats/.index --chats ./chatgpt_chats --desktop-only

# Only Claude Code
chatgpt2md install --index ./chatgpt_chats/.index --chats ./chatgpt_chats --code-only
```

### Manual setup

See [Manual MCP Configuration](#manual-mcp-configuration) below if you prefer to configure manually.

## Step 5: Use It!

Once connected, you can ask Claude things like:

- *"Search my ChatGPT history for discussions about Rust async"*
- *"Find all my conversations about machine learning from 2024"*
- *"Show me my chat about the pasta carbonara recipe"*
- *"What did I discuss about Docker in January 2025?"*
- *"List all my conversations from March 2024"*

Claude will use the MCP tools automatically:

| Tool | What it does |
|------|-------------|
| `search_conversations` | Full-text search across all conversations by keywords |
| `get_conversation` | Read the complete text of a specific conversation |
| `list_conversations` | Browse conversations by year and/or month |

### Example session in Claude

> **You:** Search my ChatGPT history for anything about React performance optimization
>
> **Claude:** *(uses search_conversations)* I found 3 relevant conversations:
>
> 1. **React Performance Deep Dive** (2024-09-15) — 24 messages
> 2. **Optimizing Re-renders** (2024-07-03) — 18 messages
> 3. **Virtual DOM vs Signals** (2025-01-12) — 32 messages
>
> Would you like me to read any of these in full?
>
> **You:** Yes, show me the first one
>
> **Claude:** *(uses get_conversation)* Here's the full conversation...

## Commands Reference

### `convert` (default)

Convert ChatGPT export to Markdown files. This is the default command — you can omit `convert`.

```
chatgpt2md [convert] <INPUT> [OPTIONS]

Arguments:
  <INPUT>              Path to .zip export or conversations.json

Options:
  -o, --output <DIR>   Output directory [default: chatgpt_chats]
      --flat           No year/month subdirectories
      --include-system  Include system/tool messages
      --no-index       Skip building search index
```

### `serve`

Start MCP server (stdio transport). This is called automatically by Claude — you don't need to run it manually.

```
chatgpt2md serve --index <PATH> --chats <PATH>
```

### `install`

Auto-configure Claude Desktop and/or Claude Code.

```
chatgpt2md install --index <PATH> --chats <PATH> [--desktop-only] [--code-only]
```

## Manual MCP Configuration

### Claude Desktop

Edit the config file:
- **macOS:** `~/Library/Application Support/Claude/claude_desktop_config.json`
- **Windows:** `%APPDATA%\Claude\claude_desktop_config.json`

Add the `chatgpt-history` server:

```json
{
  "mcpServers": {
    "chatgpt-history": {
      "command": "/path/to/chatgpt2md",
      "args": ["serve", "--index", "/path/to/chatgpt_chats/.index", "--chats", "/path/to/chatgpt_chats"]
    }
  }
}
```

Replace `/path/to/` with actual absolute paths.

### Claude Code

```bash
claude mcp add --transport stdio --scope user chatgpt-history -- \
  /path/to/chatgpt2md serve \
  --index /path/to/chatgpt_chats/.index \
  --chats /path/to/chatgpt_chats
```

## Output Structure

```
chatgpt_chats/
├── .index/                              # Tantivy search index (used by MCP server)
├── 2024/
│   ├── 01/
│   │   ├── 2024-01-15_My_first_chat.md
│   │   └── 2024-01-20_Debugging_Rust.md
│   ├── 02/
│   │   └── ...
│   └── 12/
│       └── ...
├── 2025/
│   └── ...
└── undated/
    └── Untitled_conversation.md
```

Each `.md` file has YAML frontmatter:

```yaml
---
title: "My first chat"
source: chatgpt
created: 2024-01-15 10:30:00 UTC
updated: 2024-01-15 11:45:00 UTC
message_count: 12
---
```

These files work great with Obsidian, VS Code, or any Markdown viewer.

## Building from Source

Requires [Rust](https://rustup.rs/) 1.85 or later.

```bash
git clone https://github.com/NextStat/chatgpt2md
cd chatgpt2md
cargo build --release
```

The binary will be at `./target/release/chatgpt2md` (6-7 MB).

## License

MIT
