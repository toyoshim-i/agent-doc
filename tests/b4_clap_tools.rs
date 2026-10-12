#[cfg(all(feature = "mcp", feature = "clap"))]
#[test]
fn test_b4_clap_tools_acceptance() {
    use agent_doc::DocEngine;
    use clap::{Arg, ArgAction, Command};
    use serde_json::json;

    let mut engine = DocEngine::new("kinema").with_scheme("kinema");

    // Construct a sample clap CLI command with subcommands
    let cli = Command::new("kinema")
        .subcommand(
            Command::new("check")
                .about("Check schematics and board layout equivalence")
                .arg(
                    Arg::new("schematic")
                        .long("schematic")
                        .required(true)
                        .help("Path to verilog schematic"),
                )
                .arg(
                    Arg::new("strict")
                        .long("strict")
                        .action(ArgAction::SetTrue)
                        .help("Enable strict DRC checking"),
                )
                .arg(
                    Arg::new("format")
                        .long("format")
                        .value_parser(["json", "text"])
                        .help("Output format"),
                )
                .arg(
                    Arg::new("files")
                        .action(ArgAction::Append)
                        .help("Verilog files to check"),
                ),
        )
        .subcommand(Command::new("guide").about("Internal guide command - should be excluded"))
        .subcommand(Command::new("explain").about("Internal explain command - should be excluded"));

    // Register clap subcommands excluding guide and explain (subcommand ArgMatches)
    engine
        .register_clap_subcommand_tools(cli, &["guide", "explain"], |subcmd, matches| {
            let schem: &String = matches.get_one("schematic").unwrap();
            let strict = matches.get_flag("strict");
            let files: Vec<String> = matches
                .get_many::<String>("files")
                .map(|vals| vals.cloned().collect())
                .unwrap_or_default();
            agent_doc::mcp::ToolResult::text(format!(
                "Ran {subcmd} on {schem} (strict: {strict}, files: {files:?})"
            ))
            .into()
        })
        .unwrap();

    // 1. tools/list: "check" is registered, while "guide" and "explain" are excluded
    let tools = engine.mcp_list_tools();
    assert!(tools.iter().any(|t| t.name == "check"));
    assert!(!tools.iter().any(|t| t.name == "guide"));
    assert!(!tools.iter().any(|t| t.name == "explain"));

    let check_tool = tools.iter().find(|t| t.name == "check").unwrap();
    assert_eq!(
        check_tool.input_schema["properties"]["schematic"]["type"],
        "string"
    );
    assert_eq!(
        check_tool.input_schema["properties"]["strict"]["type"],
        "boolean"
    );
    assert_eq!(
        check_tool.input_schema["properties"]["files"]["type"],
        "array"
    );
    assert_eq!(
        check_tool.input_schema["properties"]["format"]["enum"],
        json!(["json", "text"])
    );

    // 2. Acceptance condition: Invocation passes through clap try_get_matches_from
    // Positional arguments (files) are passed without '--files' flag
    let valid_call = engine
        .mcp_call_tool(
            "check",
            json!({
                "schematic": "board.v",
                "strict": true,
                "files": ["examples/timer_core.v", "examples/sub.v"]
            }),
        )
        .unwrap();
    assert!(!valid_call.is_error);
    assert!(valid_call.content[0]
        .text
        .contains("Ran check on board.v (strict: true, files: [\"examples/timer_core.v\", \"examples/sub.v\"])"));

    // 3. Acceptance condition: Clap parser rejects invalid input identically to CLI
    let invalid_call = engine
        .mcp_call_tool(
            "check",
            json!({
                "schematic": "board.v",
                "format": "invalid_format"
            }),
        )
        .unwrap();
    assert!(invalid_call.is_error);
    assert!(invalid_call.content[0]
        .text
        .contains("invalid value 'invalid_format'"));

    // 4. Acceptance condition (Item 5 & 6): derive-based CLI from_arg_matches works on root ArgMatches
    // Also tests SetFalse (cache=false) and hyphenated positional arguments (files=["-", "-v"])
    use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};

    #[derive(Parser, Debug, PartialEq)]
    #[command(name = "mycli")]
    struct DeriveCli {
        #[command(subcommand)]
        command: DeriveCommands,
    }

    #[derive(Subcommand, Debug, PartialEq, Clone)]
    enum DeriveCommands {
        Lint {
            #[arg(long)]
            deny_warnings: bool,
            #[arg(long, default_value_t = true, action = ArgAction::SetFalse)]
            cache: bool,
            files: Vec<String>,
        },
    }

    let mut derive_engine = DocEngine::new("mycli");
    derive_engine
        .register_clap_tools(DeriveCli::command(), &[], |_subcmd, matches| {
            let cli = DeriveCli::from_arg_matches(&matches).expect("from_arg_matches failed");
            match cli.command {
                DeriveCommands::Lint {
                    deny_warnings,
                    cache,
                    files,
                } => agent_doc::mcp::ToolResult::text(format!(
                    "Linted: deny={deny_warnings}, cache={cache}, files={files:?}"
                ))
                .into(),
            }
        })
        .unwrap();

    // Call lint with deny_warnings=true, cache=false, files=["-", "-v"]
    let derive_call = derive_engine
        .mcp_call_tool(
            "lint",
            json!({
                "deny_warnings": true,
                "cache": false,
                "files": ["-", "-v"]
            }),
        )
        .unwrap();
    assert!(
        !derive_call.is_error,
        "Derive call error: {:?}",
        derive_call
    );
    assert_eq!(
        derive_call.content[0].text,
        "Linted: deny=true, cache=false, files=[\"-\", \"-v\"]"
    );
}
