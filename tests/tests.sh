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

# ---

echo ""
echo "Results: ${PASS} passed, ${FAIL} failed"
[ "$FAIL" -eq 0 ] && exit 0 || exit 1
