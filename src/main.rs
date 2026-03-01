mod convert;
mod index;
mod install;
mod mcp;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "chatgpt2md",
    version,
    about = "Convert ChatGPT export to Markdown with full-text search and MCP server for Claude"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Convert ChatGPT export (.zip or .json) to organized Markdown files
    Convert(convert::ConvertArgs),

    /// Start MCP server for Claude integration (stdio transport)
    Serve(mcp::ServeArgs),

    /// Install MCP server configuration into Claude Desktop and/or Claude Code
    Install(install::InstallArgs),
}

fn main() {
    // Backward compatibility: if first arg looks like a file path (not a subcommand),
    // insert "convert" before it so clap can parse it.
    let args: Vec<String> = std::env::args().collect();
    let effective_args = maybe_insert_convert_subcommand(args);

    let cli = Cli::parse_from(effective_args);

    match cli.command {
        Some(Commands::Convert(args)) => convert::run(args),
        Some(Commands::Serve(args)) => mcp::run(args),
        Some(Commands::Install(args)) => install::run(args),
        None => {
            // No subcommand and no file arg — show help
            use clap::CommandFactory;
            Cli::command().print_help().ok();
            println!();
        }
    }
}

/// If the first real argument (after binary name) is not a known subcommand
/// and looks like a file path, insert "convert" before it.
fn maybe_insert_convert_subcommand(args: Vec<String>) -> Vec<String> {
    if args.len() < 2 {
        return args;
    }

    let first_arg = &args[1];

    // Known subcommands and flags
    let known = ["convert", "serve", "install", "--help", "-h", "--version", "-V", "help"];
    if known.iter().any(|k| first_arg == k) {
        return args;
    }

    // If it looks like a file path or doesn't start with --, assume convert
    if !first_arg.starts_with('-') || first_arg.contains('.') || first_arg.contains('/') || first_arg.contains('\\') {
        let mut new_args = Vec::with_capacity(args.len() + 1);
        new_args.push(args[0].clone());
        new_args.push("convert".to_string());
        new_args.extend_from_slice(&args[1..]);
        return new_args;
    }

    args
}
