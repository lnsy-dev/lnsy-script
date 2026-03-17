# lnsy-script Agent Guide

lnsy-script is a JavaScript runtime built in Rust using QuickJS. It embeds ML/AI capabilities (embeddings, QA, sentiment, NER), multiple database backends (SQLite, vector, graph), a tool registry for LLM interop, and a worker thread system — all accessible from JavaScript with no npm or node_modules required.

## Running Scripts

```bash
./lnsy-script              # interactive REPL
./lnsy-script script.js    # run a script file (evaluated as an ES6 module)
```

## Core Rules

- **`async/await` is supported**. The REPL wraps each input in an async IIFE, so top-level `await` works. Script files are evaluated as ES6 modules, so top-level `await` works there too.
- **ES6 modules are supported in files**. Use `import`/`export` in `.js` files. Static `import` declarations require module context (i.e., files run via `lnsy-script script.js`). In the REPL, use dynamic `import()` instead.
- **No `require`**. There is no CommonJS module system.
- **All async APIs return Promises**. Use `await` or `.then().catch()` chains.
- **REPL variable scoping**: The REPL wraps each command in an async IIFE. `var`, `let`, and `const` declarations do not persist across REPL commands. Use `globalThis.x = value` to persist values between commands.
- Scripts run to completion; the runtime drains the microtask queue after each command.

---

## Global APIs

### `console`

```javascript
console.log("message");
console.warn("warning");
console.error("error");
```

---

### `fetch`

Promise-based HTTP client. Supports `await`.

```javascript
const res = await fetch("https://example.com/api", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ key: "value" })
});
const data = await res.json();
console.log(data);
```

Response methods: `.json()`, `.text()`, `.status` (number).

---

### `fs`

Synchronous file system operations, Promise-wrapped.

```javascript
fs.writeFile("output.txt", "hello world")
  .then(function() { return fs.readFile("output.txt"); })
  .then(function(contents) { console.log(contents); });

fs.appendFile("log.txt", "new line\n");
fs.deleteFile("temp.txt");
fs.exists("config.json").then(function(exists) { console.log(exists); });

// Parse a .env file into an object
fs.readDotEnv(".env").then(function(env) {
  console.log(env.API_KEY);
});
```

---

### `cl`

Run shell commands. Returns a Promise resolving with `{ stdout, stderr, code, success, duration }`.

```javascript
// Basic usage
const result = await cl("git log --oneline -5");
console.log(result.stdout);
console.log(result.success);  // true if exit code 0

// With options
const r = await cl("npm test", {
  cwd: "/path/to/project",
  timeout: 30000,
  env: { NODE_ENV: "test" }
});

// Stream output as it arrives
await cl("make build", {
  onStatus: function(s) {
    // s.stream  → "stdout" or "stderr"
    // s.chunk   → string data for this chunk
    // s.elapsed → milliseconds since start
    // s.kill()  → terminate the process early
    console.log(s.chunk);
  }
});

// Pipe data to stdin
const r = await cl("cat", { stdin: "hello world" });
console.log(r.stdout);  // "hello world"
```

**Error handling:**

```javascript
// Non-zero exit — resolves (does not reject); check result.code
const r = await cl("exit 1");
console.log(r.code);     // 1
console.log(r.success);  // false

// Command not found — rejects
cl("nonexistent").catch(e => console.error(e.message));

// Timeout — rejects; err.result has partial output captured so far
cl("sleep 60", { timeout: 500 }).catch(e => {
  console.log(e.result.stdout);
});
```

---

### `Tools`

Registers JavaScript functions as LLM-compatible tools. Accepts OpenAI-style `tool_calls` responses directly.

**Defining tools:**

```javascript
var toolbox = new Tools([
  {
    name: "get_weather",
    description: "Get current weather for a city",
    parameters: {
      type: "object",
      properties: {
        city: { type: "string", description: "City name" }
      },
      required: ["city"]
    },
    handler: function(args) {
      return fetch("https://wttr.in/" + args.city + "?format=j1")
        .then(function(r) { return r.json(); });
    }
  }
]);
```

**Calling tools:**

```javascript
// Direct call with arguments object
toolbox.call("get_weather", { city: "Portland" })
  .then(function(result) { console.log(result); });

// Pass an LLM tool_call object directly
toolbox.call({
  id: "call_abc123",
  type: "function",
  function: { name: "get_weather", arguments: '{"city":"Portland"}' }
}).then(function(result) { console.log(result); });

// Execute all tool_calls from an LLM response at once
toolbox.callMany(response.tool_calls)
  .then(function(results) { console.log(results); });
```

**Introspection:**

```javascript
toolbox.listTools();        // returns OpenAI-compatible schema array
toolbox.hasTool("name");    // boolean
toolbox.removeTool("name"); // deregister a tool
```

**Errors:**
- `ToolNotFoundError` — unregistered tool name
- `ToolValidationError` — missing required parameters or malformed JSON arguments
- `ToolRegistrationError` — duplicate tool name or invalid definition
- Handler exceptions return `{ error: true, result: "message" }` (do not throw)
- Extra parameters beyond `required` are stripped silently before calling the handler

---

### `Agent`

Worker threads. Each worker runs a separate `.js` file in its own OS thread with full API access.

**Main thread:**

```javascript
var worker = new Agent("worker.js");

worker.onmessage = function(event) {
  console.log("Received:", event.data);
};

worker.postMessage({ task: "process", input: "data" });

// Must call __agentPoll() to flush the message queue in the REPL or loops
__agentPoll();
```

**Worker file (`worker.js`):**

```javascript
onmessage = function(event) {
  var result = doWork(event.data);
  postMessage(result);
};
```

Workers have access to all globals: `fetch`, `fs`, `Database`, `KNN`, `EmbeddingServer`, etc.

---

### `Database`

SQLite document store with full-text search (FTS5). Stores arbitrary JSON objects.

```javascript
// In-memory
var db = new Database();

// File-based (persists)
var db = new Database("store.db");

db.addItem({ name: "Alice", city: "Portland", age: 30 })
  .then(function(id) { console.log("Inserted:", id); });

// Exact field match
db.find("city", "Portland")
  .then(function(results) { console.log(results); });

// Full-text search across all text fields
db.search("portland alice")
  .then(function(results) { console.log(results); });
```

---

### `VectorDatabase`

Stores vectors with metadata and performs similarity search.

```javascript
var vdb = new VectorDatabase();          // in-memory
var vdb = new VectorDatabase("vecs.db"); // persisted to JSON

// Add item: vector (Float32Array or array), plus metadata fields
vdb.addItem({ vector: [0.1, 0.2, ...], label: "example", text: "hello" })
  .then(function(id) { console.log(id); });

// Query: (queryVector, topK, metric)
// metric: "cosine" (default), "euclidean", "manhattan"
vdb.query([0.1, 0.2, ...], 5, "cosine")
  .then(function(results) {
    results.forEach(function(r) {
      console.log(r.score, r.item.label);
    });
  });
```

---

### `GraphDatabase`

Property graph backed by SQLite.

```javascript
var graph = new GraphDatabase();           // in-memory
var graph = new GraphDatabase("graph.db"); // persisted

graph.addNode({ type: "Person", name: "Alice", age: 30 })
  .then(function(nodeId) {
    return graph.addNode({ type: "City", name: "Portland" })
      .then(function(cityId) {
        return graph.addEdge(nodeId, cityId, "LIVES_IN", { since: 2020 });
      });
  });

graph.getNode(nodeId).then(function(node) { console.log(node); });
graph.findNode("name", "Alice").then(function(node) { console.log(node); });
graph.getConnectedNodes(nodeId).then(function(neighbors) {
  console.log(neighbors);
});
```

---

### `KNN`

Embedding-based k-nearest neighbor text classifier using FastEmbed MiniLM.

```javascript
var knn = new KNN();            // in-memory
var knn = new KNN("data.json"); // persists to JSON

// Train with labeled examples
knn.train([
  { text: "I love this product", label: "positive" },
  { text: "This is terrible",    label: "negative" },
  { text: "It is okay",          label: "neutral" }
]).then(function() { console.log("Trained"); });

// Add a single example
knn.trainText("Absolutely fantastic!", "positive");

// Classify new text (returns top label string)
knn.classify("This works great")
  .then(function(label) { console.log(label); });

// Query for top-k results with confidence scores
knn.query("This works great", 3)
  .then(function(results) {
    results.forEach(function(r) {
      console.log(r.label, r.score);
    });
  });
```

---

### `TRM`

Tiny Recursive Model neural text classifier using recursive H-cycle/L-cycle reasoning (~7M params). Unlike KNN, TRM trains a neural network — a full retrain runs on every call to `train()` or `trainText()`. Persists the model weights, config, and training data to a directory.

```javascript
var trm = new TRM();              // in-memory (no persistence)
var trm = new TRM("model_dir/");  // persists to directory

// Train with labeled examples
trm.train([
  { text: "I love this product", label: "positive" },
  { text: "This is terrible",    label: "negative" },
  { text: "Absolutely wonderful", label: "positive" },
  { text: "Awful experience",    label: "negative" }
]).then(function() { console.log("Trained"); });

// Classify new text (returns top label string)
trm.classify("This is amazing")
  .then(function(label) { console.log(label); });  // "positive"

// Query for top-k results with softmax probability scores
trm.query("This works great", 2)
  .then(function(results) {
    results.forEach(function(r) {
      console.log(r.label, r.score);
    });
    // positive 0.87
    // negative 0.13
  });
```

---

### `EmbeddingServer`

BERT/ONNX models embedded in the binary. No external service required.

```javascript
var es = new EmbeddingServer();

// 384-dimensional sentence embedding (all-MiniLM-L6-v2)
es.generateEmbedding("Hello world")
  .then(function(vector) { console.log(vector.length); }); // 384

// Extractive QA (DistilBERT SQuAD)
es.askQuestion("What is the capital of France?", "France's capital is Paris.")
  .then(function(answer) { console.log(answer); }); // "Paris"

// Sentiment (DistilBERT SST-2)
es.getSentiment("I love this!")
  .then(function(result) { console.log(result); }); // "POSITIVE"

// Named entity recognition (BERT NER)
es.getNamedEntities("Alice works at Google in New York")
  .then(function(entities) { console.log(entities); });
  // [{ entity: "Alice", label: "PER" }, { entity: "Google", label: "ORG" }, ...]
```

---

### `StaticServer`

Serves files over HTTPS from a local directory.

```javascript
var server = new StaticServer("./public", 8443);
// Runs in background thread
// Requires /etc/hosts entry: 127.0.0.1 lnsy-static.local
```

---

## Patterns

### LLM Tool Loop

A complete agentic loop in a script file (`agent.js`):

```javascript
// agent.js — run with: lnsy-script agent.js
import { createToolbox } from "./tools.js";

const env = await fs.readDotEnv(".env");
const toolbox = createToolbox();
const messages = [{ role: "user", content: "Read the file README.md" }];

async function step() {
  const res = await fetch("https://api.openai.com/v1/chat/completions", {
    method: "POST",
    headers: {
      "Authorization": "Bearer " + env.OPENAI_API_KEY,
      "Content-Type": "application/json"
    },
    body: JSON.stringify({
      model: "gpt-4o",
      tools: toolbox.listTools(),
      messages
    })
  });
  const response = await res.json();
  const msg = response.choices[0].message;
  messages.push(msg);

  if (msg.tool_calls && msg.tool_calls.length > 0) {
    const results = await toolbox.callMany(msg.tool_calls);
    results.forEach(r => {
      messages.push({ role: "tool", tool_call_id: r.id, content: JSON.stringify(r.result) });
    });
    await step();
  } else {
    console.log(msg.content);
  }
}

await step();
```

```javascript
// tools.js
export function createToolbox() {
  return new Tools([
    {
      name: "read_file",
      description: "Read a file from disk",
      parameters: {
        type: "object",
        properties: { path: { type: "string" } },
        required: ["path"]
      },
      handler: async (args) => fs.readFile(args.path)
    }
  ]);
}
```

### Semantic Search Pipeline

```javascript
// search.js — run with: lnsy-script search.js
const es = new EmbeddingServer();
const vdb = new VectorDatabase("knowledge.db");

const docs = ["Paris is in France", "London is in England", "Tokyo is in Japan"];
for (const doc of docs) {
  const vec = await es.generateEmbedding(doc);
  await vdb.addItem({ vector: vec, text: doc });
}

const queryVec = await es.generateEmbedding("European capitals");
const results = await vdb.query(queryVec, 2, "cosine");
results.forEach(r => console.log(r.item.text, r.score));
```

### Dynamic Import in the REPL

Static `import` requires module context (script files). In the REPL, use dynamic `import()`:

```javascript
// REPL: load a local module dynamically
const { createToolbox } = await import("./tools.js");
```

### Parallel Workers

```javascript
const results = [];
const workers = ["worker1.js", "worker2.js", "worker3.js"].map(file => {
  const w = new Agent(file);
  w.onmessage = e => results.push(e.data);
  return w;
});

workers.forEach((w, i) => w.postMessage({ index: i, data: "input" }));

function poll() {
  __agentPoll();
  if (results.length < workers.length) poll();
  else console.log(results);
}
poll();
```

---

## Error Handling

Always attach `.catch()` to promise chains:

```javascript
fetch("https://api.example.com/data")
  .then(function(r) { return r.json(); })
  .then(function(data) { console.log(data); })
  .catch(function(err) { console.error("Failed:", err); });
```

Handler exceptions in `Tools` are caught and returned as `{ error: true, result: "message" }` — check for this in your tool loop rather than relying on thrown errors.

---

## Building and Installing

```bash
cargo build --release
# Binary at: ./target/release/lnsy-script

# macOS: move to PATH
cp target/release/lnsy-script /usr/local/bin/lnsy-script
```

The build step downloads ONNX model weights from Hugging Face and embeds them in the binary. No runtime model files are needed.
