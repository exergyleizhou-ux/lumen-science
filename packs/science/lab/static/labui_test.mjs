#!/usr/bin/env node
/**
 * Drives shipped window.LabUI from app.js (escHtml, renderMarkdown, reduceSSE).
 * Run: node labui_test.mjs
 * No re-implementation — loads app.js source via vm up to LabUI export.
 */
import fs from "fs";
import path from "path";
import vm from "vm";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const appPath = path.join(__dirname, "app.js");
const src = fs.readFileSync(appPath, "utf8");
const cut = src.indexOf("/* ── 3. Global state");
if (cut < 0) {
  console.error("FAIL: could not find Global state marker in app.js");
  process.exit(1);
}

function loadLabUI() {
  const sandbox = {
    window: {},
    document: { getElementById: () => null },
    location: { pathname: "/" },
    console,
  };
  vm.createContext(sandbox);
  vm.runInContext(src.slice(0, cut), sandbox);
  if (!sandbox.window.LabUI) {
    throw new Error("window.LabUI not exported");
  }
  return sandbox.window.LabUI;
}

function assert(cond, msg) {
  if (!cond) throw new Error("ASSERT: " + msg);
}

function runOnce(runLabel) {
  console.log("--- " + runLabel + " ---");
  const L = loadLabUI();
  assert(typeof L.escHtml === "function", "escHtml is function");
  assert(typeof L.renderMarkdown === "function", "renderMarkdown is function");
  assert(typeof L.reduceSSE === "function", "reduceSSE is function");

  // escHtml
  const esc = L.escHtml('<script>alert(1)</script> & "x"');
  assert(esc.includes("&lt;script&gt;"), "escHtml escapes <script>: " + esc);
  assert(esc.includes("&amp;"), "escHtml escapes &: " + esc);
  assert(!esc.includes("<script>"), "escHtml no raw script tag");
  console.log("OK escHtml escapes tags/amp: " + esc.slice(0, 60));

  // markdown: bold, list, code — and HTML escaped before tags
  const md = L.renderMarkdown("**bold** and `code`\n\n- item one\n- item two");
  assert(md.includes("<strong>bold</strong>"), "md bold strong: " + md);
  assert(md.includes("<code>code</code>"), "md inline code: " + md);
  assert(md.includes("<li>item one</li>") || md.includes("<li>"), "md list li: " + md);
  console.log("OK renderMarkdown bold/list/code tags present");

  // fenced code block has copy button
  const fenced = L.renderMarkdown("```js\nconst x = 1;\n```");
  assert(fenced.includes("code-wrap"), "fenced code-wrap: " + fenced.slice(0, 120));
  assert(fenced.includes("code-copy"), "fenced copy btn");
  assert(fenced.includes("const x = 1"), "fenced body");
  console.log("OK fenced code has code-wrap + copy");

  // attachment path parse
  if (typeof L.parseAttachmentPaths === "function") {
    const paths = L.parseAttachmentPaths("[附件] data/a.csv\nhello @notes/b.txt");
    assert(paths.includes("data/a.csv"), "attach card path: " + JSON.stringify(paths));
    assert(paths.includes("notes/b.txt"), "at path: " + JSON.stringify(paths));
    console.log("OK parseAttachmentPaths");
  }

  // file templates if exported
  if (typeof L.fileTemplateContent === "function") {
    const py = L.fileTemplateContent("scripts/run.py");
    assert(py.includes("def main"), "py template: " + py.slice(0, 80));
    const md = L.fileTemplateContent("notes/x.md");
    assert(md.includes("#"), "md template");
    console.log("OK fileTemplateContent");
  }

  // mol path + content validation (shipped helpers)
  assert(typeof L.normalizeMolPath === "function", "normalizeMolPath");
  assert(L.normalizeMolPath("") === "molecules/structure.mol", "default mol path");
  assert(L.normalizeMolPath("foo").endsWith(".mol"), "adds .mol: " + L.normalizeMolPath("foo"));
  assert(L.normalizeMolPath("../x.sdf") === "x.sdf" || !L.normalizeMolPath("../x.sdf").includes(".."), "no ..");
  assert(typeof L.validateMolContent === "function", "validateMolContent");
  assert(L.validateMolContent("").ok === false, "empty mol invalid");
  assert(L.validateMolContent("  \n  ").ok === false, "whitespace invalid");
  const sampleMol = "\n\n\n  0  0  0  0  0  0  0  0  0  0999 V2000\nM  END\n";
  assert(L.validateMolContent(sampleMol).ok === true, "V2000 mol ok");
  assert(L.validateMolContent("ATOM      1  N   MET A   1").ok === true, "pdb heuristic");
  console.log("OK normalizeMolPath + validateMolContent");

  // compute job body builder
  assert(typeof L.buildComputeJobBody === "function", "buildComputeJobBody");
  const jb = L.buildComputeJobBody({
    host: "local",
    command: "echo hi",
    work_dir: "/tmp/x",
    timeout_sec: 120,
    output_globs: "a.txt, b.csv",
  });
  assert(jb.host === "local" && jb.command === "echo hi", "job host/cmd");
  assert(jb.work_dir === "/tmp/x", "job work_dir");
  assert(jb.timeout_sec === 120, "job timeout");
  assert(Array.isArray(jb.output_globs) && jb.output_globs.length === 2, "job globs: " + JSON.stringify(jb.output_globs));
  const jb2 = L.buildComputeJobBody({ host: "gpu1", command: "ls", timeout_sec: 99999 });
  assert(jb2.timeout_sec === 7200, "timeout capped at 7200");
  console.log("OK buildComputeJobBody");

  // LangGraph body + result formatters
  if (typeof L.buildLangGraphBody === "function") {
    const lg = L.buildLangGraphBody("  proj  ", "  hello  ");
    assert(lg.project_id === "proj" && lg.prompt === "hello", "langgraph body trim: " + JSON.stringify(lg));
    const empty = L.buildLangGraphBody("", "   ");
    assert(empty.prompt === "", "empty prompt stays empty");
    console.log("OK buildLangGraphBody");
  }
  if (typeof L.formatLangGraphResult === "function") {
    assert(L.formatLangGraphResult({ ok: true, result: "done" }) === "done", "lg result ok");
    assert(L.formatLangGraphResult({ ok: false, error: "nope" }) === "nope", "lg result err");
    assert(L.formatLangGraphResult({ ok: false }).indexOf("失败") >= 0, "lg result default err");
    console.log("OK formatLangGraphResult");
  }
  if (typeof L.linkifyPathTokens === "function") {
    const linked = L.linkifyPathTokens("see reports/notes.md and script.py");
    assert(linked.includes('data-path="reports/notes.md"'), "path token notes: " + linked);
    assert(linked.includes('data-path="script.py"'), "path token py: " + linked);
    assert(!linked.includes("<script"), "no raw script");
    console.log("OK linkifyPathTokens");
  }
  if (typeof L.linkifyLangGraphPaths === "function") {
    const block = L.linkifyLangGraphPaths("<li>reports/notes.md</li><h3>script.py</h3>");
    assert(block.includes('class="ws-ref"') && block.includes("reports/notes.md"), "lg li path: " + block);
    assert(block.includes("<h3>") && block.includes('data-path="script.py"'), "lg h3 path");
    console.log("OK linkifyLangGraphPaths");
  }
  if (typeof L.reduceLangGraphHistory === "function") {
    const a = L.reduceLangGraphHistory([], { id: "1", ts: 1, prompt: "p1", ok: true, result: "r1" }, 2);
    assert(a.length === 1 && a[0].prompt === "p1", "hist first: " + JSON.stringify(a));
    const b = L.reduceLangGraphHistory(a, { id: "2", ts: 2, prompt: "p2", ok: false, result: "r2" }, 2);
    assert(b.length === 2 && b[0].id === "2" && b[1].id === "1", "hist prepend");
    const c = L.reduceLangGraphHistory(b, { id: "3", ts: 3, prompt: "p3", ok: true, result: "r3" }, 2);
    assert(c.length === 2 && c[0].id === "3" && c[1].id === "2", "hist cap 2");
    console.log("OK reduceLangGraphHistory");
  }
  if (typeof L.truncateLangGraphHistoryResult === "function") {
    const long = "x".repeat(100);
    const t = L.truncateLangGraphHistoryResult(long, 20);
    assert(t.length < long.length && t.includes("截断"), "hist truncate: " + t.length);
    console.log("OK truncateLangGraphHistoryResult");
  }
  if (typeof L.filterLangGraphHistory === "function") {
    const src = [
      { id: "1", project_id: "a", prompt: "p1" },
      { id: "2", project_id: "b", prompt: "p2" },
      { id: "3", project_id: "a", prompt: "p3" },
      { id: "4", project_id: "", prompt: "p4" },
    ];
    const fa = L.filterLangGraphHistory(src, "a", "project");
    assert(fa.length === 2 && fa[0].id === "1" && fa[1].id === "3", "filter a: " + JSON.stringify(fa));
    const fall = L.filterLangGraphHistory(src, "a", "all");
    assert(fall.length === 4, "filter all");
    const empty = L.filterLangGraphHistory(src, "", "project");
    assert(empty.length === 1 && empty[0].id === "4", "filter empty slug");
    console.log("OK filterLangGraphHistory");
  }
  if (typeof L.buildLangGraphHistoryExport === "function") {
    const doc = L.buildLangGraphHistoryExport(
      [{ id: "x", ts: 1, project_id: "p", prompt: "hi", ok: true, result: "out" }],
      { project_id: "p", scope: "project", exported_at: "2026-01-01T00:00:00Z" }
    );
    assert(doc.kind === "lumen.langgraph.history" && doc.version === 1, "export kind");
    assert(doc.count === 1 && doc.entries[0].prompt === "hi", "export entries");
    assert(doc.scope === "project" && doc.project_id === "p", "export meta");
    console.log("OK buildLangGraphHistoryExport");
  }
  if (typeof L.buildLangGraphHistoryMarkdown === "function") {
    const md = L.buildLangGraphHistoryMarkdown(
      [{ id: "x", ts: 1700000000000, project_id: "p", prompt: "hello prompt", ok: true, result: "## LangGraph\nok" }],
      { project_id: "p", scope: "project", exported_at: "2026-01-01T00:00:00Z", json_path: "artifacts/x.json" }
    );
    assert(md.includes("# LangGraph 运行历史"), "md title");
    assert(md.includes("hello prompt") && md.includes("## LangGraph"), "md body");
    assert(md.includes("artifacts/x.json"), "md json path ref");
    assert(md.includes("成功"), "md status");
    console.log("OK buildLangGraphHistoryMarkdown");
  }
  if (typeof L.buildLangGraphRunNote === "function") {
    const note = L.buildLangGraphRunNote("my prompt", "result body here", {
      project_id: "slug-1",
      saved_at: "2026-01-02T00:00:00Z",
    });
    assert(note.includes("# LangGraph 运行笔记"), "run note title");
    assert(note.includes("my prompt") && note.includes("result body here"), "run note body");
    assert(note.includes("slug-1"), "run note project");
    console.log("OK buildLangGraphRunNote");
  }
  if (typeof L.mergeLangGraphHistoryImport === "function") {
    const bad = L.mergeLangGraphHistoryImport([], { kind: "nope" }, 10);
    assert(bad.error, "import rejects bad kind");
    const good = L.mergeLangGraphHistoryImport(
      [{ id: "1", prompt: "old", ok: true, result: "a" }],
      {
        kind: "lumen.langgraph.history",
        entries: [
          { id: "1", prompt: "dup", ok: true, result: "a" },
          { id: "2", prompt: "new", ok: true, result: "b", ts: 9 },
        ],
      },
      10
    );
    assert(!good.error && good.added === 1, "import added: " + JSON.stringify(good));
    assert(good.list.some((e) => e.id === "2"), "import has id 2");
    console.log("OK mergeLangGraphHistoryImport");
  }

  // XSS: raw HTML must be escaped, not executed as tags
  const xss = L.renderMarkdown("<img onerror=alert(1) src=x> **ok**");
  assert(!xss.includes("<img"), "md no raw img tag after escape: " + xss);
  assert(xss.includes("&lt;img") || xss.includes("&lt;"), "md escaped lt: " + xss);
  assert(xss.includes("<strong>ok</strong>"), "md still applies bold after escape");
  console.log("OK HTML escaped before markdown (no raw img)");

  // reduceSSE tool id merge
  let s = L.reduceSSE(null, {
    kind: "tool_dispatch",
    tool: { id: "tool-42", name: "bash", args: '{"cmd":"ls"}' },
  });
  assert(s.tools["tool-42"], "dispatch creates tools[tool-42]");
  assert(s.tools["tool-42"].status === "running", "dispatch status running");
  assert(s.tools["tool-42"].name === "bash", "dispatch name bash");
  console.log("OK tool_dispatch keyed by id status=running");

  s = L.reduceSSE(s, {
    kind: "tool_result",
    tool: { id: "tool-42", name: "bash", output: "FINAL_OUT", err: "" },
  });
  assert(s.tools["tool-42"].status === "done", "result status done");
  assert(s.tools["tool-42"].output === "FINAL_OUT", "result output replace not double: " + s.tools["tool-42"].output);
  // second tool does not clobber first
  s = L.reduceSSE(s, {
    kind: "tool_dispatch",
    tool: { id: "tool-99", name: "read", args: "a.md" },
  });
  assert(s.tools["tool-42"].output === "FINAL_OUT", "tool id merge keeps tool-42");
  assert(s.tools["tool-99"].status === "running", "tool-99 running");
  console.log("OK tool_result merges by id; second tool independent");

  // approval shape
  s = L.reduceSSE(s, {
    kind: "approval_request",
    id: "appr-7",
    tool: "shell",
    summary: "run rm -rf /tmp/x",
    args: '{"command":"rm -rf /tmp/x"}',
  });
  assert(Array.isArray(s.approvals) && s.approvals.length >= 1, "approvals array");
  const ap = s.approvals[s.approvals.length - 1];
  assert(ap.id === "appr-7", "approval id");
  assert(ap.tool === "shell", "approval tool");
  assert(ap.summary && ap.summary.indexOf("rm") >= 0, "approval summary");
  assert(ap.args && ap.args.indexOf("command") >= 0, "approval args retained");
  console.log("OK approval_request shape {id,tool,summary,args}: " + JSON.stringify(ap));

  // tool args preview for OCS-style cards
  if (typeof L.formatToolArgsPreview === "function") {
    const bashPrev = L.formatToolArgsPreview('{"command":"ls -la /tmp"}');
    assert(bashPrev.indexOf("ls") >= 0, "preview bash command: " + bashPrev);
    const pathPrev = L.formatToolArgsPreview('{"path":"notes/a.md","content":"x"}');
    assert(pathPrev.indexOf("notes/a.md") >= 0, "preview write path: " + pathPrev);
    console.log("OK formatToolArgsPreview");
  }

  // text + reasoning accumulate
  s = L.reduceSSE(s, { kind: "text", text: "Hello " });
  s = L.reduceSSE(s, { kind: "text", text: "world" });
  assert(s.text === "Hello world", "text accumulate: " + s.text);
  s = L.reduceSSE(s, { kind: "reasoning", text: "think" });
  assert(s.reasoning === "think", "reasoning: " + s.reasoning);
  console.log("OK text/reasoning accumulate");

  console.log("PASS " + runLabel + " all assertions");
  return true;
}

let fails = 0;
try {
  runOnce("run-1");
} catch (e) {
  console.error(e.message || e);
  fails++;
}
try {
  runOnce("run-2");
} catch (e) {
  console.error(e.message || e);
  fails++;
}

if (fails) {
  console.error("FAIL labui_test.mjs exits with " + fails + " failed run(s)");
  process.exit(1);
}
console.log("PASS labui_test.mjs (2 runs)");
process.exit(0);
