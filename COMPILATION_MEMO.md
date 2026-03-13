# qjs-md Compilation Pipeline Memo

This document describes how `qjs-md` compiles a single Markdown file into HTML. The pipeline has five distinct stages: front matter parsing, content pre-processing, Markdown-to-HTML conversion, post-processing, and template compilation.

---

## Overview

```
.md file
    ↓  [1] Front Matter Parser    (lib/md-yaml.js)
    ↓  [2] Content Pre-processors (src/content/)
    ↓  [3] Markdown Parser        (lib/markdown.js)
    ↓  [4] Post-processors        (src/content/)
    ↓  [5] Template Compiler      (src/templates/engine.js)
    ↓
 .html file
```

The entry point is `src/compiler.js`. For each publishable Markdown file it finds (files with `publish: true` in their front matter), it runs this pipeline and writes the result to the output directory.

---

## Stage 1 — Front Matter Parsing

**File:** `lib/md-yaml.js`

The parser looks for content between two `---` delimiters at the top of the file and parses it as YAML. Everything after the closing `---` is treated as the body.

### Source Markdown

```markdown
---
title: "Getting Started with WebAssembly"
date: 2026-03-01
tags:
  - wasm
  - systems
  - rust
publish: true
type: post
summary: "A hands-on introduction to WASM modules."
---

WebAssembly changes how we ship code to the browser.
```

### Parsed JSON

The YAML block is parsed into a plain JavaScript object. The remaining content string is kept separately.

```json
{
  "data": {
    "title": "Getting Started with WebAssembly",
    "date": "2026-03-01",
    "tags": ["wasm", "systems", "rust"],
    "publish": true,
    "type": "post",
    "summary": "A hands-on introduction to WASM modules."
  },
  "content": "\nWebAssembly changes how we ship code to the browser.\n"
}
```

### YAML Type Support

The parser handles all common YAML scalar types:

| YAML input | Parsed type | Value |
|---|---|---|
| `true` / `false` | Boolean | `true` / `false` |
| `null` / `~` | Null | `null` |
| `42` | Number | `42` |
| `3.14` | Float | `3.14` |
| `"hello"` | String | `"hello"` |
| `[a, b, c]` | Array | `["a", "b", "c"]` |

---

## Stage 2 — Content Pre-processors

Before the Markdown parser runs, three processors transform the raw content string. Order matters: footnotes → abbreviations → wikilinks.

---

### 2a. Footnotes

**File:** `src/content/footnotes.js`

Footnote definitions (`[^label]: text`) are extracted from the content and stored. Inline references (`[^label]`) are replaced with numbered superscript links. The definitions are collected into a `<section>` block that gets appended after the main HTML is generated.

#### Source Markdown

```markdown
WebAssembly was first announced in 2015.[^history] It became a W3C recommendation in 2019.[^w3c]

[^history]: Announced jointly by Mozilla, Google, Microsoft, and Apple.
[^w3c]: See the official W3C press release for details.
```

#### After Footnote Pre-processing (content string passed to Markdown parser)

```markdown
WebAssembly was first announced in 2015.<sup><a href="#fn-history" id="fnref-history" class="footnote-ref" role="doc-noteref" aria-label="Footnote 1">1</a></sup> It became a W3C recommendation in 2019.<sup><a href="#fn-w3c" id="fnref-w3c" class="footnote-ref" role="doc-noteref" aria-label="Footnote 2">2</a></sup>
```

#### Footnote Section HTML (appended after body)

```html
<section class="footnotes" aria-label="Footnotes">
  <hr>
  <ol>
    <li id="fn-history">
      Announced jointly by Mozilla, Google, Microsoft, and Apple.
      <a href="#fnref-history" class="footnote-backref" role="doc-backlink" aria-label="Back to reference 1">↩</a>
    </li>
    <li id="fn-w3c">
      See the official W3C press release for details.
      <a href="#fnref-w3c" class="footnote-backref" role="doc-backlink" aria-label="Back to reference 2">↩</a>
    </li>
  </ol>
</section>
```

#### JSON Representation (conceptual)

```json
{
  "footnotes": [
    {
      "id": "history",
      "number": 1,
      "text": "Announced jointly by Mozilla, Google, Microsoft, and Apple."
    },
    {
      "id": "w3c",
      "number": 2,
      "text": "See the official W3C press release for details."
    }
  ]
}
```

---

### 2b. Abbreviations

**File:** `src/content/abbreviations.js`

Abbreviation definitions (`*[TERM]: Definition`) are extracted from the content. Every occurrence of each term in the remaining text is wrapped with an `<abbr>` tag. Definitions are sorted longest-first to prevent shorter terms from shadowing longer ones (e.g., matching `API` before `REST API`).

#### Source Markdown

```markdown
WASM runs in the browser alongside JS. The WASM binary format is compact and fast to decode.

*[WASM]: WebAssembly
*[JS]: JavaScript
```

#### After Abbreviation Pre-processing

```html
<abbr title="WebAssembly">WASM</abbr> runs in the browser alongside <abbr title="JavaScript">JS</abbr>. The <abbr title="WebAssembly">WASM</abbr> binary format is compact and fast to decode.
```

The definition lines (`*[TERM]: ...`) are removed from the content entirely.

#### JSON Representation (conceptual)

```json
{
  "abbreviations": [
    { "term": "WASM", "definition": "WebAssembly" },
    { "term": "JS",   "definition": "JavaScript" }
  ]
}
```

---

### 2c. Wikilinks

**File:** `src/content/wikilinks.js`

Double-bracket links (`[[Page Title]]`) are converted to HTML anchor tags. The page title is lowercased and spaces are replaced with hyphens to form the `href`.

#### Source Markdown

```markdown
See [[Getting Started]] for prerequisites and [[Advanced Topics]] for deeper coverage.
```

#### After Wikilink Pre-processing

```html
See <a href="getting-started.html">Getting Started</a> for prerequisites and <a href="advanced-topics.html">Advanced Topics</a> for deeper coverage.
```

---

## Stage 3 — Markdown Parser

**File:** `lib/markdown.js` (based on snarkdown)

The processed content string is passed to the Markdown parser, which converts standard Markdown syntax to HTML using regex-based tokenization.

---

### Headings

#### Source Markdown

```markdown
# WebAssembly Fundamentals

## Memory Model

### Linear Memory
```

#### Output HTML

```html
<h1>WebAssembly Fundamentals</h1>

<h2>Memory Model</h2>

<h3>Linear Memory</h3>
```

---

### Paragraphs and Inline Formatting

#### Source Markdown

```markdown
WASM modules are **compact** and _fast_. You can also use `inline code` within text.
```

#### Output HTML

```html
<p>WASM modules are <strong>compact</strong> and <em>fast</em>. You can also use <code>inline code</code> within text.</p>
```

---

### Lists

#### Source Markdown (Unordered)

```markdown
- Linear memory (a flat byte array)
- Import/export system
- Typed function table
```

#### Output HTML

```html
<ul>
  <li>Linear memory (a flat byte array)</li>
  <li>Import/export system</li>
  <li>Typed function table</li>
</ul>
```

#### Source Markdown (Ordered)

```markdown
1. Write Rust code
2. Compile to `.wasm` with `wasm-pack`
3. Import into JavaScript
```

#### Output HTML

```html
<ol>
  <li>Write Rust code</li>
  <li>Compile to <code>.wasm</code> with <code>wasm-pack</code></li>
  <li>Import into JavaScript</li>
</ol>
```

#### JSON Representation (conceptual)

```json
{
  "type": "list",
  "ordered": false,
  "items": [
    { "type": "listItem", "text": "Linear memory (a flat byte array)" },
    { "type": "listItem", "text": "Import/export system" },
    { "type": "listItem", "text": "Typed function table" }
  ]
}
```

---

### Links

#### Source Markdown

```markdown
Read the [MDN WASM docs](https://developer.mozilla.org/en-US/docs/WebAssembly) for the full reference.
```

#### Output HTML

```html
<p>Read the <a href="https://developer.mozilla.org/en-US/docs/WebAssembly">MDN WASM docs</a> for the full reference.</p>
```

Reference-style links are also supported:

```markdown
Visit [the spec][wasm-spec] for details.

[wasm-spec]: https://webassembly.github.io/spec/
```

Both produce the same `<a>` output.

---

### Code Blocks

The Markdown parser wraps fenced code blocks in `<pre>` tags with a `data-lang` attribute. The actual syntax highlighting happens in Stage 4.

#### Source Markdown

````markdown
```rust
use std::mem;

#[no_mangle]
pub extern "C" fn add(a: i32, b: i32) -> i32 {
    a + b
}
```
````

#### Output HTML (after Markdown parse, before highlighting)

```html
<pre class="code" data-lang="rust"><code>use std::mem;

#[no_mangle]
pub extern "C" fn add(a: i32, b: i32) -> i32 {
    a + b
}</code></pre>
```

#### JSON Representation (conceptual)

```json
{
  "type": "codeBlock",
  "language": "rust",
  "code": "use std::mem;\n\n#[no_mangle]\npub extern \"C\" fn add(a: i32, b: i32) -> i32 {\n    a + b\n}"
}
```

---

### Blockquotes

#### Source Markdown

```markdown
> The soul of a new machine — the WASM spec is a masterpiece of minimalism.
```

#### Output HTML

```html
<blockquote>The soul of a new machine — the WASM spec is a masterpiece of minimalism.</blockquote>
```

---

## Stage 4 — Post-processors

After the Markdown parser runs, three additional processors operate on the HTML string.

---

### 4a. Code Highlighting

**File:** `src/content/highlight.js`

The highlighter finds `<pre class="code" data-lang="LANG">` blocks and applies syntax coloring using language-specific regex rules.

**Supported languages:** `js`/`javascript`, `py`/`python`, `rs`/`rust`, `rb`/`ruby`, `c`, `json`, `yaml`, `toml`, `css`, `html`

The processor uses a slot-based approach to avoid conflicts: strings and comments are extracted first, keywords are colored, then strings and comments are restored.

#### Input HTML (from Markdown parser)

```html
<pre class="code" data-lang="rust"><code>pub extern "C" fn add(a: i32, b: i32) -> i32 {
    a + b
}</code></pre>
```

#### Output HTML (after highlighting)

```html
<pre class="code language-rust"><code><span class="keyword">pub</span> <span class="keyword">extern</span> <span class="string">"C"</span> <span class="keyword">fn</span> add(a: i32, b: i32) -> i32 {
    a + b
}</code></pre>
```

#### JavaScript example

````markdown
```js
const greet = (name) => {
  return `Hello, ${name}!`;
};
```
````

```html
<pre class="code language-js"><code><span class="keyword">const</span> greet = (name) => {
  <span class="keyword">return</span> <span class="string">`Hello, ${name}!`</span>;
};</code></pre>
```

#### Python example

````markdown
```python
def fibonacci(n: int) -> list[int]:
    result = [0, 1]
    while len(result) < n:
        result.append(result[-1] + result[-2])
    return result
```
````

```html
<pre class="code language-python"><code><span class="keyword">def</span> fibonacci(n: int) -> list[int]:
    result = [<span class="literal">0</span>, <span class="literal">1</span>]
    <span class="keyword">while</span> len(result) < n:
        result.append(result[-<span class="literal">1</span>] + result[-<span class="literal">2</span>])
    <span class="keyword">return</span> result</code></pre>
```

---

### 4b. Bare URL Linking

**File:** `src/content/urls.js`

Any bare `http://` or `https://` URL in the HTML that isn't already inside an HTML attribute is wrapped in an anchor tag.

#### Input (in HTML body)

```html
<p>Full spec at https://webassembly.github.io/spec/core/</p>
```

#### Output

```html
<p>Full spec at <a href="https://webassembly.github.io/spec/core/">https://webassembly.github.io/spec/core/</a></p>
```

---

### 4c. Image Embedding

**File:** `src/assets/handler.js`

Local images referenced in the HTML are read from disk and embedded as base64 data URIs. Inline SVGs are embedded directly.

#### Input HTML

```html
<img src="./diagrams/memory-model.svg" alt="WASM memory model">
```

#### Output HTML (SVG inlined)

```html
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 200">
  <!-- SVG content embedded directly -->
</svg>
```

---

## Stage 5 — Template Compilation

**File:** `src/templates/engine.js`

The compiled HTML body is inserted into a template. Templates are selected based on the `type` field in front matter (`type.html`), falling back to `post.html`, then `default.html`.

Templates use `{{variable}}` for value substitution and `{{filename.html}}` for partial includes.

### Template Selection

```
front matter type: "post"  →  templates/post.html
front matter type: "note"  →  templates/note.html  (if it exists)
                              else templates/post.html
                              else templates/default.html
```

### Template Variables

| Variable | Source |
|---|---|
| `{{title}}` | `data.title` from front matter |
| `{{date}}` | `data.date`, formatted |
| `{{tags}}` | Rendered tag links |
| `{{content}}` | Compiled HTML body |
| `{{summary}}` | First paragraph or `data.summary` |

### Example Template (`templates/post.html`)

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>{{title}}</title>
  {{head.html}}
</head>
<body>
  {{header.html}}
  <main>
    <article>
      <h1>{{title}}</h1>
      <time datetime="{{date}}">{{date}}</time>
      <div class="tags">{{tags}}</div>
      <div class="content">
        {{content}}
      </div>
    </article>
  </main>
  {{footer.html}}
</body>
</html>
```

### Final Output HTML

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>Getting Started with WebAssembly</title>
  <style>/* inlined CSS from assets */</style>
</head>
<body>
  <header><a href="/">Home</a></header>
  <main>
    <article>
      <h1>Getting Started with WebAssembly</h1>
      <time datetime="2026-03-01">March 1, 2026</time>
      <div class="tags">
        <a href="tag-wasm.html">wasm</a>
        <a href="tag-systems.html">systems</a>
        <a href="tag-rust.html">rust</a>
      </div>
      <div class="content">
        <p>
          <abbr title="WebAssembly">WASM</abbr> runs in the browser alongside
          <abbr title="JavaScript">JS</abbr>.
          WebAssembly was first announced in 2015.
          <sup><a href="#fn-history" id="fnref-history" class="footnote-ref">1</a></sup>
        </p>

        <pre class="code language-rust"><code>
          <span class="keyword">pub</span> <span class="keyword">extern</span>
          <span class="string">"C"</span> <span class="keyword">fn</span>
          add(a: i32, b: i32) -> i32 { a + b }
        </code></pre>

        <section class="footnotes" aria-label="Footnotes">
          <hr>
          <ol>
            <li id="fn-history">
              Announced jointly by Mozilla, Google, Microsoft, and Apple.
              <a href="#fnref-history" class="footnote-backref">↩</a>
            </li>
          </ol>
        </section>
      </div>
    </article>
  </main>
  <footer>© 2026</footer>
</body>
</html>
```

---

## Complete End-to-End Example

The following shows a single Markdown file processed through every stage.

### Source Markdown

```markdown
---
title: "WASM Memory"
date: 2026-03-12
tags: [wasm, systems]
publish: true
type: post
---

WASM uses a linear memory model.[^spec] All data lives in a flat byte array accessible from both WASM and JS.

*[WASM]: WebAssembly
*[JS]: JavaScript

See [[Memory Layout]] for how the stack and heap are arranged.

## Accessing Memory

Use the `WebAssembly.Memory` API to read and write bytes:

```js
const memory = new WebAssembly.Memory({ initial: 1 });
const view = new Uint8Array(memory.buffer);
view[0] = 42;
```

[^spec]: Defined in Section 2.3 of the WebAssembly Core Specification.
```

### Parsed Front Matter JSON

```json
{
  "data": {
    "title": "WASM Memory",
    "date": "2026-03-12",
    "tags": ["wasm", "systems"],
    "publish": true,
    "type": "post"
  },
  "content": "..."
}
```

### After Pre-processors (content string, simplified)

```
WASM uses a linear memory model.<sup><a href="#fn-spec" ...>1</a></sup>
All data lives in a flat byte array accessible from both WASM and JS.

See <a href="memory-layout.html">Memory Layout</a> for how the stack and heap are arranged.

## Accessing Memory

Use the `WebAssembly.Memory` API to read and write bytes: ...codeblock...
```

Note: abbreviation definitions (`*[WASM]: ...`) are removed from the string, and all occurrences of `WASM` and `JS` in the remaining text are wrapped with `<abbr>` tags.

### After Markdown Parser

```html
<p><abbr title="WebAssembly">WASM</abbr> uses a linear memory model.<sup>...</sup>
All data lives in a flat byte array accessible from both <abbr title="WebAssembly">WASM</abbr> and <abbr title="JavaScript">JS</abbr>.</p>

<p>See <a href="memory-layout.html">Memory Layout</a> for how the stack and heap are arranged.</p>

<h2>Accessing Memory</h2>

<p>Use the <code>WebAssembly.Memory</code> API to read and write bytes:</p>

<pre class="code" data-lang="js"><code>const memory = new WebAssembly.Memory({ initial: 1 });
const view = new Uint8Array(memory.buffer);
view[0] = 42;</code></pre>

<section class="footnotes">...</section>
```

### After Code Highlighter

```html
<pre class="code language-js"><code>
<span class="keyword">const</span> memory = <span class="keyword">new</span> WebAssembly.Memory({ initial: <span class="literal">1</span> });
<span class="keyword">const</span> view = <span class="keyword">new</span> Uint8Array(memory.buffer);
view[<span class="literal">0</span>] = <span class="literal">42</span>;
</code></pre>
```

### Final HTML (inserted into template)

```html
<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <title>WASM Memory</title>
  <style>/* inlined CSS */</style>
</head>
<body>
  <main>
    <article>
      <h1>WASM Memory</h1>
      <time datetime="2026-03-12">March 12, 2026</time>
      <div class="content">
        <p>
          <abbr title="WebAssembly">WASM</abbr> uses a linear memory model.
          <sup><a href="#fn-spec" id="fnref-spec" class="footnote-ref">1</a></sup>
          All data lives in a flat byte array accessible from both
          <abbr title="WebAssembly">WASM</abbr> and <abbr title="JavaScript">JS</abbr>.
        </p>
        <p>
          See <a href="memory-layout.html">Memory Layout</a> for how the
          stack and heap are arranged.
        </p>
        <h2>Accessing Memory</h2>
        <p>Use the <code>WebAssembly.Memory</code> API to read and write bytes:</p>
        <pre class="code language-js"><code>
<span class="keyword">const</span> memory = <span class="keyword">new</span> WebAssembly.Memory({ initial: <span class="literal">1</span> });
<span class="keyword">const</span> view = <span class="keyword">new</span> Uint8Array(memory.buffer);
view[<span class="literal">0</span>] = <span class="literal">42</span>;
        </code></pre>
        <section class="footnotes" aria-label="Footnotes">
          <hr>
          <ol>
            <li id="fn-spec">
              Defined in Section 2.3 of the WebAssembly Core Specification.
              <a href="#fnref-spec" class="footnote-backref">↩</a>
            </li>
          </ol>
        </section>
      </div>
    </article>
  </main>
</body>
</html>
```

---

## Key Files Reference

| Stage | File | Responsibility |
|---|---|---|
| Orchestration | `src/compiler.js` | Runs the full pipeline for each file |
| Front matter | `lib/md-yaml.js` | YAML → JS object |
| Footnotes | `src/content/footnotes.js` | `[^label]` → `<sup>` + `<section>` |
| Abbreviations | `src/content/abbreviations.js` | `*[TERM]: Def` → `<abbr>` |
| Wikilinks | `src/content/wikilinks.js` | `[[Page]]` → `<a href>` |
| Markdown | `lib/markdown.js` | Markdown → HTML (snarkdown) |
| Highlighting | `src/content/highlight.js` | Code spans → `<span class="keyword">` |
| URL linking | `src/content/urls.js` | Bare URLs → `<a href>` |
| Images | `src/assets/handler.js` | Local images → base64 / inline SVG |
| Templates | `src/templates/engine.js` | `{{variable}}` substitution + includes |
| File I/O | `src/utils/file-ops.js` | File discovery and writing |
