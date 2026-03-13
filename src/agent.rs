use rquickjs::{class::Trace, Class, Context, Ctx, Function, JsLifetime, Object, Runtime, Value};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::sync::mpsc::{Receiver, SyncSender};

static AGENT_REGISTRY: OnceLock<Mutex<Vec<AgentHandle>>> = OnceLock::new();

pub fn get_registry() -> &'static Mutex<Vec<AgentHandle>> {
    AGENT_REGISTRY.get_or_init(|| Mutex::new(Vec::new()))
}

pub struct AgentHandle {
    pub id: u64,
    pub to_worker: SyncSender<String>,
    pub from_worker: Receiver<String>,
}

#[derive(Trace, JsLifetime)]
#[rquickjs::class]
pub struct Agent {
    #[qjs(skip_trace)]
    id: u64,
}

#[rquickjs::methods]
impl Agent {
    #[qjs(constructor)]
    pub fn new(ctx: Ctx<'_>, script_path: String) -> rquickjs::Result<Self> {
        if !std::path::Path::new(&script_path).exists() {
            return Err(ctx.throw(
                rquickjs::String::from_str(ctx.clone(), &format!("Agent script not found: {script_path}"))
                    .unwrap()
                    .into_value(),
            ));
        }

        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);

        let (to_worker_tx, to_worker_rx) = std::sync::mpsc::sync_channel::<String>(64);
        let (from_worker_tx, from_worker_rx) = std::sync::mpsc::sync_channel::<String>(64);

        std::thread::spawn(move || {
            run_worker(script_path, to_worker_rx, from_worker_tx);
        });

        get_registry().lock().unwrap().push(AgentHandle {
            id,
            to_worker: to_worker_tx,
            from_worker: from_worker_rx,
        });

        Ok(Agent { id })
    }

    #[qjs(rename = "__id", get)]
    pub fn agent_id(&self) -> u64 {
        self.id
    }

    #[qjs(rename = "__postMessageRaw")]
    pub fn post_message_raw(&self, json: String) -> rquickjs::Result<()> {
        let registry = get_registry().lock().unwrap();
        if let Some(handle) = registry.iter().find(|h| h.id == self.id) {
            handle.to_worker.send(json).ok();
        }
        Ok(())
    }
}

pub fn poll_agents(ctx: Ctx<'_>) -> rquickjs::Result<()> {
    // Collect pending messages WITHOUT holding the lock while calling JS callbacks
    let pending: Vec<(u64, Vec<String>)> = {
        let mut registry = get_registry().lock().unwrap();
        registry
            .iter_mut()
            .map(|h| {
                let msgs: Vec<String> =
                    std::iter::from_fn(|| h.from_worker.try_recv().ok()).collect();
                (h.id, msgs)
            })
            .collect()
    };

    // If no messages found, sleep briefly to let worker threads run
    if pending.iter().all(|(_, msgs)| msgs.is_empty()) {
        std::thread::sleep(std::time::Duration::from_millis(1));
        return Ok(());
    }

    for (agent_id, messages) in pending {
        for json_msg in messages {
            let key = format!("__agent_cb_{agent_id}");
            let cb: Value = ctx.globals().get(key.as_str())?;
            if let Some(f) = cb.as_function() {
                let event = Object::new(ctx.clone())?;
                let json_parse: Function = ctx
                    .globals()
                    .get::<_, Object>("JSON")?
                    .get("parse")?;
                let parsed: Value = json_parse.call((json_msg,))?;
                event.set("data", parsed)?;
                f.call::<_, ()>((event,))?;
            }
        }
    }
    Ok(())
}

pub fn setup_agent(ctx: Ctx<'_>) -> rquickjs::Result<()> {
    Class::<Agent>::define(&ctx.globals())?;

    ctx.eval::<(), _>(
        r#"
Agent.prototype.postMessage = function(data) {
    this.__postMessageRaw(JSON.stringify(data));
};
Object.defineProperty(Agent.prototype, 'onmessage', {
    set: function(fn) { globalThis['__agent_cb_' + this.__id] = fn; },
    get: function() { return globalThis['__agent_cb_' + this.__id]; }
});
"#,
    )?;

    ctx.globals().set(
        "__agentPoll",
        Function::new(ctx.clone(), |ctx: Ctx<'_>| poll_agents(ctx))?,
    )?;

    Ok(())
}

fn run_worker(
    script_path: String,
    inbox: Receiver<String>,
    outbox: SyncSender<String>,
) {
    let runtime = Runtime::new().expect("worker runtime");
    let context = Context::full(&runtime).expect("worker context");

    let init_ok = context.with(|ctx| {
        // Full API setup — same as main thread
        crate::setup_context(ctx.clone())?;

        // Set up self global
        let self_obj = Object::new(ctx.clone())?;
        let outbox_clone = outbox.clone();
        self_obj.set(
            "__postMessageRaw",
            Function::new(ctx.clone(), move |json: String| {
                outbox_clone.send(json).ok();
                Ok::<(), rquickjs::Error>(())
            })?,
        )?;
        self_obj.set("onmessage", Value::new_undefined(ctx.clone()))?;
        ctx.globals().set("self", self_obj)?;

        // Inject self.postMessage shim
        ctx.eval::<(), _>(
            "self.postMessage = function(data) { self.__postMessageRaw(JSON.stringify(data)); };",
        )?;

        // Execute the worker script
        let source = std::fs::read_to_string(&script_path)
            .map_err(|e| ctx.throw(
                rquickjs::String::from_str(ctx.clone(), &e.to_string()).unwrap().into_value(),
            ))?;
        ctx.eval::<(), _>(source.as_str())?;

        Ok::<(), rquickjs::Error>(())
    });

    if init_ok.is_err() {
        return;
    }

    // Message dispatch loop
    loop {
        match inbox.recv() {
            Ok(json_msg) => {
                context
                    .with(|ctx| {
                        let self_obj: Object = ctx.globals().get("self")?;
                        let onmessage: Value = self_obj.get("onmessage")?;
                        if let Some(f) = onmessage.as_function() {
                            let event = Object::new(ctx.clone())?;
                            let json_parse: Function = ctx
                                .globals()
                                .get::<_, Object>("JSON")?
                                .get("parse")?;
                            let parsed: Value = json_parse.call((json_msg,))?;
                            event.set("data", parsed)?;
                            f.call::<_, ()>((event,))?;
                        }
                        Ok::<(), rquickjs::Error>(())
                    })
                    .ok();

                // Drain microtasks after each message dispatch
                loop {
                    match runtime.execute_pending_job() {
                        Ok(true) => {}
                        _ => break,
                    }
                }
            }
            Err(_) => break,
        }
    }
}
