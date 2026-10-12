use crate::mcp::tool::{ParamType, Tool};
use crate::mcp::types::{HandlerResult, ToolResult};
use clap::{ArgAction, Command};
use std::collections::HashMap;
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
        let mut positional_order = Vec::new();
        let mut flag_map = HashMap::new();

        for arg in subcmd.get_arguments() {
            let arg_id = arg.get_id().as_str();
            if arg_id == "help" || arg_id == "version" {
                continue;
            }

            let param_name = if let Some(long) = arg.get_long() {
                flag_map.insert(long.to_string(), format!("--{long}"));
                long.to_string()
            } else if let Some(short) = arg.get_short() {
                flag_map.insert(arg_id.to_string(), format!("-{short}"));
                arg_id.to_string()
            } else {
                positional_order.push(arg_id.to_string());
                arg_id.to_string()
            };

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
                let mut options_argv = Vec::new();
                let mut positionals_argv = Vec::new();

                if let Some(obj) = args.as_object() {
                    // 1. Process options (non-positional arguments)
                    for (key, val) in obj {
                        if positional_order.contains(key) {
                            continue;
                        }
                        let flag = flag_map
                            .get(key)
                            .cloned()
                            .unwrap_or_else(|| format!("--{key}"));

                        match val {
                            serde_json::Value::Bool(true) => {
                                options_argv.push(flag);
                            }
                            serde_json::Value::Bool(false) => {}
                            serde_json::Value::Array(items) => {
                                for item in items {
                                    options_argv.push(flag.clone());
                                    if let Some(s) = item.as_str() {
                                        options_argv.push(s.to_string());
                                    } else {
                                        options_argv.push(item.to_string());
                                    }
                                }
                            }
                            serde_json::Value::String(s) => {
                                options_argv.push(flag);
                                options_argv.push(s.clone());
                            }
                            serde_json::Value::Number(n) => {
                                options_argv.push(flag);
                                options_argv.push(n.to_string());
                            }
                            _ => {}
                        }
                    }

                    // 2. Process positional arguments in definition order
                    for pos_key in &positional_order {
                        if let Some(val) = obj.get(pos_key) {
                            match val {
                                serde_json::Value::Array(items) => {
                                    for item in items {
                                        if let Some(s) = item.as_str() {
                                            positionals_argv.push(s.to_string());
                                        } else {
                                            positionals_argv.push(item.to_string());
                                        }
                                    }
                                }
                                serde_json::Value::String(s) => {
                                    positionals_argv.push(s.clone());
                                }
                                serde_json::Value::Number(n) => {
                                    positionals_argv.push(n.to_string());
                                }
                                serde_json::Value::Bool(b) => {
                                    positionals_argv.push(b.to_string());
                                }
                                _ => {}
                            }
                        }
                    }
                }

                let mut argv = vec![subcmd_name.clone()];
                argv.extend(options_argv);
                argv.extend(positionals_argv);

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
