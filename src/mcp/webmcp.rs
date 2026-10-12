//! WebMCP integration for browser environments (W3C Web Model Context Protocol).
//!
//! Provides in-browser tool registration via `document.modelContext` / `navigator.modelContext`,
//! and exposes a global object on `window` for extension and harness access.

use crate::mcp::types::{poll_until_settled, HandlerResult, ToolResult};
use crate::DocEngine;
use std::rc::Rc;
use std::time::Duration;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

pub struct WebMcpOptions {
    /// Name of the object placed on `window` (e.g. "__karakuri_mcp"). `None` places nothing.
    pub global_name: Option<String>,
    /// Called right after a tool call is dispatched and before waiting on its result.
    pub on_call: Option<Rc<dyn Fn()>>,
    /// How long a pending result may take before the call fails.
    pub timeout: Duration,
}

impl Default for WebMcpOptions {
    fn default() -> Self {
        Self {
            global_name: None,
            on_call: None,
            timeout: Duration::from_secs(10),
        }
    }
}

pub struct WebMcpInstalled {
    /// Whether `document.modelContext` / `navigator.modelContext` was found.
    pub model_context: bool,
    /// How many tools `registerTool` accepted.
    pub registered: usize,
    /// Tools `registerTool` refused, with the reason.
    pub refused: Vec<(String, String)>,
}

/// Yields control to the browser animation frame queue so the host render loop can advance.
async fn yield_to_browser() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(win) = web_sys::window() {
            let cb = wasm_bindgen::closure::Closure::once_into_js(move || {
                let _ = resolve.call0(&JsValue::NULL);
            });
            let _ = win.request_animation_frame(cb.unchecked_ref());
        } else {
            let _ = resolve.call0(&JsValue::NULL);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// Calls one tool and waits for its result without blocking the browser.
/// Exposed so a host's own harness can call tools the same way `execute` does.
pub async fn call_tool(
    engine: &DocEngine,
    name: &str,
    args: serde_json::Value,
    options: &WebMcpOptions,
) -> Result<ToolResult, String> {
    let res = engine.mcp_call_tool_handler(name, args);
    if let Some(on_call) = &options.on_call {
        on_call();
    }
    match res {
        HandlerResult::Ready(tool_res) => Ok(tool_res),
        HandlerResult::Pending(mut pending) => {
            let timeout = options.timeout;
            let perf = web_sys::window().and_then(|w| w.performance());
            let now = move || {
                let ms = perf.as_ref().map(|p| p.now()).unwrap_or(0.0);
                Duration::from_secs_f64(ms / 1000.0)
            };
            poll_until_settled(&mut *pending, timeout, yield_to_browser, now).await
        }
    }
}

/// Discovers WebMCP context (`document.modelContext` or `navigator.modelContext`)
/// and registers all tools from `engine.mcp_list_tools()`.
fn discover_and_register_webmcp(
    dom_window: &web_sys::Window,
    engine: &Rc<DocEngine>,
    options: &Rc<WebMcpOptions>,
) -> Result<WebMcpInstalled, JsValue> {
    let document = dom_window.document();
    let model_context_opt = document
        .as_ref()
        .and_then(|doc| js_sys::Reflect::get(doc, &"modelContext".into()).ok())
        .filter(|v| !v.is_undefined() && !v.is_null())
        .or_else(|| {
            let nav = dom_window.navigator();
            js_sys::Reflect::get(&nav, &"modelContext".into())
                .ok()
                .filter(|v| !v.is_undefined() && !v.is_null())
        });

    let Some(model_context) = model_context_opt else {
        return Ok(WebMcpInstalled {
            model_context: false,
            registered: 0,
            refused: Vec::new(),
        });
    };

    let register_fn = js_sys::Reflect::get(&model_context, &"registerTool".into())?;
    if !register_fn.is_function() {
        return Ok(WebMcpInstalled {
            model_context: false,
            registered: 0,
            refused: Vec::new(),
        });
    }

    let register_fn_cast: js_sys::Function = register_fn.unchecked_into();
    let tools = engine.mcp_list_tools();
    let mut registered = 0;
    let mut refused = Vec::new();

    for tool in tools {
        let name_str = tool.name.clone();
        let desc_str = tool.description.clone();
        let schema_json = tool.input_schema.to_string();
        let schema_js = js_sys::JSON::parse(&schema_json).unwrap_or(JsValue::NULL);

        let engine_clone = Rc::clone(engine);
        let options_clone = Rc::clone(options);
        let name_clone = name_str.clone();

        let execute_fn = Closure::wrap(Box::new(move |args: JsValue| -> js_sys::Promise {
            let engine = Rc::clone(&engine_clone);
            let options = Rc::clone(&options_clone);
            let name = name_clone.clone();

            let args_value: serde_json::Value = if args.is_null() || args.is_undefined() {
                serde_json::Value::Object(serde_json::Map::new())
            } else {
                let args_json = js_sys::JSON::stringify(&args)
                    .ok()
                    .and_then(|s| s.as_string())
                    .unwrap_or_else(|| "{}".into());
                serde_json::from_str(&args_json).unwrap_or_default()
            };

            wasm_bindgen_futures::future_to_promise(async move {
                match call_tool(&engine, &name, args_value, &options).await {
                    Ok(tool_res) => {
                        let json_str = serde_json::to_string(&tool_res)
                            .map_err(|e| JsValue::from_str(&e.to_string()))?;
                        let js_val = js_sys::JSON::parse(&json_str)
                            .unwrap_or_else(|_| JsValue::from_str(&json_str));
                        Ok(js_val)
                    }
                    Err(err) => Err(JsValue::from_str(&err)),
                }
            })
        }) as Box<dyn FnMut(JsValue) -> js_sys::Promise>);

        let tool_obj = js_sys::Object::new();
        js_sys::Reflect::set(&tool_obj, &"name".into(), &name_str.clone().into())?;
        js_sys::Reflect::set(&tool_obj, &"description".into(), &desc_str.into())?;
        js_sys::Reflect::set(&tool_obj, &"inputSchema".into(), &schema_js)?;
        js_sys::Reflect::set(&tool_obj, &"execute".into(), execute_fn.as_ref())?;
        execute_fn.forget();

        if let Err(e) = register_fn_cast.call1(&model_context, &tool_obj) {
            refused.push((name_str, format!("{e:?}")));
        } else {
            registered += 1;
        }
    }

    Ok(WebMcpInstalled {
        model_context: true,
        registered,
        refused,
    })
}

/// Places the window object and registers every tool with WebMCP when available.
pub fn install(engine: Rc<DocEngine>, options: WebMcpOptions) -> Result<WebMcpInstalled, JsValue> {
    let dom_window = web_sys::window().ok_or_else(|| JsValue::from_str("no global window"))?;
    let options = Rc::new(options);

    // 1. Expose window global object if requested
    if let Some(ref global_name) = options.global_name {
        let global_obj = js_sys::Object::new();

        // listTools()
        let engine_for_list = Rc::clone(&engine);
        let list_tools_closure = Closure::wrap(Box::new(move || -> JsValue {
            let tools = engine_for_list.mcp_list_tools();
            let json_str = serde_json::to_string(&tools).unwrap_or_default();
            js_sys::JSON::parse(&json_str).unwrap_or(JsValue::NULL)
        }) as Box<dyn FnMut() -> JsValue>);
        js_sys::Reflect::set(
            &global_obj,
            &"listTools".into(),
            list_tools_closure.as_ref(),
        )?;
        list_tools_closure.forget();

        // callTool(name, args)
        let engine_for_call = Rc::clone(&engine);
        let options_for_call = Rc::clone(&options);
        let call_tool_closure = Closure::wrap(Box::new(
            move |name: String, args: JsValue| -> js_sys::Promise {
                let engine = Rc::clone(&engine_for_call);
                let options = Rc::clone(&options_for_call);

                let args_value: serde_json::Value = if args.is_null() || args.is_undefined() {
                    serde_json::Value::Object(serde_json::Map::new())
                } else {
                    let args_json = js_sys::JSON::stringify(&args)
                        .ok()
                        .and_then(|s| s.as_string())
                        .unwrap_or_else(|| "{}".into());
                    serde_json::from_str(&args_json).unwrap_or_default()
                };

                wasm_bindgen_futures::future_to_promise(async move {
                    match call_tool(&engine, &name, args_value, &options).await {
                        Ok(tool_res) => {
                            let json_str = serde_json::to_string(&tool_res)
                                .map_err(|e| JsValue::from_str(&e.to_string()))?;
                            let js_val = js_sys::JSON::parse(&json_str)
                                .unwrap_or_else(|_| JsValue::from_str(&json_str));
                            Ok(js_val)
                        }
                        Err(err) => Err(JsValue::from_str(&err)),
                    }
                })
            },
        )
            as Box<dyn FnMut(String, JsValue) -> js_sys::Promise>);
        js_sys::Reflect::set(&global_obj, &"callTool".into(), call_tool_closure.as_ref())?;
        call_tool_closure.forget();

        // listResources()
        let engine_for_res = Rc::clone(&engine);
        let list_res_closure = Closure::wrap(Box::new(move || -> JsValue {
            let res = engine_for_res.mcp_list_resources();
            let json_str = serde_json::to_string(&res).unwrap_or_default();
            js_sys::JSON::parse(&json_str).unwrap_or(JsValue::NULL)
        }) as Box<dyn FnMut() -> JsValue>);
        js_sys::Reflect::set(
            &global_obj,
            &"listResources".into(),
            list_res_closure.as_ref(),
        )?;
        list_res_closure.forget();

        // readResource(uri)
        let engine_for_read = Rc::clone(&engine);
        let read_res_closure = Closure::wrap(Box::new(move |uri: String| -> JsValue {
            match engine_for_read.mcp_read_resource(&uri) {
                Ok(content) => {
                    let res = serde_json::json!({
                        "contents": [content]
                    });
                    let json_str = res.to_string();
                    js_sys::JSON::parse(&json_str).unwrap_or_else(|_| JsValue::from_str(&json_str))
                }
                Err(err) => JsValue::from_str(&err.to_string()),
            }
        }) as Box<dyn FnMut(String) -> JsValue>);
        js_sys::Reflect::set(
            &global_obj,
            &"readResource".into(),
            read_res_closure.as_ref(),
        )?;
        read_res_closure.forget();

        js_sys::Reflect::set(&dom_window, &global_name.into(), &global_obj)?;
    }

    // 2. Discover and register with WebMCP context
    discover_and_register_webmcp(&dom_window, &engine, &options)
}
