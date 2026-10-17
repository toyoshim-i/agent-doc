#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use agent_man::workspace::{
        build_workspace_mcp_engine, preprocess_mdbook_json, CreatePageRequest, Workspace,
    };
    use agent_man::RenderMode;
    use clap::{Parser, Subcommand, ValueEnum};
    use std::io::Read;
    use std::path::PathBuf;
    use std::process::ExitCode;

    #[derive(Debug, Clone, Copy, ValueEnum, Default)]
    pub enum OutputFormat {
        #[default]
        Agent,
        Terminal,
        Json,
        Mcp,
    }

    impl From<OutputFormat> for RenderMode {
        fn from(f: OutputFormat) -> Self {
            match f {
                OutputFormat::Agent => RenderMode::Agent,
                OutputFormat::Terminal => RenderMode::Terminal,
                OutputFormat::Json => RenderMode::Json,
                OutputFormat::Mcp => RenderMode::Mcp,
            }
        }
    }

    #[derive(Debug, Parser)]
    #[command(
        name = "agent-man-cli",
        version,
        about = "Dynamic documentation browser, MCP server, and affordance-driven editing proxy for agent-man workspaces"
    )]
    pub struct Cli {
        /// Documentation workspace root directory (containing agent-man.json and .md files)
        #[arg(long, global = true, default_value = "docs")]
        pub root: PathBuf,

        /// Output rendering mode
        #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Agent)]
        pub format: OutputFormat,

        /// Start MCP server over standard input/output (stdio)
        #[arg(long)]
        pub mcp: bool,

        /// Start MCP server over HTTP at the specified address (e.g. 127.0.0.1:8080)
        #[arg(long)]
        pub mcp_http: Option<String>,

        #[command(subcommand)]
        pub command: Option<Commands>,
    }

    #[derive(Debug, Subcommand)]
    pub enum Commands {
        /// Initialize or synchronize agent-man.json by scanning .md files in --root
        Init {
            /// Project or documentation title
            #[arg(long)]
            name: Option<String>,
            /// Short description of the documentation workspace
            #[arg(long)]
            description: Option<String>,
        },

        /// Browse the workspace index overview or read a specific topic by ID/alias
        Guide {
            /// Topic ID, alias, or search query (omit to view top-level index)
            topic: Option<String>,
            /// Optional section anchor to extract
            #[arg(long)]
            section: Option<String>,
        },

        /// Explain a specific rule or topic by ID/alias
        Explain {
            /// Rule or topic ID
            topic: String,
            /// Optional section anchor to extract
            #[arg(long)]
            section: Option<String>,
        },

        /// List topics, optionally filtering by category or expanding folded categories (like 'record')
        List {
            /// Filter by category key (e.g. 'entrypoint', 'guide', 'rule', 'record')
            #[arg(long)]
            category: Option<String>,
            /// Include folded categories when listing all topics
            #[arg(long)]
            include_folded: bool,
        },

        /// View raw markdown lines with 1-indexed line numbers before making partial edits
        Lines {
            /// Topic ID
            id: String,
            /// 1-indexed start line
            #[arg(long)]
            start: Option<usize>,
            /// 1-indexed end line (inclusive)
            #[arg(long)]
            end: Option<usize>,
        },

        /// Insert new line(s) into a [readwrite] page before --line (or append at EOF if --line is omitted)
        Insert {
            /// Topic ID
            id: String,
            /// 1-indexed line number to insert before (omit to append at the end of file)
            #[arg(long)]
            line: Option<usize>,
            /// Markdown content to insert (may contain newlines)
            #[arg(long)]
            content: String,
        },

        /// Replace a 1-indexed inclusive line range (start..=end) in a [readwrite] page
        Replace {
            /// Topic ID
            id: String,
            /// 1-indexed start line (inclusive)
            #[arg(long)]
            start: usize,
            /// 1-indexed end line (inclusive)
            #[arg(long)]
            end: usize,
            /// Replacement markdown content
            #[arg(long)]
            content: String,
        },

        /// Delete a 1-indexed inclusive line range (start..=end) from a [readwrite] page
        DeleteLines {
            /// Topic ID
            id: String,
            /// 1-indexed start line (inclusive)
            #[arg(long)]
            start: usize,
            /// 1-indexed end line (inclusive)
            #[arg(long)]
            end: usize,
        },

        /// Create a new [readwrite] documentation page and register it in agent-man.json
        Create {
            /// Unique page ID (e.g. 'auth-flow')
            #[arg(long)]
            id: String,
            /// Display title
            #[arg(long)]
            title: String,
            /// One-line summary
            #[arg(long)]
            summary: String,
            /// Category key (defaults to 'guide')
            #[arg(long)]
            category: Option<String>,
            /// Relative file path inside --root (defaults to '<category>/<id>.md')
            #[arg(long)]
            path: Option<String>,
            /// Optional lookup aliases
            #[arg(long = "alias")]
            aliases: Vec<String>,
            /// Initial markdown body content
            #[arg(long, default_value = "")]
            content: String,
        },

        /// Update metadata (title, summary, category, aliases) of a [readwrite] page in agent-man.json
        Meta {
            /// Topic ID
            id: String,
            /// New display title
            #[arg(long)]
            title: Option<String>,
            /// New one-line summary
            #[arg(long)]
            summary: Option<String>,
            /// New category key
            #[arg(long)]
            category: Option<String>,
            /// Replace lookup aliases (specify multiple times: --alias a --alias b)
            #[arg(long = "alias")]
            aliases: Option<Vec<String>>,
        },

        /// Remove a [readwrite] page from disk and agent-man.json
        Remove {
            /// Topic ID
            id: String,
            /// Force removal even if other pages link to this page
            #[arg(long)]
            force: bool,
        },

        /// Validate workspace integrity (broken links, missing files, unregistered .md files, orphan pages)
        Lint,

        /// Generate mdBook SUMMARY.md from agent-man.json
        Summary {
            /// Write directly to <root>/SUMMARY.md instead of stdout
            #[arg(long)]
            write: bool,
        },

        /// Native mdBook preprocessor that strips YAML frontmatter during `mdbook build`
        MdbookPreprocessor {
            /// Subcommand passed by mdBook (e.g. `supports <renderer>`)
            sub: Vec<String>,
        },
    }

    pub fn run() -> ExitCode {
        let cli = Cli::parse();
        let mode: RenderMode = cli.format.into();
        let prefix = format!("agent-man-cli --root {}", cli.root.display());

        if cli.mcp {
            match build_workspace_mcp_engine(&cli.root, &prefix) {
                Ok(engine) => {
                    if let Err(e) = engine.serve_stdio() {
                        eprintln!("Error serving MCP over stdio: {e}");
                        return ExitCode::FAILURE;
                    }
                    return ExitCode::SUCCESS;
                }
                Err(e) => {
                    eprintln!("Error loading workspace for MCP server: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }

        if let Some(ref addr) = cli.mcp_http {
            match build_workspace_mcp_engine(&cli.root, &prefix) {
                Ok(engine) => match engine.serve_http(addr.as_str()) {
                    Ok(server) => {
                        eprintln!("Serving MCP HTTP at http://{}", server.local_addr());
                        let _ = server.join();
                        return ExitCode::SUCCESS;
                    }
                    Err(e) => {
                        eprintln!("Error starting MCP HTTP server on {addr}: {e}");
                        return ExitCode::FAILURE;
                    }
                },
                Err(e) => {
                    eprintln!("Error loading workspace for MCP HTTP server: {e}");
                    return ExitCode::FAILURE;
                }
            }
        }

        let cmd = cli.command.unwrap_or(Commands::Guide {
            topic: None,
            section: None,
        });

        match cmd {
            Commands::Init { name, description } => {
                match Workspace::init(&cli.root, name.as_deref(), description.as_deref()) {
                    Ok(report) => {
                        if mode == RenderMode::Json {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(&report).unwrap_or_default()
                            );
                        } else {
                            println!(
                                "Initialized workspace manifest at '{}': {} new page(s) added ({} total).",
                                report.manifest_path,
                                report.added_pages.len(),
                                report.total_pages
                            );
                            if !report.added_pages.is_empty() {
                                println!("Added IDs: {}", report.added_pages.join(", "));
                            }
                            println!("\nNext: run '{prefix} guide' to browse the workspace.");
                        }
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        ExitCode::FAILURE
                    }
                }
            }

            Commands::Guide { topic, section } => {
                match Workspace::load(&cli.root, &prefix) {
                    Ok(ws) => match topic {
                        None => {
                            print!("{}", ws.overview(mode));
                            ExitCode::SUCCESS
                        }
                        Some(t) => match ws.read_page(&t, section.as_deref(), mode) {
                            Ok(text) => {
                                print!("{text}");
                                ExitCode::SUCCESS
                            }
                            Err(e) => {
                                eprintln!("Error: {e}");
                                ExitCode::FAILURE
                            }
                        },
                    },
                    Err(e) => {
                        eprintln!("Error: {e}");
                        ExitCode::FAILURE
                    }
                }
            }

            Commands::Explain { topic, section } => match Workspace::load(&cli.root, &prefix) {
                Ok(ws) => match ws.read_page(&topic, section.as_deref(), mode) {
                    Ok(text) => {
                        print!("{text}");
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        ExitCode::FAILURE
                    }
                },
                Err(e) => {
                    eprintln!("Error: {e}");
                    ExitCode::FAILURE
                }
            },

            Commands::List {
                category,
                include_folded,
            } => match Workspace::load(&cli.root, &prefix) {
                Ok(ws) => {
                    print!(
                        "{}",
                        ws.list_pages(category.as_deref(), include_folded, mode)
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    ExitCode::FAILURE
                }
            },

            Commands::Lines { id, start, end } => match Workspace::load(&cli.root, &prefix) {
                Ok(ws) => match ws.lines(&id, start, end) {
                    Ok(text) => {
                        print!("{text}");
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        ExitCode::FAILURE
                    }
                },
                Err(e) => {
                    eprintln!("Error: {e}");
                    ExitCode::FAILURE
                }
            },

            Commands::Insert { id, line, content } => match Workspace::load(&cli.root, &prefix) {
                Ok(mut ws) => match ws.insert(&id, line, &content) {
                    Ok(report) => {
                        print!("{report}");
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        ExitCode::FAILURE
                    }
                },
                Err(e) => {
                    eprintln!("Error: {e}");
                    ExitCode::FAILURE
                }
            },

            Commands::Replace {
                id,
                start,
                end,
                content,
            } => match Workspace::load(&cli.root, &prefix) {
                Ok(mut ws) => match ws.replace(&id, start, end, &content) {
                    Ok(report) => {
                        print!("{report}");
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        ExitCode::FAILURE
                    }
                },
                Err(e) => {
                    eprintln!("Error: {e}");
                    ExitCode::FAILURE
                }
            },

            Commands::DeleteLines { id, start, end } => match Workspace::load(&cli.root, &prefix) {
                Ok(mut ws) => match ws.delete_lines(&id, start, end) {
                    Ok(report) => {
                        print!("{report}");
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        ExitCode::FAILURE
                    }
                },
                Err(e) => {
                    eprintln!("Error: {e}");
                    ExitCode::FAILURE
                }
            },

            Commands::Create {
                id,
                title,
                summary,
                category,
                path,
                aliases,
                content,
            } => match Workspace::load(&cli.root, &prefix) {
                Ok(mut ws) => {
                    let req = CreatePageRequest {
                        id,
                        title,
                        summary,
                        category,
                        path,
                        aliases,
                        content,
                    };
                    match ws.create_page(req) {
                        Ok(report) => {
                            print!("{report}");
                            ExitCode::SUCCESS
                        }
                        Err(e) => {
                            eprintln!("Error: {e}");
                            ExitCode::FAILURE
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    ExitCode::FAILURE
                }
            },

            Commands::Meta {
                id,
                title,
                summary,
                category,
                aliases,
            } => match Workspace::load(&cli.root, &prefix) {
                Ok(mut ws) => match ws.update_meta(&id, title, summary, category, aliases) {
                    Ok(report) => {
                        println!("{report}");
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        ExitCode::FAILURE
                    }
                },
                Err(e) => {
                    eprintln!("Error: {e}");
                    ExitCode::FAILURE
                }
            },

            Commands::Remove { id, force } => match Workspace::load(&cli.root, &prefix) {
                Ok(mut ws) => match ws.remove_page(&id, force) {
                    Ok(report) => {
                        println!("{report}");
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("Error: {e}");
                        ExitCode::FAILURE
                    }
                },
                Err(e) => {
                    eprintln!("Error: {e}");
                    ExitCode::FAILURE
                }
            },

            Commands::Lint => match Workspace::load(&cli.root, &prefix) {
                Ok(ws) => {
                    let report = ws.lint();
                    if mode == RenderMode::Json {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&report).unwrap_or_default()
                        );
                    } else {
                        println!("{}", report.format_report(&prefix));
                    }
                    if report.is_clean() {
                        ExitCode::SUCCESS
                    } else {
                        ExitCode::FAILURE
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    ExitCode::FAILURE
                }
            },

            Commands::Summary { write } => match Workspace::load(&cli.root, &prefix) {
                Ok(ws) => {
                    if write {
                        match ws.write_summary_md() {
                            Ok(p) => {
                                println!("Wrote {}", p.display());
                                ExitCode::SUCCESS
                            }
                            Err(e) => {
                                eprintln!("Error: {e}");
                                ExitCode::FAILURE
                            }
                        }
                    } else {
                        print!("{}", ws.summary_md());
                        ExitCode::SUCCESS
                    }
                }
                Err(e) => {
                    eprintln!("Error: {e}");
                    ExitCode::FAILURE
                }
            },

            Commands::MdbookPreprocessor { sub } => {
                if sub.first().map(|s| s.as_str()) == Some("supports") {
                    return ExitCode::SUCCESS;
                }
                let mut raw = String::new();
                if let Err(e) = std::io::stdin().read_to_string(&mut raw) {
                    eprintln!("Failed to read stdin: {e}");
                    return ExitCode::FAILURE;
                }
                match preprocess_mdbook_json(&raw) {
                    Ok(out) => {
                        print!("{out}");
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("Preprocessor error: {e}");
                        ExitCode::FAILURE
                    }
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    native::run()
}
