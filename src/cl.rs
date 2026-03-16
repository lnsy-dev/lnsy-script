use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rquickjs::{Ctx, Function, Object, Value};

enum Msg {
    Chunk {
        stream: &'static str,
        data: Vec<u8>,
        elapsed_ms: u64,
    },
    Done,
}

fn throw_str<'js>(ctx: &Ctx<'js>, msg: &str) -> rquickjs::Error {
    ctx.throw(
        rquickjs::String::from_str(ctx.clone(), msg)
            .unwrap()
            .into_value(),
    )
}

fn build_result<'js>(
    ctx: Ctx<'js>,
    stdout: &str,
    stderr: &str,
    code: i32,
    duration_ms: u64,
) -> rquickjs::Result<Object<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set("stdout", stdout)?;
    obj.set("stderr", stderr)?;
    obj.set("code", code)?;
    obj.set("success", code == 0)?;
    obj.set("duration", duration_ms as f64)?;
    Ok(obj)
}

pub fn js_cl_sync<'js>(
    ctx: Ctx<'js>,
    command: String,
    opts: Value<'js>,
    on_status: Value<'js>,
) -> rquickjs::Result<Object<'js>> {
    // ── parse options ─────────────────────────────────────────────────────────
    let mut cwd: Option<String> = None;
    let mut timeout_ms: Option<u64> = None;
    let mut shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let mut stdin_str: Option<String> = None;
    let mut extra_env: Vec<(String, String)> = Vec::new();

    if let Some(obj) = opts.as_object() {
        if let Ok(v) = obj.get::<_, String>("cwd") {
            cwd = Some(v);
        }
        if let Ok(v) = obj.get::<_, f64>("timeout") {
            timeout_ms = Some(v as u64);
        }
        if let Ok(v) = obj.get::<_, String>("shell") {
            shell = v;
        }
        if let Ok(v) = obj.get::<_, String>("stdin") {
            stdin_str = Some(v);
        }
        if let Ok(env_obj) = obj.get::<_, Object>("env") {
            for k in env_obj.keys::<String>() {
                if let Ok(k) = k {
                    if let Ok(v) = env_obj.get::<_, String>(k.as_str()) {
                        extra_env.push((k, v));
                    }
                }
            }
        }
    }

    let start = Instant::now();

    // ── spawn process ─────────────────────────────────────────────────────────
    let mut cmd = Command::new(&shell);
    cmd.arg("-c").arg(&command);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    if let Some(ref dir) = cwd {
        cmd.current_dir(dir);
    }
    for (k, v) in &extra_env {
        cmd.env(k, v);
    }
    if stdin_str.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }

    let mut child = cmd.spawn().map_err(|e| throw_str(&ctx, &e.to_string()))?;

    // Write stdin and close the pipe so the process doesn't block waiting for input
    if let (Some(data), Some(mut handle)) = (stdin_str, child.stdin.take()) {
        use std::io::Write;
        let _ = handle.write_all(data.as_bytes());
        // handle dropped here → EOF sent to child
    }

    // ── reader threads ────────────────────────────────────────────────────────
    let stdout_pipe = child.stdout.take().unwrap();
    let stderr_pipe = child.stderr.take().unwrap();

    let (tx, rx) = mpsc::channel::<Msg>();

    let tx_out = tx.clone();
    std::thread::spawn(move || {
        let mut pipe = stdout_pipe;
        let mut buf = [0u8; 8192];
        loop {
            match pipe.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let elapsed_ms = start.elapsed().as_millis() as u64;
                    tx_out
                        .send(Msg::Chunk {
                            stream: "stdout",
                            data: buf[..n].to_vec(),
                            elapsed_ms,
                        })
                        .ok();
                }
            }
        }
        tx_out.send(Msg::Done).ok();
    });

    let tx_err = tx;
    std::thread::spawn(move || {
        let mut pipe = stderr_pipe;
        let mut buf = [0u8; 8192];
        loop {
            match pipe.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let elapsed_ms = start.elapsed().as_millis() as u64;
                    tx_err
                        .send(Msg::Chunk {
                            stream: "stderr",
                            data: buf[..n].to_vec(),
                            elapsed_ms,
                        })
                        .ok();
                }
            }
        }
        tx_err.send(Msg::Done).ok();
    });

    // ── collect chunks, invoke onStatus ──────────────────────────────────────
    let kill_flag = Arc::new(AtomicBool::new(false));
    let has_on_status = on_status.is_function();

    // Create the kill function once so we can reuse it across chunks
    let kill_fn: Option<Function<'js>> = if has_on_status {
        let kf = kill_flag.clone();
        Some(Function::new(ctx.clone(), move || {
            kf.store(true, Ordering::SeqCst);
            Ok::<(), rquickjs::Error>(())
        })?)
    } else {
        None
    };

    let mut stdout_bytes: Vec<u8> = Vec::new();
    let mut stderr_bytes: Vec<u8> = Vec::new();
    let mut streams_done: u32 = 0;
    let mut timed_out = false;

    'drain: while streams_done < 2 {
        // Compute receive timeout: poll every 50 ms, but cap at remaining budget
        let wait = if let Some(ms) = timeout_ms {
            let elapsed = start.elapsed().as_millis() as u64;
            if elapsed >= ms {
                timed_out = true;
                let _ = child.kill();
                // Drain any buffered chunks so we report everything captured so far
                loop {
                    match rx.recv_timeout(Duration::from_millis(200)) {
                        Ok(Msg::Done) => {
                            streams_done += 1;
                            if streams_done >= 2 {
                                break;
                            }
                        }
                        Ok(Msg::Chunk { stream, data, .. }) => {
                            if stream == "stdout" {
                                stdout_bytes.extend_from_slice(&data);
                            } else {
                                stderr_bytes.extend_from_slice(&data);
                            }
                        }
                        Err(_) => break,
                    }
                }
                break 'drain;
            }
            Duration::from_millis((ms - elapsed).min(50))
        } else {
            Duration::from_millis(50)
        };

        match rx.recv_timeout(wait) {
            Ok(Msg::Done) => {
                streams_done += 1;
            }
            Ok(Msg::Chunk {
                stream,
                data,
                elapsed_ms,
            }) => {
                let chunk_str = String::from_utf8_lossy(&data).to_string();
                if stream == "stdout" {
                    stdout_bytes.extend_from_slice(&data);
                } else {
                    stderr_bytes.extend_from_slice(&data);
                }

                if let (true, Some(f), Some(kf)) =
                    (has_on_status, on_status.as_function(), &kill_fn)
                {
                    let status_obj = Object::new(ctx.clone())?;
                    status_obj.set("stream", stream)?;
                    status_obj.set("chunk", chunk_str)?;
                    status_obj.set("elapsed", elapsed_ms as f64)?;
                    status_obj.set("kill", kf.clone())?;
                    let _ = f.call::<_, Value>((status_obj,));

                    if kill_flag.load(Ordering::SeqCst) {
                        let _ = child.kill();
                        // Drain remaining buffered chunks
                        loop {
                            match rx.recv_timeout(Duration::from_millis(200)) {
                                Ok(Msg::Done) => {
                                    streams_done += 1;
                                    if streams_done >= 2 {
                                        break;
                                    }
                                }
                                Ok(Msg::Chunk {
                                    stream: s, data: d, ..
                                }) => {
                                    if s == "stdout" {
                                        stdout_bytes.extend_from_slice(&d);
                                    } else {
                                        stderr_bytes.extend_from_slice(&d);
                                    }
                                }
                                Err(_) => break,
                            }
                        }
                        break 'drain;
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // just loop back and re-check the timeout budget
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // both reader threads finished without sending Done — shouldn't
                // happen, but break safely
                break;
            }
        }
    }

    let exit_code = child
        .wait()
        .map(|s| s.code().unwrap_or(-1))
        .unwrap_or(-1);
    let duration_ms = start.elapsed().as_millis() as u64;

    let stdout_str = String::from_utf8_lossy(&stdout_bytes).to_string();
    let stderr_str = String::from_utf8_lossy(&stderr_bytes).to_string();

    // Timeout: store partial result in a well-known global, then throw a
    // sentinel string that the JS wrapper converts into a proper Error
    if timed_out {
        let partial = build_result(
            ctx.clone(),
            &stdout_str,
            &stderr_str,
            exit_code,
            duration_ms,
        )?;
        ctx.globals().set("__cl_timeout_result", partial)?;
        return Err(throw_str(
            &ctx,
            &format!(
                "__cl_timeout__:command timed out after {}ms",
                timeout_ms.unwrap()
            ),
        ));
    }

    // Command not found (exit 127) — reject with the shell's error message
    if exit_code == 127 {
        let raw = stderr_str.trim().to_string();
        let msg = if raw.is_empty() {
            format!("command not found: {command}")
        } else {
            raw
        };
        return Err(throw_str(&ctx, &format!("__cl_notfound__:{msg}")));
    }

    build_result(ctx, &stdout_str, &stderr_str, exit_code, duration_ms)
}

pub fn setup_cl(ctx: Ctx<'_>) -> rquickjs::Result<()> {
    ctx.globals().set(
        "__cl_sync",
        Function::new(ctx.clone(), js_cl_sync)?,
    )?;

    ctx.eval::<(), _>(
        r#"
globalThis.cl = function cl(command, opts) {
    return new Promise(function(resolve, reject) {
        var onStatus = opts && typeof opts.onStatus === 'function' ? opts.onStatus : null;

        // Strip onStatus from the options passed to Rust (it handles it separately)
        var rustOpts = null;
        if (opts) {
            rustOpts = {};
            if (opts.cwd !== undefined)     rustOpts.cwd     = opts.cwd;
            if (opts.env !== undefined)     rustOpts.env     = opts.env;
            if (opts.timeout !== undefined) rustOpts.timeout = opts.timeout;
            if (opts.shell !== undefined)   rustOpts.shell   = opts.shell;
            if (opts.stdin !== undefined)   rustOpts.stdin   = opts.stdin;
        }

        var raw;
        try {
            raw = __cl_sync(command, rustOpts, onStatus);
        } catch (e) {
            var msg = typeof e === 'string' ? e : String(e);
            if (msg.indexOf('__cl_timeout__:') === 0) {
                var err = new Error(msg.slice('__cl_timeout__:'.length));
                err.result = globalThis.__cl_timeout_result;
                delete globalThis.__cl_timeout_result;
                reject(err);
            } else if (msg.indexOf('__cl_notfound__:') === 0) {
                reject(new Error(msg.slice('__cl_notfound__:'.length)));
            } else {
                reject(new Error(msg));
            }
            return;
        }
        resolve(raw);
    });
};
"#,
    )?;
    Ok(())
}
