# lnsy-script

A JavaScript runtime built in Rust, embedding ML and AI capabilities directly into a scriptable environment. Powered by QuickJS for a lightweight JS engine and Rust for high-performance inference.

---

## Installation

```sh
cargo build --release
./target/release/lnsy-script          # start REPL
./target/release/lnsy-script file.js  # run a script file
```

---

## JavaScript Runtime

lnsy-script is a full JavaScript REPL and script runner. It supports modern JS syntax via QuickJS and exposes a set of built-in globals for I/O, networking, ML, and databases.

**Built-in globals:**

| Global | Description |
|--------|-------------|
| `console` | `log`, `warn`, `error` |
| `fetch` | HTTP client (Promise-based) |
| `fs` | File system utilities |
| `KNN` | k-nearest neighbor classifier |
| `EmbeddingServer` | BERT-based NLP (embeddings, QA, sentiment, NER) |
| `Database` | SQLite with full-text search |
| `VectorDatabase` | Embedding store with similarity search |
| `GraphDatabase` | Property graph (nodes + edges) |
| `Agent` | Multi-threaded JavaScript workers |
| `StaticServer` | Static file HTTPS server |
| `Tools` | LLM tool-calling registry and executor |

**Promise support** — all async operations return Promises and can be chained:

```javascript
fetch("https://api.example.com/data")
  .then(function(r) { return r.json(); })
  .then(function(data) { console.log(data); })
  .catch(function(e) { console.log("error: " + e); });
```

**File system (`fs`):**

```javascript
fs.writeFile("/tmp/notes.txt", "hello world");
var content = fs.readFile("/tmp/notes.txt");
fs.appendFile("/tmp/notes.txt", "\nmore text");
var exists = fs.exists("/tmp/notes.txt");  // true
fs.deleteFile("/tmp/notes.txt");

var env = fs.readDotEnv(".env");           // parse .env → object
```

---

## KNN

Text classification using k-nearest neighbors over sentence embeddings. Embeddings are generated with FastEmbed's MiniLM model (384 dimensions). Data can be persisted to a JSON file.

```javascript
var knn = new KNN();           // in-memory
var knn = new KNN("data.json") // persisted
```

**Methods:**

| Method | Description |
|--------|-------------|
| `train(data)` | Train on `[{text, label}, ...]` |
| `trainText(texts, labels)` | Train on separate text/label arrays |
| `query(text, k?)` | Return k nearest `[{label, score}, ...]` |
| `classify(text, k?)` | Return the majority-vote label |

**Example:**

```javascript
var knn = new KNN();

knn.train([
  { text: "I love this product", label: "positive" },
  { text: "Absolutely wonderful", label: "positive" },
  { text: "Terrible experience",  label: "negative" },
  { text: "Complete waste of time", label: "negative" }
]).then(function() {
  return knn.classify("This is amazing");
}).then(function(label) {
  console.log(label);  // "positive"
});
```

**Query with scores:**

```javascript
knn.query("great purchase", 3).then(function(results) {
  results.forEach(function(r) {
    console.log(r.label + " - " + r.score);
  });
  // positive - 0.94
  // positive - 0.91
  // negative - 0.43
});
```

---

## BERT

Transformer-based NLP via `EmbeddingServer`. All ONNX models are compiled into the binary — no model downloads required.

**Bundled models:**
- MiniLM — sentence embeddings (384-dim)
- DistilBERT — extractive question answering
- DistilRoBERTa — sentiment classification
- BERT-NER — named entity recognition

```javascript
var es = new EmbeddingServer();
```

### Embeddings

Generate a 384-dimensional float vector for any text:

```javascript
es.generateEmbedding("The quick brown fox").then(function(vec) {
  console.log(vec.length);  // 384
});
```

### Question Answering

Extract an answer span from a passage of text:

```javascript
es.askQuestion(
  "The Eiffel Tower is located in Paris, France.",
  "Where is the Eiffel Tower?"
).then(function(answer) {
  console.log(answer);  // "Paris"
});
```

### Sentiment Analysis

Classify text as positive or negative with a confidence score:

```javascript
es.getSentiment("I absolutely love this!").then(function(result) {
  console.log(result.label);  // "POSITIVE"
  console.log(result.score);  // 0.98
});
```

### Named Entity Recognition

Extract named entities and their types from text:

```javascript
es.getNamedEntities("Apple Inc. is headquartered in Cupertino.").then(function(entities) {
  entities.forEach(function(e) {
    console.log(e.entity + ": " + e.word);
    // ORG: Apple Inc.
    // LOC: Cupertino
  });
});
```

---

## Fetch

Standards-compatible `fetch` API for HTTP requests, backed by Rust's `reqwest`.

```javascript
fetch(url, options?)  // → Promise<Response>
```

**Options:**

```javascript
{
  method: "GET",                              // GET, POST, PUT, DELETE, PATCH
  body: JSON.stringify({ key: "value" }),
  headers: { "Content-Type": "application/json" }
}
```

**Response:**

```javascript
response.status      // 200
response.ok          // true if 200–299
response.statusText  // "OK"
response.url         // final URL
response.text()      // → Promise<String>
response.json()      // → Promise<Object>
```

**Examples:**

```javascript
// GET request
fetch("https://httpbin.org/json").then(function(r) {
  return r.json();
}).then(function(data) {
  console.log(data);
});

// POST request
fetch("https://httpbin.org/post", {
  method: "POST",
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify({ name: "lnsy-script" })
}).then(function(r) {
  return r.json();
}).then(function(data) {
  console.log(data.json.name);  // "lnsy-script"
});
```

---

## Agents

Multi-threaded JavaScript workers. Each `Agent` runs in its own OS thread with a full JS runtime. Communication is message-passing via JSON-serialized events.

```javascript
var agent = new Agent("worker.js");

agent.onmessage = function(event) {
  console.log(event.data);
};

agent.postMessage({ task: "compute", n: 1000 });
```

Call `__agentPoll()` in the main thread to receive messages from workers:

```javascript
for (var i = 0; i < 1000; i++) {
  __agentPoll();
}
```

**Worker file** (`worker.js`):

```javascript
self.onmessage = function(event) {
  var result = event.data.n * 2;
  self.postMessage(result);
};
```

Workers have access to the full runtime — including `KNN`, `EmbeddingServer`, `fetch`, `Database`, etc. — making them suitable for parallel inference pipelines.

**Parallel classification example:**

```javascript
var workers = [new Agent("classifier.js"), new Agent("classifier.js")];
var results = [];

workers.forEach(function(w, i) {
  w.onmessage = function(e) { results[i] = e.data; };
  w.postMessage({ text: "classify this text", id: i });
});

for (var i = 0; i < 500; i++) { __agentPoll(); }
```

---

## Databases

### Database (SQLite)

General-purpose document store with full-text search (FTS5).

```javascript
var db = new Database();            // in-memory
var db = new Database("store.db");  // persisted
```

| Method | Description |
|--------|-------------|
| `addItem(obj)` | Insert JSON object, returns `Promise<id>` |
| `find(key, value)` | Find records by JSON field |
| `search(query)` | Full-text search across all text fields |

```javascript
db.addItem({ name: "alice", city: "portland" }).then(function(id) {
  return db.find("city", "portland");
}).then(function(rows) {
  console.log(rows[0].name);  // "alice"
});

db.search("portland").then(function(rows) {
  console.log(rows.length);
});
```

### VectorDatabase

Store and query embedding vectors with metadata.

```javascript
var vdb = new VectorDatabase();
var vdb = new VectorDatabase("vecs.db");
```

```javascript
var es = new EmbeddingServer();
es.generateEmbedding("cat").then(function(vec) {
  return vdb.addItem(vec, { label: "cat" });
}).then(function() {
  return es.generateEmbedding("kitten");
}).then(function(queryVec) {
  return vdb.query(queryVec, 5, "cosine");
}).then(function(results) {
  console.log(results[0].metadata.label);  // "cat"
  console.log(results[0].score);           // ~0.95
});
```

Supported metrics: `"cosine"` (default), `"euclidean"`, `"manhattan"`.

### GraphDatabase

Property graph with typed nodes and directed edges.

```javascript
var g = new GraphDatabase();
var g = new GraphDatabase("graph.db");
```

| Method | Description |
|--------|-------------|
| `addNode(obj)` | Create node, returns `Promise<id>` |
| `addEdge({source, target, name, ...})` | Create edge between nodes |
| `getNode(id)` | Fetch node by ID |
| `findNode({key: value})` | Find first matching node |
| `getConnectedNodes(id)` | Get outgoing neighbors |

```javascript
var aliceId, bobId;

g.addNode({ name: "Alice", role: "engineer" }).then(function(id) {
  aliceId = id;
  return g.addNode({ name: "Bob", role: "manager" });
}).then(function(id) {
  bobId = id;
  return g.addEdge({ source: aliceId, target: bobId, name: "reports_to" });
}).then(function() {
  return g.getConnectedNodes(aliceId);
}).then(function(neighbors) {
  console.log(neighbors[0].name);  // "Bob"
});
```

---

## Tools

A registry and executor for LLM tool-calling. Register named functions with JSON Schema parameter definitions — nothing can be invoked unless it's been explicitly added.

```javascript
var toolbox = new Tools([
  {
    name: 'get_weather',
    description: 'Get current weather for a city',
    parameters: {
      type: 'object',
      properties: {
        city: { type: 'string', description: 'City name' }
      },
      required: ['city']
    },
    handler: async function(args) {
      var r = await fetch('https://api.example.com/weather?city=' + args.city);
      return r.json();
    }
  }
]);
```

**Registry methods:**

| Method | Description |
|--------|-------------|
| `addTool(def)` | Register a tool definition `{name, description, parameters, handler}` |
| `removeTool(name)` | Remove a registered tool |
| `hasTool(name)` | Returns `true` if the tool is registered |
| `listTools()` | Returns OpenAI-compatible tool schema array |

**Execution:**

```javascript
// Direct call — you know the name and args
var result = await toolbox.call('get_weather', { city: 'Portland' });

// LLM tool_call object — pass the raw object from an LLM response
var result = await toolbox.call({
  id: 'call_abc123',
  type: 'function',
  function: { name: 'get_weather', arguments: '{"city":"Portland"}' }
});
// → { tool_call_id: 'call_abc123', name: 'get_weather', result: { ... } }

// Batch — execute multiple tool calls concurrently
var results = await toolbox.callMany(response.tool_calls);
```

**Behavior:**

| Situation | Behavior |
|-----------|----------|
| Unregistered tool | Throws `ToolNotFoundError` |
| Missing required parameter | Throws `ToolValidationError` |
| Extra unknown parameters | Silently stripped before handler is called |
| Handler throws at runtime | Returns `{ tool_call_id, name, error: true, result: message }` |
| `arguments` is a JSON string | Parsed automatically |
| Malformed JSON arguments | Throws `ToolValidationError` |
| `addTool()` with duplicate name | Throws `ToolRegistrationError` |

**Full loop example:**

```javascript
var toolbox = new Tools([/* ...tool definitions... */]);

// 1. Send schemas to the LLM
var response = await fetch('https://api.openai.com/v1/chat/completions', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json', 'Authorization': 'Bearer ' + key },
  body: JSON.stringify({ model: 'gpt-4o', messages: messages, tools: toolbox.listTools() })
}).then(function(r) { return r.json(); });

// 2. Execute all requested tool calls
var toolResults = await toolbox.callMany(response.choices[0].message.tool_calls);

// 3. Feed results back as tool messages
toolResults.forEach(function(r) {
  messages.push({ role: 'tool', tool_call_id: r.tool_call_id, content: JSON.stringify(r.result) });
});
```

---

## Static Server

Serve static files over HTTPS from a local directory.

```javascript
new StaticServer("/path/to/www", 8443);
// → https://lnsy-static.local:8443
```

Uses a self-signed TLS certificate. Runs in a background thread and does not block the JS event loop.

---

## Building for LLMs

See [AGENTS.md](./AGENTS.md) for a complete guide to writing lnsy-script applications with large language models.

---

## License

MIT
