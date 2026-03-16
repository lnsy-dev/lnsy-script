#!/usr/bin/env bash

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
BINARY="$PROJECT_DIR/target/debug/lnsy-script"

GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

PASS=0
FAIL=0

echo "Building lnsy-script..."
cd "$PROJECT_DIR" && cargo build --quiet
echo "Build OK"
echo ""

# Strip REPL artifacts from captured stdout.
# Prompts use print! (no newline), so they pile up on the same line as output.
# Use global replacement to remove all occurrences, not just line-start.
strip_repl() {
    grep -v "^lnsy-script " |
    sed 's/lnsy> //g' |
    sed 's/  \.\.\. //g' |
    grep -v '^[[:space:]]*$'
}

# Strip ANSI color codes from stderr
strip_ansi() {
    sed 's/\x1b\[[0-9;]*m//g'
}

check() {
    local name="$1"
    local js_file="$2"
    local expected="$3"

    actual=$(cat "$js_file" | "$BINARY" 2>/dev/null | strip_repl || true)

    if [ "$actual" = "$expected" ]; then
        printf "${GREEN}PASS${NC}  %s\n" "$name"
        PASS=$((PASS + 1))
    else
        printf "${RED}FAIL${NC}  %s\n" "$name"
        echo "  expected: |$(echo "$expected" | head -5)|"
        echo "  actual:   |$(echo "$actual" | head -5)|"
        FAIL=$((FAIL + 1))
    fi
}

# Run a .js file directly (supports top-level await, no REPL scoping quirks)
check_file() {
    local name="$1"
    local js_file="$2"
    local expected="$3"

    actual=$("$BINARY" "$js_file" 2>/dev/null || true)

    if [ "$actual" = "$expected" ]; then
        printf "${GREEN}PASS${NC}  %s\n" "$name"
        PASS=$((PASS + 1))
    else
        printf "${RED}FAIL${NC}  %s\n" "$name"
        echo "  expected: |$(echo "$expected" | head -5)|"
        echo "  actual:   |$(echo "$actual" | head -5)|"
        FAIL=$((FAIL + 1))
    fi
}

check_stderr() {
    local name="$1"
    local js_file="$2"
    local expected="$3"

    actual=$(cat "$js_file" | "$BINARY" 2>&1 1>/dev/null | strip_ansi || true)

    if [ "$actual" = "$expected" ]; then
        printf "${GREEN}PASS${NC}  %s\n" "$name"
        PASS=$((PASS + 1))
    else
        printf "${RED}FAIL${NC}  %s\n" "$name"
        echo "  expected: |$expected|"
        echo "  actual:   |$actual|"
        FAIL=$((FAIL + 1))
    fi
}

check_static_server() {
    local name="$1"
    local port=3001
    local serve_dir="$SCRIPT_DIR"

    # Keep stdin open so the REPL doesn't exit before the server starts
    (echo "new StaticServer(\"$serve_dir\", $port)"; sleep 15) | "$BINARY" > /dev/null 2>&1 &
    local pid=$!

    # Wait for server to come up
    sleep 2

    # Fetch a known file via HTTPS (-k skips cert verification for self-signed)
    local actual
    actual=$(curl -sk "https://lnsy-static.local:$port/basic.js" 2>/dev/null)

    kill $pid 2>/dev/null
    wait $pid 2>/dev/null

    local expected
    expected=$(cat "$SCRIPT_DIR/basic.js")

    if [ "$actual" = "$expected" ]; then
        printf "${GREEN}PASS${NC}  %s\n" "$name"
        PASS=$((PASS + 1))
    else
        printf "${RED}FAIL${NC}  %s\n" "$name"
        echo "  expected: |$(echo "$expected" | head -3)|"
        echo "  actual:   |$(echo "$actual" | head -3)|"
        FAIL=$((FAIL + 1))
    fi
}

# --- Tests ---

check "console.log strings" \
    "$SCRIPT_DIR/basic.js" \
    "$(printf 'hello world\ngoodbye world')"

check "console.log numbers and booleans" \
    "$SCRIPT_DIR/types.js" \
    "$(printf '42\n3.14\ntrue\nfalse\nnull')"

check "console.log objects and arrays" \
    "$SCRIPT_DIR/objects.js" \
    "$(printf '{"a":1,"b":2}\n[1,2,3]\n{"nested":{"x":10}}')"

check "multi-line function definition" \
    "$SCRIPT_DIR/multiline.js" \
    "$(printf '7\n30')"

check_stderr "console.warn and console.error go to stderr" \
    "$SCRIPT_DIR/warn_error.js" \
    "$(printf 'this is a warning\nthis is an error\nanother warning')"

check "fetch is a function" \
    "$SCRIPT_DIR/fetch_struct.js" \
    "$(printf 'function\nfunction')"

check "fetch GET returns status 200" \
    "$SCRIPT_DIR/fetch_status.js" \
    "$(printf '200\ntrue')"

check "fetch json() parses response body" \
    "$SCRIPT_DIR/fetch_json.js" \
    "$(printf 'object\nobject')"

check "fs.writeFile creates a file and fs.readFile loads it" \
    "$SCRIPT_DIR/fs_write.js" \
    "$(printf 'hello file\nfalse')"

check "fs.readFile returns file contents" \
    "$SCRIPT_DIR/fs_read.js" \
    "$(printf 'read me back')"

check "fs.appendFile appends to existing file" \
    "$SCRIPT_DIR/fs_append.js" \
    "$(printf 'line1\nline2\nline3')"

check "fs.deleteFile removes file" \
    "$SCRIPT_DIR/fs_delete.js" \
    "$(printf 'true\nfalse')"

check_static_server "StaticServer serves files over HTTPS"

check "EmbeddingServer is defined as a class" \
    "$SCRIPT_DIR/embedding_server_class.js" \
    "function"

check "EmbeddingServer.generateEmbedding returns a float vector of length 384" \
    "$SCRIPT_DIR/embedding_server_embedding.js" \
    "$(printf 'true\n384\nnumber\ntrue\ntrue')"

check "EmbeddingServer.askQuestion extracts answer span from context" \
    "$SCRIPT_DIR/embedding_server_qa.js" \
    "$(printf 'Paris\nstring\ntrue')"

check "EmbeddingServer.getSentiment classifies positive and negative sentiment" \
    "$SCRIPT_DIR/embedding_server_sentiment.js" \
    "$(printf 'POSITIVE\nnumber\ntrue\nNEGATIVE')"

check "EmbeddingServer.getNamedEntities extracts ORG and LOC entities" \
    "$SCRIPT_DIR/embedding_server_ner.js" \
    "$(printf 'true\ntrue\nORG: Apple\nLOC: Cupertino')"

check "Database is defined as a class" \
    "$SCRIPT_DIR/database_class.js" \
    "function"

check "Database addItem, find, search (in-memory)" \
    "$SCRIPT_DIR/database_ops.js" \
    "$(printf 'true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue')"

check "VectorDatabase is defined as a class" \
    "$SCRIPT_DIR/vector_database_class.js" \
    "true"

check "VectorDatabase addItem and query (cosine, euclidean, manhattan)" \
    "$SCRIPT_DIR/vector_database_ops.js" \
    "$(printf 'true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue')"

check "GraphDatabase is defined as a class" \
    "$SCRIPT_DIR/graph_database_class.js" \
    "function"

check "GraphDatabase addNode, addEdge, getNode, findNode, getConnectedNodes (in-memory)" \
    "$SCRIPT_DIR/graph_database_ops.js" \
    "$(printf 'true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue')"

check "KNN is defined as a class" \
    "$SCRIPT_DIR/knn_class.js" \
    "true"

check "KNN.train accepts [{text, label}] and query returns [{label, score}]" \
    "$SCRIPT_DIR/knn_train.js" \
    "$(printf 'true\ntrue\ntrue\ntrue')"

check "KNN.trainText accepts separate text and label arrays" \
    "$SCRIPT_DIR/knn_train_text.js" \
    "$(printf 'true\ntrue\ntrue\ntrue\ntrue')"

check "KNN.query returns k results sorted by descending score" \
    "$SCRIPT_DIR/knn_query.js" \
    "$(printf 'true\ntrue\ntrue\ntrue\ntrue\ntrue')"

check "KNN.classify returns the majority label among nearest neighbors" \
    "$SCRIPT_DIR/knn_classify.js" \
    "$(printf 'true\ntrue\ntrue\ntrue')"

check "KNN persists training data to file and reloads on construction" \
    "$SCRIPT_DIR/knn_persist.js" \
    "$(printf 'true\ntrue\ntrue\ntrue')"

check "Agent is defined as a class" \
    "$SCRIPT_DIR/agent_class.js" \
    "function"

check "Agent round-trip message via echo worker" \
    "$SCRIPT_DIR/agent_basic.js" \
    "HELLO"

check "Agent worker can use fetch" \
    "$SCRIPT_DIR/agent_fetch.js" \
    "200"

check "Tools class and error types are defined as globals" \
    "$SCRIPT_DIR/tools_class.js" \
    "$(printf 'true\ntrue\ntrue\ntrue')"

check "Tools basic construction, hasTool, listTools, and direct call" \
    "$SCRIPT_DIR/tools_basic.js" \
    "$(printf 'true\nfalse\ntrue\ntrue\ntrue')"

check "Tools.call with LLM tool_call object returns structured result" \
    "$SCRIPT_DIR/tools_tool_call.js" \
    "$(printf 'true\ntrue\ntrue')"

check "Tools throws ToolNotFoundError, ToolRegistrationError, ToolValidationError" \
    "$SCRIPT_DIR/tools_errors.js" \
    "$(printf 'true\ntrue\ntrue')"

check "Tools strips undeclared params and wraps handler errors in tool_call response" \
    "$SCRIPT_DIR/tools_strip_params.js" \
    "$(printf 'true\ntrue\ntrue')"

check "cl basic: stdout, stderr, code, success, duration" \
    "$SCRIPT_DIR/cl_basic.js" \
    "$(printf 'true\ntrue\ntrue\ntrue\ntrue')"

check "cl non-zero exit resolves (does not reject)" \
    "$SCRIPT_DIR/cl_nonzero.js" \
    "$(printf 'true\ntrue')"

check "cl command not found rejects with Error" \
    "$SCRIPT_DIR/cl_notfound.js" \
    "$(printf 'true\ntrue\ntrue')"

check "cl timeout rejects with message and partial result" \
    "$SCRIPT_DIR/cl_timeout.js" \
    "$(printf 'true\ntrue\ntrue\ntrue\ntrue\ntrue')"

check_file "cl onStatus receives chunk/stream/elapsed/kill on each chunk" \
    "$SCRIPT_DIR/cl_onstatus.js" \
    "$(printf 'true\ntrue\ntrue\ntrue\ntrue\ntrue\ntrue')"

check_file "cl kill() from onStatus terminates process early" \
    "$SCRIPT_DIR/cl_kill.js" \
    "$(printf 'true\ntrue\ntrue')"

check "cl options: cwd, env, stdin" \
    "$SCRIPT_DIR/cl_opts.js" \
    "$(printf 'true\ntrue\ntrue')"

check_file "cl slow command streams incrementally and completes" \
    "$SCRIPT_DIR/cl_slow.js" \
    "$(printf 'true\ntrue\ntrue\ntrue\ntrue\ntrue')"

# ---

echo ""
echo "Results: ${PASS} passed, ${FAIL} failed"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
