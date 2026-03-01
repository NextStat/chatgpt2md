use std::path::PathBuf;
use std::sync::Arc;

use clap::Args;
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::*;
use rmcp::service::ServiceExt;
use rmcp::{tool, tool_router, ErrorData as McpError};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::index::SearchIndex;

/// Start MCP server for Claude integration (stdio transport).
#[derive(Args, Debug)]
pub struct ServeArgs {
    /// Path to the search index directory
    #[arg(long)]
    pub index: PathBuf,

    /// Path to the chats directory (contains .md files)
    #[arg(long)]
    pub chats: PathBuf,
}

// --- Tool parameter types ---

#[derive(Deserialize, JsonSchema)]
pub struct SearchParams {
    /// Search query string
    pub query: String,
    /// Maximum number of results (default: 20)
    pub limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct GetConversationParams {
    /// Conversation ID to retrieve
    pub id: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ListParams {
    /// Filter by year (e.g. "2024")
    pub year: Option<String>,
    /// Filter by month (e.g. "03")
    pub month: Option<String>,
    /// Maximum number of results (default: 50)
    pub limit: Option<usize>,
}

// --- MCP Server ---

#[derive(Clone)]
pub struct ChatHistoryServer {
    search_index: Arc<SearchIndex>,
    chats_dir: PathBuf,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
}

#[tool_router]
impl ChatHistoryServer {
    pub fn new(search_index: SearchIndex, chats_dir: PathBuf) -> Self {
        Self {
            search_index: Arc::new(search_index),
            chats_dir,
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "search_conversations",
        description = "Full-text search across ChatGPT conversation history. Returns matching conversations with title, date, message count, and relevance score."
    )]
    async fn search_conversations(
        &self,
        params: Parameters<SearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let limit = params.0.limit.unwrap_or(20).min(100);

        match self.search_index.search(&params.0.query, limit) {
            Ok(results) => {
                let text = if results.is_empty() {
                    "No conversations found matching your query.".to_string()
                } else {
                    let mut out = format!("Found {} conversations:\n\n", results.len());
                    for r in &results {
                        out.push_str(&format!(
                            "- **{}** ({})\n  ID: `{}` | Messages: {} | Score: {:.2}\n",
                            r.title,
                            if r.date.is_empty() { "undated" } else { &r.date },
                            r.id,
                            r.message_count,
                            r.score,
                        ));
                    }
                    out
                };
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => Err(McpError::internal_error(format!("Search failed: {}", e), None)),
        }
    }

    #[tool(
        name = "get_conversation",
        description = "Retrieve the full Markdown content of a specific ChatGPT conversation by its ID."
    )]
    async fn get_conversation(
        &self,
        params: Parameters<GetConversationParams>,
    ) -> Result<CallToolResult, McpError> {
        match self.search_index.get_by_id(&params.0.id) {
            Ok(Some(result)) => {
                let file_path = PathBuf::from(&result.file_path);

                // Try absolute path first, then relative to chats_dir
                let path = if file_path.is_absolute() && file_path.exists() {
                    file_path
                } else {
                    // Try to find relative to chats dir
                    let relative = self.chats_dir.join(
                        file_path
                            .strip_prefix(&self.chats_dir)
                            .unwrap_or(&file_path),
                    );
                    if relative.exists() {
                        relative
                    } else {
                        file_path
                    }
                };

                match std::fs::read_to_string(&path) {
                    Ok(content) => Ok(CallToolResult::success(vec![Content::text(content)])),
                    Err(e) => Err(McpError::internal_error(
                        format!("Failed to read conversation file {:?}: {}", path, e),
                        None,
                    )),
                }
            }
            Ok(None) => Err(McpError::invalid_params(
                format!("Conversation not found: {}", params.0.id),
                None,
            )),
            Err(e) => Err(McpError::internal_error(format!("Lookup failed: {}", e), None)),
        }
    }

    #[tool(
        name = "list_conversations",
        description = "Browse ChatGPT conversations by date. Filter by year and/or month. Returns a list of conversations with metadata."
    )]
    async fn list_conversations(
        &self,
        params: Parameters<ListParams>,
    ) -> Result<CallToolResult, McpError> {
        let limit = params.0.limit.unwrap_or(50).min(200);

        match self.search_index.list_by_date(
            params.0.year.as_deref(),
            params.0.month.as_deref(),
            limit,
        ) {
            Ok(results) => {
                let text = if results.is_empty() {
                    "No conversations found for the specified date filter.".to_string()
                } else {
                    let mut out = format!("Found {} conversations:\n\n", results.len());
                    for r in &results {
                        out.push_str(&format!(
                            "- **{}** ({})\n  ID: `{}` | Messages: {}\n",
                            r.title,
                            if r.date.is_empty() { "undated" } else { &r.date },
                            r.id,
                            r.message_count,
                        ));
                    }
                    out
                };
                Ok(CallToolResult::success(vec![Content::text(text)]))
            }
            Err(e) => Err(McpError::internal_error(format!("List failed: {}", e), None)),
        }
    }
}

/// Implement ServerHandler — get_info provides server metadata,
/// call_tool/list_tools delegate to the tool_router generated by #[tool_router].
impl rmcp::handler::server::ServerHandler for ChatHistoryServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: None,
                }),
                ..Default::default()
            },
            server_info: Implementation {
                name: "chatgpt-history".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: None,
                description: Some("Search and browse your ChatGPT conversation history".to_string()),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Search and retrieve your ChatGPT conversation history. \
                 Use search_conversations to find chats by keywords, \
                 get_conversation to read full chat content, \
                 and list_conversations to browse by date."
                    .into(),
            ),
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, McpError>> + Send + '_ {
        std::future::ready(Ok(ListToolsResult {
            tools: self.tool_router.list_all(),
            ..Default::default()
        }))
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: rmcp::service::RequestContext<rmcp::RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, McpError>> + Send + '_ {
        async move {
            let ctx = rmcp::handler::server::tool::ToolCallContext::new(self, request, context);
            self.tool_router.call(ctx).await
        }
    }
}

pub fn run(args: ServeArgs) {
    // Validate paths
    if !args.index.exists() {
        eprintln!("Error: index directory does not exist: {}", args.index.display());
        eprintln!("Run 'chatgpt2md convert <export>' first to create the index.");
        std::process::exit(1);
    }

    if !args.chats.exists() {
        eprintln!("Error: chats directory does not exist: {}", args.chats.display());
        std::process::exit(1);
    }

    let search_index = match SearchIndex::open(&args.index) {
        Ok(idx) => idx,
        Err(e) => {
            eprintln!("Error: failed to open search index: {}", e);
            std::process::exit(1);
        }
    };

    let server = ChatHistoryServer::new(search_index, args.chats.clone());

    eprintln!("Starting MCP server (stdio transport)...");
    eprintln!("  Index: {}", args.index.display());
    eprintln!("  Chats: {}", args.chats.display());

    let rt = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    rt.block_on(async {
        let transport = rmcp::transport::io::stdio();
        let service = server.serve(transport).await.expect("Failed to start MCP server");
        service.waiting().await.expect("MCP server error");
    });
}
