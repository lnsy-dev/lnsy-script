use lnsy_script::{setup_context, setup_script_args};

use rquickjs::{Context, Module, Runtime, Value};
use rquickjs::loader::{FileResolver, ScriptLoader};
use std::io::{self, BufRead, Write};

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

fn drain_jobs(runtime: &Runtime) {
    loop {
        match runtime.execute_pending_job() {
            Ok(true) => {}
            Ok(false) => break,
            Err(_) => { eprintln!("Unhandled promise rejection"); break; }
        }
    }
}

fn print_js_error(ctx: &rquickjs::Ctx) {
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
    eprintln!("{msg}");
}

fn run_file(runtime: &Runtime, context: &Context, path: &str) {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => { eprintln!("Error reading {path}: {e}"); return; }
    };
    context.with(|ctx| {
        match Module::evaluate(ctx.clone(), path, source.as_str()) {
            Ok(_) => {}
            Err(_) => print_js_error(&ctx),
        }
    });
    drain_jobs(runtime);
    context.with(|ctx| lnsy_script::agent::poll_agents(ctx.clone())).ok();
    drain_jobs(runtime);
}

fn main() {
    let runtime = Runtime::new().expect("failed to create QuickJS runtime");

    runtime.set_loader(
        FileResolver::default().with_path("."),
        ScriptLoader::default(),
    );

    let context = Context::full(&runtime).expect("failed to create QuickJS context");

    context.with(|ctx| {
        setup_context(ctx).expect("failed to set up context");
    });

    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        if args[1] == "--agents" {
            print!("{}", include_str!("../AGENTS.md"));
            return;
        }
        let script_args = &args[2..];
        context.with(|ctx| {
            setup_script_args(ctx, script_args).expect("failed to set scriptArgs");
        });
        run_file(&runtime, &context, &args[1]);
        return;
    }

    context.with(|ctx| {
        setup_script_args(ctx, &[]).expect("failed to set scriptArgs");
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
            let has_await = source.contains("await ");
            let result = if has_await {
                let wrapped = format!(
                    "(async () => {{\n{}\n}})().catch(e => console.error(String(e)));",
                    source
                );
                ctx.eval::<Value, _>(wrapped.as_str())
            } else {
                ctx.eval::<Value, _>(source.as_str())
            };
            match result {
                Ok(_) => {}
                Err(_) => print_js_error(&ctx),
            }
        });

        drain_jobs(&runtime);

        context.with(|ctx| lnsy_script::agent::poll_agents(ctx.clone())).ok();

        drain_jobs(&runtime);
    }
}
