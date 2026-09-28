"""Static assets for the 'preview' command's local web UI.

One self-contained, fully offline HTML page: every style and script is
inline, nothing is fetched from a CDN.  preview.py fills two placeholder
tokens at startup:

  __POLYGLOT_INITIAL_SOURCE_JSON__   editor's initial source as a JSON
                                     string literal (script-safe)
  __POLYGLOT_INITIAL_LANGUAGE_JSON__ "c" or "cpp"

Use render_page(); it guarantees the tokens are substituted exactly once
and that a source containing '</script>' cannot break out of the inline
script block.
"""

import json

#: demo snippet shown when no FILE argument is given.
DEFAULT_SOURCE = """#include <stdio.h>

int add(int a, int b) { return a + b; }

int main(void) {
    for (int i = 0; i < 5; i++)
        printf("add(%d, %d) = %d\\n", i, i, add(i, i));
    return 0;
}
"""

PAGE_TEMPLATE = """<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>polyglot preview</title>
<style>
  :root {
    --bg: #0d1017;
    --panel: #141926;
    --panel2: #1a2133;
    --border: #2a3247;
    --text: #d8dfec;
    --muted: #8b96ad;
    --accent: #62a0ff;
    --ok: #4fc38a;
    --warn: #e6b455;
    --bad: #ef8080;
    --mono: ui-monospace, SFMono-Regular, Menlo, Consolas,
            "Liberation Mono", "Courier New", monospace;
  }
  * { box-sizing: border-box; }
  html, body { height: 100%; }
  body {
    margin: 0; background: var(--bg); color: var(--text);
    font: 13px/1.5 var(--mono);
    display: flex; flex-direction: column;
  }
  header {
    display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
    padding: 8px 12px; background: var(--panel);
    border-bottom: 1px solid var(--border);
  }
  .brand { font-weight: 700; letter-spacing: .4px; color: var(--accent); }
  .brand span { color: var(--muted); font-weight: 400; }
  select, button {
    font: inherit; color: var(--text); background: var(--panel2);
    border: 1px solid var(--border); border-radius: 6px; padding: 4px 8px;
  }
  select { cursor: pointer; }
  button { cursor: pointer; }
  button:hover { border-color: var(--accent); }
  .chk {
    display: inline-flex; align-items: center; gap: 5px;
    color: var(--muted); cursor: pointer; user-select: none;
  }
  .chk input { accent-color: var(--accent); cursor: pointer; }
  #compile {
    background: var(--accent); border-color: var(--accent);
    color: #0b1020; font-weight: 700; padding: 4px 16px;
  }
  #compile:disabled { opacity: .55; cursor: default; }
  #status { color: var(--muted); white-space: nowrap; }
  #status.ok { color: var(--ok); }
  #status.warn { color: var(--warn); }
  #status.bad { color: var(--bad); }
  .spacer { flex: 1; }
  .hint { color: var(--muted); white-space: nowrap; }
  .hint kbd {
    background: var(--panel2); border: 1px solid var(--border);
    border-radius: 4px; padding: 0 5px; font: inherit;
  }
  main {
    flex: 1; min-height: 0;
    display: grid; grid-template-columns: minmax(280px, 1fr) minmax(320px, 1fr);
  }
  .left { display: flex; min-width: 0; border-right: 1px solid var(--border); }
  #src {
    flex: 1; resize: none; border: 0; outline: 0; padding: 10px 12px;
    background: var(--bg); color: var(--text); font: inherit;
    white-space: pre; tab-size: 4;
  }
  .right { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  .tabs {
    display: flex; align-items: center; gap: 2px;
    padding: 6px 8px 0; background: var(--panel);
    border-bottom: 1px solid var(--border);
  }
  .tab {
    position: relative; background: none; color: var(--muted);
    border: 1px solid transparent; border-bottom: 0;
    border-radius: 6px 6px 0 0; padding: 5px 12px;
  }
  .tab.active { color: var(--text); background: var(--bg); border-color: var(--border); }
  .tab .dot {
    display: none; position: absolute; top: 4px; right: 4px;
    width: 6px; height: 6px; border-radius: 50%; background: var(--warn);
  }
  .tab.noteworthy .dot { display: block; }
  .panes { flex: 1; min-height: 0; position: relative; }
  .pane {
    display: none; position: absolute; inset: 0;
    overflow: auto; padding: 8px 10px;
  }
  .pane.active { display: block; }
  .pane pre { margin: 0; }
  .copy {
    position: sticky; top: 0; float: right; z-index: 3;
    font-size: 11px; padding: 2px 8px; color: var(--muted);
    background: var(--panel2);
  }
  .ph:empty::after { content: "(nothing yet - press Compile)"; color: var(--muted); }
  #stderr.ph:empty::after { content: "(no diagnostics)"; }
  @media (max-width: 760px) {
    main { grid-template-columns: 1fr; grid-template-rows: minmax(160px, 42%) 1fr; }
    .left { border-right: 0; border-bottom: 1px solid var(--border); }
  }
</style>
</head>
<body>
<header>
  <div class="brand">polyglot <span>preview</span></div>
  <select id="lang" title="source language">
    <option value="c">C</option>
    <option value="cpp">C++</option>
  </select>
  <select id="backend" title="compiler backend">
    <option value="auto">backend: auto</option>
    <option value="clang">clang</option>
    <option value="gcc">gcc</option>
  </select>
  <select id="opt" title="optimization level">
    <option value="none">-O0</option>
    <option value="less">-O1</option>
    <option value="default" selected>-O2</option>
    <option value="aggressive">-O3</option>
    <option value="size">-Os</option>
    <option value="size-aggressive">-Oz</option>
  </select>
  <label class="chk" title="Intel assembly syntax (-masm=intel)">
    <input type="checkbox" id="intel"> intel
  </label>
  <button id="compile" title="Ctrl+Enter">Compile</button>
  <span id="status"></span>
  <div class="spacer"></div>
  <div class="hint"><kbd>Ctrl</kbd>+<kbd>Enter</kbd> compiles</div>
</header>
<main>
  <div class="left">
    <textarea id="src" spellcheck="false" wrap="off"
      placeholder="type C/C++ here..."></textarea>
  </div>
  <div class="right">
    <div class="tabs">
      <button class="tab" id="tab-asm"><span id="tab-asm-label">ASM</span><span class="dot"></span></button>
      <button class="tab" id="tab-ir"><span id="tab-ir-label">LLVM IR</span><span class="dot"></span></button>
      <button class="tab" id="tab-mca"><span id="tab-mca-label">MCA</span><span class="dot"></span></button>
      <button class="tab" id="tab-stderr"><span id="tab-stderr-label">stderr</span><span class="dot"></span></button>
    </div>
    <div class="panes">
      <section class="pane" id="pane-asm">
        <button class="copy" data-pane="asm" title="copy pane to clipboard">copy</button>
        <pre id="asm" class="ph"></pre>
      </section>
      <section class="pane" id="pane-ir">
        <button class="copy" data-pane="ir" title="copy pane to clipboard">copy</button>
        <pre id="ir" class="ph"></pre>
      </section>
      <section class="pane" id="pane-mca">
        <button class="copy" data-pane="mca" title="copy pane to clipboard">copy</button>
        <pre id="mca" class="ph"></pre>
      </section>
      <section class="pane" id="pane-stderr">
        <button class="copy" data-pane="stderr" title="copy pane to clipboard">copy</button>
        <pre id="stderr" class="ph"></pre>
      </section>
    </div>
  </div>
</main>
<script>
"use strict";
var PANES = ["asm", "ir", "mca", "stderr"];
var INITIAL_SOURCE = __POLYGLOT_INITIAL_SOURCE_JSON__;
var INITIAL_LANGUAGE = __POLYGLOT_INITIAL_LANGUAGE_JSON__;
var busy = false;

function $(id) { return document.getElementById(id); }

function setStatus(text, mood) {
  var el = $("status");
  el.textContent = text;
  el.className = mood || "";
}

function activate(name) {
  for (var i = 0; i < PANES.length; i++) {
    var p = PANES[i];
    $("pane-" + p).classList.toggle("active", p === name);
    $("tab-" + p).classList.toggle("active", p === name);
  }
}

function setPane(name, text) {
  $("#" + name).textContent = text;
  if (name === "stderr") {
    $("tab-stderr").classList.toggle("noteworthy", !!text);
  }
}

function copyPane(name) {
  var text = $("#" + name).textContent;
  var done = function (ok) {
    var btn = document.querySelector('.copy[data-pane="' + name + '"]');
    if (!btn) return;
    btn.textContent = ok ? "copied" : "copy failed";
    setTimeout(function () { btn.textContent = "copy"; }, 900);
  };
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(text).then(
      function () { done(true); },
      function () { done(copyFallback(text)); });
  } else {
    done(copyFallback(text));
  }
}

function copyFallback(text) {
  try {
    var ta = document.createElement("textarea");
    ta.value = text;
    document.body.appendChild(ta);
    ta.select();
    var ok = document.execCommand("copy");
    ta.remove();
    return ok;
  } catch (err) {
    return false;
  }
}

function compile() {
  if (busy) return;
  busy = true;
  $("compile").disabled = true;
  setStatus("compiling...", "");
  var payload = {
    source: $("src").value,
    language: $("lang").value,
    opt_level: $("opt").value,
    backend: $("backend").value,
    intel: $("intel").checked
  };
  fetch("/compile", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(payload)
  }).then(function (r) {
    return r.json().catch(function () {
      return { asm: "", ir: "", mca: "", stderr: "server returned HTTP " + r.status, ms: 0 };
    });
  }).then(function (d) {
    setPane("asm", d.asm || "");
    setPane("ir", d.ir || "");
    setPane("mca", d.mca || "");
    setPane("stderr", d.stderr || "");
    var failed = !d.asm && !d.ir && !!d.stderr;
    if (failed) activate("stderr");
    setStatus((failed ? "failed" : "done") + " - " + d.ms + " ms",
              failed ? "bad" : (d.stderr ? "warn" : "ok"));
  }).catch(function (err) {
    setPane("asm", ""); setPane("ir", ""); setPane("mca", "");
    setPane("stderr", "request failed: " + err);
    activate("stderr");
    setStatus("request failed", "bad");
  }).finally(function () {
    busy = false;
    $("compile").disabled = false;
  });
}

function syncIrLabel() {
  $("tab-ir-label").textContent = $("backend").value === "gcc" ? "GIMPLE" : "LLVM IR";
}

document.addEventListener("keydown", function (ev) {
  if ((ev.ctrlKey || ev.metaKey) && ev.key === "Enter") {
    ev.preventDefault();
    compile();
  }
});
document.addEventListener("DOMContentLoaded", function () {
  $("src").value = INITIAL_SOURCE;
  $("lang").value = INITIAL_LANGUAGE;
  $("compile").addEventListener("click", compile);
  $("backend").addEventListener("change", syncIrLabel);
  for (var i = 0; i < PANES.length; i++) {
    (function (p) {
      $("tab-" + p).addEventListener("click", function () { activate(p); });
      document.querySelector('.copy[data-pane="' + p + '"]')
        .addEventListener("click", function () { copyPane(p); });
    })(PANES[i]);
  }
  syncIrLabel();
  activate("asm");
  $("src").focus();
});
</script>
</body>
</html>
"""


def render_page(source, language):
    """Return the final HTML with the initial source/language embedded.

    Both values are JSON-encoded; '<' inside the JSON is escaped so a
    source containing '</script>' cannot terminate the inline script.
    """
    src_json = json.dumps(str(source)).replace("<", "\\u003c")
    lang_json = json.dumps(str(language))
    html = PAGE_TEMPLATE.replace("__POLYGLOT_INITIAL_SOURCE_JSON__", src_json)
    html = html.replace("__POLYGLOT_INITIAL_LANGUAGE_JSON__", lang_json)
    return html
