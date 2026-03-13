mod database;
use database::setup_database;
mod embedding_server;
use embedding_server::setup_embedding_server;
mod vector_database;
use vector_database::setup_vector_database;
mod graph_database;
use graph_database::setup_graph_database;
mod knn;
use knn::setup_knn;
mod agent;
use agent::setup_agent;

use rquickjs::{class::Trace, Class, Context, Ctx, Function, JsLifetime, Object, Runtime, Value};
use rquickjs::prelude::Rest;
use std::io::{self, BufRead, Write};
use std::sync::Arc;

fn display_value(val: Value) -> String {
    if val.is_undefined() {
        return "undefined".to_string();
    }
    if val.is_null() {
        return "null".to_string();
    }
    if let Some(b) = val.as_bool() {
        return b.to_string();
    }
    if let Some(i) = val.as_int() {
        return i.to_string();
    }
    if let Some(f) = val.as_float() {
        return f.to_string();
    }
    if let Some(s) = val.as_string() {
        if let Ok(s) = s.to_string() {
            return format!("'{s}'");
        }
    }
    "[object]".to_string()
}

fn console_format<'js>(ctx: Ctx<'js>, val: Value<'js>) -> String {
    if val.is_undefined() {
        return "undefined".to_string();
    }
    if val.is_null() {
        return "null".to_string();
    }
    if let Some(b) = val.as_bool() {
        return b.to_string();
    }
    if let Some(i) = val.as_int() {
        return i.to_string();
    }
    if let Some(f) = val.as_float() {
        return f.to_string();
    }
    if let Some(s) = val.as_string() {
        if let Ok(s) = s.to_string() {
            return s;
        }
    }
    // Try JSON.stringify for objects/arrays
    if let Ok(json) = ctx.globals().get::<_, Object>("JSON") {
        if let Ok(stringify) = json.get::<_, Function>("stringify") {
            if let Ok(result) = stringify.call::<_, Value>((val,)) {
                if let Some(s) = result.as_string() {
                    if let Ok(s) = s.to_string() {
                        return s;
                    }
                }
            }
        }
    }
    "[object Object]".to_string()
}

fn js_console_log<'js>(ctx: Ctx<'js>, args: Rest<Value<'js>>) -> rquickjs::Result<()> {
    let parts: Vec<String> = args.0.into_iter().map(|v| console_format(ctx.clone(), v)).collect();
    println!("{}", parts.join(" "));
    Ok(())
}

fn js_console_warn<'js>(ctx: Ctx<'js>, args: Rest<Value<'js>>) -> rquickjs::Result<()> {
    let parts: Vec<String> = args.0.into_iter().map(|v| console_format(ctx.clone(), v)).collect();
    eprintln!("\x1b[33m{}\x1b[0m", parts.join(" "));
    Ok(())
}

fn js_console_error<'js>(ctx: Ctx<'js>, args: Rest<Value<'js>>) -> rquickjs::Result<()> {
    let parts: Vec<String> = args.0.into_iter().map(|v| console_format(ctx.clone(), v)).collect();
    eprintln!("\x1b[31m{}\x1b[0m", parts.join(" "));
    Ok(())
}

fn setup_console<'js>(ctx: Ctx<'js>) -> rquickjs::Result<()> {
    let console = Object::new(ctx.clone())?;
    console.set("log", Function::new(ctx.clone(), js_console_log)?)?;
    console.set("warn", Function::new(ctx.clone(), js_console_warn)?)?;
    console.set("error", Function::new(ctx.clone(), js_console_error)?)?;
    ctx.globals().set("console", console)?;
    Ok(())
}

fn js_fetch_sync<'js>(ctx: Ctx<'js>, url: String, opts: Value<'js>) -> rquickjs::Result<Object<'js>> {
    let mut method = "GET".to_string();
    let mut body: Option<String> = None;
    let mut req_headers: Vec<(String, String)> = Vec::new();

    if let Some(opts_obj) = opts.as_object() {
        if let Ok(m) = opts_obj.get::<_, String>("method") {
            method = m.to_uppercase();
        }
        if let Ok(b) = opts_obj.get::<_, String>("body") {
            body = Some(b);
        }
        if let Ok(headers_obj) = opts_obj.get::<_, Object>("headers") {
            for key in headers_obj.keys::<String>() {
                if let Ok(k) = key {
                    if let Ok(v) = headers_obj.get::<_, String>(k.as_str()) {
                        req_headers.push((k, v));
                    }
                }
            }
        }
    }

    let client = reqwest::blocking::Client::new();
    let mut builder = client.request(
        reqwest::Method::from_bytes(method.as_bytes()).unwrap_or(reqwest::Method::GET),
        &url,
    );
    for (k, v) in &req_headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    if let Some(b) = body {
        builder = builder.body(b);
    }

    let response = builder.send().map_err(|e| {
        ctx.throw(rquickjs::String::from_str(ctx.clone(), &e.to_string()).unwrap().into_value())
    })?;

    let status = response.status().as_u16() as i32;
    let status_text = response.status().canonical_reason().unwrap_or("").to_string();
    let response_url = response.url().to_string();
    let body_text = response.text().map_err(|e| {
        ctx.throw(rquickjs::String::from_str(ctx.clone(), &e.to_string()).unwrap().into_value())
    })?;

    let result = Object::new(ctx.clone())?;
    result.set("status", status)?;
    result.set("statusText", status_text)?;
    result.set("url", response_url)?;
    result.set("body", body_text)?;
    Ok(result)
}

fn setup_fetch<'js>(ctx: Ctx<'js>) -> rquickjs::Result<()> {
    ctx.globals().set("__fetch_sync", Function::new(ctx.clone(), js_fetch_sync)?)?;
    ctx.eval::<(), _>(r#"
globalThis.fetch = function fetch(url, options) {
    return new Promise(function(resolve, reject) {
        var raw;
        try {
            raw = __fetch_sync(url, options === undefined ? null : options);
        } catch (e) {
            reject(e);
            return;
        }
        var response = {
            status: raw.status,
            ok: raw.status >= 200 && raw.status < 300,
            statusText: raw.statusText,
            url: raw.url,
            _body: raw.body,
            text: function() {
                var self = this;
                return new Promise(function(resolve) { resolve(self._body); });
            },
            json: function() {
                var self = this;
                return new Promise(function(resolve, reject) {
                    try { resolve(JSON.parse(self._body)); }
                    catch(e) { reject(e); }
                });
            }
        };
        resolve(response);
    });
};
"#)?;
    Ok(())
}

#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct StaticServer {
    #[qjs(skip_trace)]
    root: String,
    #[qjs(skip_trace)]
    port: u16,
}

#[rquickjs::methods]
impl StaticServer {
    #[qjs(constructor)]
    pub fn new(root: String, port: u16) -> rquickjs::Result<Self> {
        println!("StaticServer: add to /etc/hosts if not already present:");
        println!("  127.0.0.1  lnsy-static.local");
        println!("Access at: https://lnsy-static.local:{port}");
        println!("(Browser will warn about self-signed cert — proceed anyway)");
        let root_clone = root.clone();
        std::thread::spawn(move || run_static_server(root_clone, port));
        Ok(StaticServer { root, port })
    }
}

fn js_fs_read_file<'js>(ctx: Ctx<'js>, path: String) -> rquickjs::Result<String> {
    std::fs::read_to_string(&path).map_err(|e| {
        ctx.throw(rquickjs::String::from_str(ctx.clone(), &e.to_string()).unwrap().into_value())
    })
}

fn js_fs_write_file<'js>(ctx: Ctx<'js>, path: String, content: String) -> rquickjs::Result<()> {
    std::fs::write(&path, content).map_err(|e| {
        ctx.throw(rquickjs::String::from_str(ctx.clone(), &e.to_string()).unwrap().into_value())
    })
}

fn js_fs_append_file<'js>(ctx: Ctx<'js>, path: String, content: String) -> rquickjs::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| {
            ctx.throw(rquickjs::String::from_str(ctx.clone(), &e.to_string()).unwrap().into_value())
        })?;
    file.write_all(content.as_bytes()).map_err(|e| {
        ctx.throw(rquickjs::String::from_str(ctx.clone(), &e.to_string()).unwrap().into_value())
    })
}

fn js_fs_delete_file<'js>(ctx: Ctx<'js>, path: String) -> rquickjs::Result<()> {
    std::fs::remove_file(&path).map_err(|e| {
        ctx.throw(rquickjs::String::from_str(ctx.clone(), &e.to_string()).unwrap().into_value())
    })
}

fn js_fs_exists(_ctx: Ctx<'_>, path: String) -> rquickjs::Result<bool> {
    Ok(std::path::Path::new(&path).exists())
}

fn setup_fs(ctx: Ctx) -> rquickjs::Result<()> {
    let fs = Object::new(ctx.clone())?;
    fs.set("readFile", Function::new(ctx.clone(), js_fs_read_file)?)?;
    fs.set("writeFile", Function::new(ctx.clone(), js_fs_write_file)?)?;
    fs.set("appendFile", Function::new(ctx.clone(), js_fs_append_file)?)?;
    fs.set("deleteFile", Function::new(ctx.clone(), js_fs_delete_file)?)?;
    fs.set("exists", Function::new(ctx.clone(), js_fs_exists)?)?;
    ctx.globals().set("fs", fs)?;
    ctx.eval::<(), _>(r#"
fs.readDotEnv = function readDotEnv(path) {
    var raw = fs.readFile(path);
    var result = {};
    var lines = raw.split('\n');
    for (var i = 0; i < lines.length; i++) {
        var line = lines[i].trim();
        if (!line || line[0] === '#') continue;
        var eq = line.indexOf('=');
        if (eq < 1) continue;
        var key = line.slice(0, eq).trim();
        var val = line.slice(eq + 1).trim();
        if ((val[0] === '"' && val[val.length - 1] === '"') ||
            (val[0] === "'" && val[val.length - 1] === "'")) {
            val = val.slice(1, val.length - 1);
        }
        result[key] = val;
    }
    return result;
};
"#)?;
    Ok(())
}

fn setup_static_server(ctx: Ctx) -> rquickjs::Result<()> {
    Class::<StaticServer>::define(&ctx.globals())
}

fn run_static_server(root: String, port: u16) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    rt.block_on(async move {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let cert = rcgen::generate_simple_self_signed(vec![
            "lnsy-static.local".to_string(),
            "localhost".to_string(),
        ])
        .expect("cert generation");

        let cert_der = cert.cert.der().clone();
        let key_der = rustls::pki_types::PrivateKeyDer::try_from(
            cert.key_pair.serialize_der(),
        )
        .expect("key serialize");

        let tls_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert_der], key_der)
            .expect("rustls config");
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls_config));

        let app = axum::Router::new()
            .fallback_service(tower_http::services::ServeDir::new(&root));

        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");

        loop {
            let (tcp, _) = listener.accept().await.expect("accept");
            let acceptor = acceptor.clone();
            let app = app.clone();
            tokio::spawn(async move {
                if let Ok(tls) = acceptor.accept(tcp).await {
                    let io = hyper_util::rt::TokioIo::new(tls);
                    hyper_util::server::conn::auto::Builder::new(
                        hyper_util::rt::TokioExecutor::new(),
                    )
                    .serve_connection(
                        io,
                        hyper_util::service::TowerToHyperService::new(app),
                    )
                    .await
                    .ok();
                }
            });
        }
    });
}

fn needs_more_input(source: &str) -> bool {
    let mut depth: i32 = 0;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' | '\'' | '`' => {
                let quote = c;
                loop {
                    match chars.next() {
                        None => return true,
                        Some('\\') => { chars.next(); }
                        Some(ch) if ch == quote => break,
                        _ => {}
                    }
                }
            }
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth -= 1,
            _ => {}
        }
    }
    depth > 0
}

pub(crate) fn setup_context(ctx: Ctx<'_>) -> rquickjs::Result<()> {
    setup_console(ctx.clone())?;
    setup_fetch(ctx.clone())?;
    setup_fs(ctx.clone())?;
    setup_static_server(ctx.clone())?;
    setup_embedding_server(ctx.clone())?;
    setup_database(ctx.clone())?;
    setup_vector_database(ctx.clone())?;
    setup_graph_database(ctx.clone())?;
    setup_knn(ctx.clone())?;
    setup_agent(ctx)?;
    Ok(())
}

fn main() {
    let runtime = Runtime::new().expect("failed to create QuickJS runtime");
    let context = Context::full(&runtime).expect("failed to create QuickJS context");

    context.with(|ctx| {
        setup_context(ctx).expect("failed to set up context");
    });

    println!("lnsy-script 0.1.0 — type .exit or Ctrl+D to quit");

    let stdin = io::stdin();
    let mut buffer = String::new();

    loop {
        if buffer.is_empty() {
            print!("lnsy> ");
        } else {
            print!("  ... ");
        }
        io::stdout().flush().unwrap();

        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => {
                println!();
                break;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("read error: {e}");
                break;
            }
        }

        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');

        if trimmed == ".exit" {
            break;
        }

        if !buffer.is_empty() {
            buffer.push('\n');
        }
        buffer.push_str(trimmed);

        if needs_more_input(&buffer) {
            continue;
        }

        let source = buffer.clone();
        buffer.clear();

        context.with(|ctx| {
            match ctx.eval::<Value, _>(source.as_str()) {
                Ok(val) => {
                    let s = display_value(val);
                    if s != "undefined" && s != "[object]" {
                        println!("{s}");
                    }
                }
                Err(_) => {
                    let msg = ctx
                        .catch()
                        .as_exception()
                        .map(|exc| {
                            let msg = exc.message().unwrap_or_default();
                            let stack = exc.stack().unwrap_or_default();
                            if stack.is_empty() {
                                format!("Error: {msg}")
                            } else {
                                format!("Error: {msg}\n{stack}")
                            }
                        })
                        .unwrap_or_else(|| "unknown error".to_string());
                    println!("{msg}");
                }
            }
        });

        // Drain Promise microtask queue so .then() chains execute
        loop {
            match runtime.execute_pending_job() {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => { eprintln!("Unhandled promise rejection"); break; }
            }
        }

        // Poll agents for messages from worker threads
        context.with(|ctx| agent::poll_agents(ctx.clone())).ok();

        // Drain again in case onmessage handlers enqueued Promises
        loop {
            match runtime.execute_pending_job() {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => { eprintln!("Unhandled promise rejection"); break; }
            }
        }
    }
}
