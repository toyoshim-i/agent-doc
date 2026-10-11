use crate::mcp::tool::{ParamType, Tool};
use crate::mcp::types::{HandlerResult, ToolResult};
use clap::{ArgAction, Command};
use std::sync::Arc;

pub fn tools_from_clap<F>(cmd: Command, exclude: &[&str], handler: F) -> Vec<Tool>
where
    F: Fn(&str, clap::ArgMatches) -> HandlerResult<ToolResult> + Send + Sync + 'static,
{
    let handler = Arc::new(handler);
    let mut tools = Vec::new();

    for subcmd in cmd.get_subcommands() {
        let name = subcmd.get_name();
        if exclude.contains(&name) {
            continue;
        }

        let description = subcmd
            .get_about()
            .map(|a| a.to_string())
            .unwrap_or_default();

        let mut builder = Tool::builder(name, description);

        for arg in subcmd.get_arguments() {
            let arg_id = arg.get_id().as_str();
            if arg_id == "help" || arg_id == "version" {
                continue;
            }

            let param_name = arg.get_long().unwrap_or(arg_id).to_string();
            let desc = arg.get_help().map(|h| h.to_string()).unwrap_or_default();
            let required = arg.is_required_set();

            let param_type = if matches!(arg.get_action(), ArgAction::SetTrue | ArgAction::SetFalse)
            {
                ParamType::Boolean
            } else {
                let possible: Vec<String> = arg
                    .get_possible_values()
                    .into_iter()
                    .map(|v| v.get_name().to_string())
                    .collect();

                if !possible.is_empty() {
                    ParamType::Enum(possible)
                } else if arg.get_num_args().is_some_and(|r| r.max_values() > 1)
                    || matches!(arg.get_action(), ArgAction::Append)
                {
                    ParamType::StringArray
                } else {
                    ParamType::String
                }
            };

            builder = builder.param(param_name, param_type, desc, required);
        }

        let subcmd_parser = subcmd.clone();
        let subcmd_name = name.to_string();
        let h = Arc::clone(&handler);

        let tool = builder
            .skip_validation(true)
            .handler(move |args| {
                let mut argv = vec![subcmd_name.clone()];

                if let Some(obj) = args.as_object() {
                    for (key, val) in obj {
                        match val {
                            serde_json::Value::Bool(true) => {
                                argv.push(format!("--{key}"));
                            }
                            serde_json::Value::Bool(false) => {}
                            serde_json::Value::Array(items) => {
                                for item in items {
                                    argv.push(format!("--{key}"));
                                    if let Some(s) = item.as_str() {
                                        argv.push(s.to_string());
                                    } else {
                                        argv.push(item.to_string());
                                    }
                                }
                            }
                            serde_json::Value::String(s) => {
                                argv.push(format!("--{key}"));
                                argv.push(s.clone());
                            }
                            serde_json::Value::Number(n) => {
                                argv.push(format!("--{key}"));
                                argv.push(n.to_string());
                            }
                            _ => {}
                        }
                    }
                }

                match subcmd_parser.clone().try_get_matches_from(argv) {
                    Ok(matches) => h(&subcmd_name, matches),
                    Err(err) => ToolResult::error(err.to_string()).into(),
                }
            })
            .build();

        tools.push(tool);
    }

    tools
}
