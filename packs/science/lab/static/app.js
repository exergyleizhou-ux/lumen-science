/* Lumen Science Lab — OCS-style static workbench
   SSE kinds: text, reasoning, tool_dispatch, tool_result,
   approval_request, error, turn_started, turn_done
   Contract: internal/event/event.go + approval.go */

/* ── 1. Utilities ── */
const $ = (id) => document.getElementById(id);

const API_BASE = (function () {
  const p = location.pathname || "";
  if (p.startsWith("/lumen-lab")) return "/lumen-lab";
  return "";
})();

function labPath(path) {
  return API_BASE + path;
}

function escHtml(s) {
  return String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

function fmtSize(bytes) {
  if (!bytes || bytes < 1024) return (bytes || 0) + "B";
  if (bytes < 1048576) return (bytes / 1024).toFixed(1) + "KB";
  return (bytes / 1048576).toFixed(1) + "MB";
}

function fileIcon(name) {
  const ext = String(name).split(".").pop().toLowerCase();
  const map = { md: "📝", py: "🐍", r: "📊", json: "📋", csv: "📈", png: "🖼", jpg: "🖼", jpeg: "🖼", svg: "🖼", pdf: "📕", html: "🌐", css: "🎨", js: "📜", txt: "📄", log: "📋", yml: "⚙", yaml: "⚙", toml: "⚙" };
  return map[ext] || "📄";
}

async function api(path, opts = {}) {
  const res = await fetch(labPath(path), {
    headers: { "Content-Type": "application/json", ...(opts.headers || {}) },
    ...opts,
  });
  const text = await res.text();
  let data;
  try { data = JSON.parse(text); } catch (_) { data = { raw: text }; }
  if (!res.ok) throw new Error(data.error || res.statusText);
  return data;
}

/* ── 2. Pure functions (window.LabUI for testing) ── */

function renderMarkdown(src) {
  // Safe subset: escHtml first, then regex → HTML
  var html = escHtml(src);

  // Fenced code blocks (before inline code) — wrap with copy button
  html = html.replace(/```(\w*)\n([\s\S]*?)```/g, function (_, lang, code) {
    var cls = lang ? ' class="language-' + escHtml(lang) + '"' : "";
    var label = lang ? escHtml(lang) : "code";
    return '<div class="code-wrap"><div class="code-hd"><span class="code-lang">' + label +
      '</span><button type="button" class="btn sm code-copy">复制</button></div>' +
      "<pre><code" + cls + ">" + code.trimEnd() + "</code></pre></div>";
  });

  // Inline code
  html = html.replace(/`([^`]+)`/g, "<code>$1</code>");

  // Bold
  html = html.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");

  // Headings
  html = html.replace(/^### (.+)$/gm, "<h3>$1</h3>");
  html = html.replace(/^## (.+)$/gm, "<h2>$1</h2>");
  html = html.replace(/^# (.+)$/gm, "<h1>$1</h1>");

  // Unordered list items
  html = html.replace(/^[\-\*] (.+)$/gm, "<li>$1</li>");
  // Ordered list items
  html = html.replace(/^\d+\. (.+)$/gm, "<li>$1</li>");

  // Links [text](url) — http/https or site-relative /
  html = html.replace(/\[([^\]]+)\]\(((?:https?:\/\/|\/)[^)\s]+)\)/g, '<a href="$2" target="_blank" rel="noopener">$1</a>');

  // Horizontal rules
  html = html.replace(/^---$/gm, "<hr>");

  // Blockquotes
  html = html.replace(/^&gt; (.+)$/gm, "<blockquote>$1</blockquote>");

  // Paragraphs: split on double newline
  html = html.replace(/\n\n/g, "</p><p>");
  html = "<p>" + html + "</p>";

  // Clean up empty paragraphs and fix list wrapping
  html = html.replace(/<p><\/p>/g, "");
  html = html.replace(/<p>(<li>[\s\S]*?<\/li>)<\/p>/g, "<ul>$1</ul>");
  html = html.replace(/<p>(<h[123][\s\S]*?<\/h[123]>)<\/p>/g, "$1");
  html = html.replace(/<p>(<div class="code-wrap">[\s\S]*?<\/div>)<\/p>/g, "$1");
  html = html.replace(/<p>(<pre>[\s\S]*?<\/pre>)<\/p>/g, "$1");
  html = html.replace(/<p>(<blockquote>[\s\S]*?<\/blockquote>)<\/p>/g, "$1");
  html = html.replace(/<p>(<hr>)<\/p>/g, "$1");
  html = html.replace(/<p>\s*<\/p>/g, "");

  // @file / bare paths → clickable (after esc+md so paths are text)
  html = linkifyWorkspacePaths(html);
  return html;
}

function bindCodeCopy(root) {
  if (!root) return;
  root.querySelectorAll(".code-copy").forEach(function (btn) {
    if (btn._bound) return;
    btn._bound = true;
    btn.addEventListener("click", function () {
      var wrap = btn.closest(".code-wrap");
      var code = wrap && wrap.querySelector("code");
      var text = code ? code.textContent : "";
      if (!text) return;
      var done = function () {
        btn.textContent = "已复制";
        setTimeout(function () { btn.textContent = "复制"; }, 1200);
      };
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text).then(done).catch(function () {
          // fallback
          try {
            var ta = document.createElement("textarea");
            ta.value = text;
            document.body.appendChild(ta);
            ta.select();
            document.execCommand("copy");
            document.body.removeChild(ta);
            done();
          } catch (_) {}
        });
      }
    });
  });
}

/** Mark workspace-like paths for click-to-preview (operates on safe HTML). */
function linkifyWorkspacePaths(html) {
  // @path/to/file
  html = html.replace(/(^|[^"'>])@([\w./\-]+\.[\w]+)/g, function (_, pre, p) {
    return pre + '<a href="#" class="ws-ref" data-path="' + p + '">@' + p + "</a>";
  });
  // [产物] path or [附件] path
  html = html.replace(/\[(产物|附件)\]\s*([\w./\-]+)/g, function (_, kind, p) {
    return '[<a href="#" class="ws-ref" data-path="' + p + '">' + kind + " " + p + "</a>]";
  });
  return html;
}

/**
 * Linkify relative file paths inside simple text nodes of already-safe HTML
 * (e.g. LangGraph inventory lines / headings after renderMarkdown).
 * Only matches common text extensions; skips attributes/existing tags.
 */
function linkifyPathTokens(text) {
  text = String(text || "");
  return text.replace(
    /\b([\w][\w./\-]*\.(?:md|txt|py|ipynb|csv|json|ya?ml|go|js|ts|tsx|r|sh|toml|ini|log|tex|bib|html|css|rst|jl|pdf))\b/g,
    function (path) {
      return '<a href="#" class="ws-ref" data-path="' + path + '">' + path + "</a>";
    }
  );
}

function linkifyLangGraphPaths(html) {
  html = String(html || "");
  // Only rewrite plain text inside common block tags produced by renderMarkdown.
  html = html.replace(/<(li|h[1-3]|p|code)(\s[^>]*)?>([^<]*)<\/\1>/g, function (_, tag, attrs, content) {
    attrs = attrs || "";
    return "<" + tag + attrs + ">" + linkifyPathTokens(content) + "</" + tag + ">";
  });
  return html;
}

function bindWorkspaceRefs(root) {
  if (!root) return;
  root.querySelectorAll("a.ws-ref").forEach(function (a) {
    if (a._bound) return;
    a._bound = true;
    a.addEventListener("click", function (e) {
      e.preventDefault();
      var p = a.getAttribute("data-path");
      if (!p) return;
      var filesTab = document.querySelector('.insp-tab[data-pane="files"]');
      if (filesTab) filesTab.click();
      previewFile(p, "");
      openPreviewModal(p);
    });
  });
}

function openPreviewModal(path) {
  var modal = $("previewModal");
  var body = $("previewModalBody");
  var title = $("previewModalTitle");
  if (!modal || !body) return;
  if (title) title.textContent = path || "预览";
  modal.hidden = false;
  // clone filePreview content if available
  var fp = $("filePreview");
  body.innerHTML = fp && fp.innerHTML ? fp.innerHTML : '<div class="hint">加载中…</div>';
  if (!fp || !fp.innerHTML) {
    previewFile(path, "").then(function () {
      body.innerHTML = ($("filePreview") && $("filePreview").innerHTML) || "";
    }).catch(function () {});
  }
}

function closePreviewModal() {
  var modal = $("previewModal");
  if (modal) modal.hidden = true;
}

function reduceSSE(state, ev) {
  if (!state || !state.tools) state = { text: "", reasoning: "", tools: {}, approvals: [], errors: [], turn: null };
  state.text = state.text || "";
  state.reasoning = state.reasoning || "";
  state.tools = state.tools || {};
  state.approvals = state.approvals || [];
  state.errors = state.errors || [];
  var kind = ev.kind || "";

  switch (kind) {
    case "text":
      state.text += (ev.text || "");
      break;
    case "reasoning":
      state.reasoning += (ev.text || "");
      break;
    case "tool_dispatch":
      if (ev.tool && ev.tool.id) {
        state.tools[ev.tool.id] = {
          id: ev.tool.id,
          name: ev.tool.name || "",
          args: ev.tool.args || "",
          description: ev.tool.description || "",
          output: "",
          err: "",
          status: "running",
          readOnly: ev.tool.read_only || false,
          parentId: ev.tool.parent_id || "",
        };
      }
      break;
    case "tool_result":
      // Backend emits full Output once (agent.go) — replace, don't append
      // (progress may have streamed partial text into output already).
      if (ev.tool && ev.tool.id) {
        var existing = state.tools[ev.tool.id];
        if (existing) {
          if (ev.tool.name) existing.name = ev.tool.name;
          if (ev.tool.args) existing.args = ev.tool.args;
          // Full snapshot from agent — overwrite progress-streamed partials
          if (Object.prototype.hasOwnProperty.call(ev.tool, "output")) existing.output = ev.tool.output || "";
          existing.err = ev.tool.err || "";
          existing.status = ev.tool.err ? "error" : "done";
          if (ev.tool.truncated) existing.truncated = true;
          if (ev.tool.parent_id) existing.parentId = ev.tool.parent_id;
        } else {
          state.tools[ev.tool.id] = {
            id: ev.tool.id,
            name: ev.tool.name || "",
            args: ev.tool.args || "",
            output: ev.tool.output || "",
            err: ev.tool.err || "",
            status: ev.tool.err ? "error" : "done",
            readOnly: ev.tool.read_only || false,
            truncated: !!ev.tool.truncated,
            parentId: ev.tool.parent_id || "",
          };
        }
      }
      break;
    case "tool_progress":
      // Optional: update tool progress text
      if (ev.tool && ev.tool.id && ev.text) {
        var tp = state.tools[ev.tool.id];
        if (tp) tp.output = (tp.output || "") + ev.text;
      }
      break;
    case "approval_request":
      state.approvals.push({
        id: ev.id,
        tool: ev.tool,
        summary: ev.summary,
        args: ev.args || "",
      });
      break;
    case "error":
      state.errors.push(ev.text || "未知错误");
      break;
    case "turn_started":
      state.turn = "started";
      break;
    case "turn_done":
      state.turn = "done";
      break;
    case "usage":
      state.usage = {
        input_tokens: ev.input_tokens || ev.prompt_tokens || (ev.usage && ev.usage.input_tokens) || 0,
        output_tokens: ev.output_tokens || ev.completion_tokens || (ev.usage && ev.usage.output_tokens) || 0,
        total_tokens: ev.total_tokens || (ev.usage && ev.usage.total_tokens) || 0,
        raw: ev,
      };
      if (!state.usage.total_tokens && (state.usage.input_tokens || state.usage.output_tokens)) {
        state.usage.total_tokens = (state.usage.input_tokens || 0) + (state.usage.output_tokens || 0);
      }
      break;
    case "phase":
    case "notice":
    case "perf":
    case "file_preview":
      // Optional: silently accept
      break;
  }
  return state;
}

function parseAttachmentPaths(text) {
  var paths = [];
  var re = /\[(附件|产物)\]\s*([^\n\r]+)/g;
  var m;
  while ((m = re.exec(text || "")) !== null) {
    var p = m[2].trim();
    if (p && paths.indexOf(p) < 0) paths.push(p);
  }
  // also @path
  re = /@([\w./\-]+\.[\w]+)/g;
  while ((m = re.exec(text || "")) !== null) {
    if (paths.indexOf(m[1]) < 0) paths.push(m[1]);
  }
  return paths;
}

function fileTemplateContent(path) {
  var lower = (path || "").toLowerCase();
  var base = (path || "").split("/").pop() || path || "file";
  if (lower.endsWith(".md") || lower.endsWith(".markdown")) {
    return "# " + base.replace(/\.md$/i, "") + "\n\n> 草稿\n\n## 目标\n\n- \n\n## 笔记\n\n";
  }
  if (lower.endsWith(".py")) {
    return '#!/usr/bin/env python3\n"""' + base + '"""\n\ndef main():\n    print("ok")\n\n\nif __name__ == "__main__":\n    main()\n';
  }
  if (lower.endsWith(".r")) {
    return "# " + base + "\nmessage(\"ok\")\n";
  }
  if (lower.endsWith(".sh")) {
    return "#!/usr/bin/env bash\nset -euo pipefail\necho ok\n";
  }
  if (lower.endsWith(".csv")) {
    return "id,name,value\n1,sample,0\n";
  }
  if (lower.endsWith(".json")) {
    return "{\n  \"title\": \"" + base.replace(/\.json$/i, "") + "\",\n  \"items\": []\n}\n";
  }
  return "";
}

/** Normalize workspace path for MOL/SDF saves (pure). */
function normalizeMolPath(path) {
  var p = String(path || "").trim().replace(/^\/+/, "").replace(/\.\./g, "");
  if (!p) p = "molecules/structure.mol";
  if (!/\.(mol|sdf|pdb|cif)$/i.test(p)) p = p + ".mol";
  return p;
}

/**
 * Validate MOL/SDF-ish content before save. Returns {ok, reason}.
 * Accepts: V2000/V3000 molfile, M  END, or multi-line atom block heuristic.
 */
function validateMolContent(content) {
  var t = String(content || "").trim();
  if (!t) return { ok: false, reason: "empty" };
  if (t.length > 8 * 1024 * 1024) return { ok: false, reason: "too_large" };
  if (/M\s+END/i.test(t) || /V2000|V3000/i.test(t) || /\$\$\$\$/.test(t)) {
    return { ok: true, reason: "molfile" };
  }
  // PDB heuristic
  if (/^(ATOM|HETATM|HEADER)\s/m.test(t)) return { ok: true, reason: "pdb" };
  // CIF heuristic
  if (/^data_/m.test(t) || /_atom_site\./.test(t)) return { ok: true, reason: "cif" };
  // Soft accept multi-line structure text (user pasted fragment)
  var lines = t.split(/\r?\n/).filter(function (l) { return l.trim(); });
  if (lines.length >= 3) return { ok: true, reason: "text_block" };
  return { ok: false, reason: "not_structure" };
}

/** Build compute job POST body from form fields (pure). */
function buildComputeJobBody(fields) {
  fields = fields || {};
  var host = String(fields.host || "").trim();
  var command = String(fields.command || "").trim();
  var workDir = String(fields.work_dir || fields.workDir || "").trim();
  var timeoutSec = parseInt(fields.timeout_sec != null ? fields.timeout_sec : fields.timeoutSec, 10);
  if (!timeoutSec || timeoutSec < 1) timeoutSec = 600;
  if (timeoutSec > 7200) timeoutSec = 7200;
  var globs = fields.output_globs || fields.globs || [];
  if (typeof globs === "string") {
    globs = globs.split(",").map(function (s) { return s.trim(); }).filter(Boolean);
  }
  if (!Array.isArray(globs)) globs = [];
  var body = { host: host, command: command, timeout_sec: timeoutSec, output_globs: globs };
  if (workDir) body.work_dir = workDir;
  return body;
}

/** Pure helper: JSON body for POST /api/lab/langgraph/run */
function buildLangGraphBody(projectId, prompt) {
  return {
    project_id: String(projectId || "").trim(),
    prompt: String(prompt || "").trim(),
  };
}

/** Pure helper: format langgraph API response for the inspector pane. */
function formatLangGraphResult(resp) {
  resp = resp || {};
  if (resp.ok) {
    return String(resp.result != null ? resp.result : "");
  }
  var err = String(resp.error || "LangGraph 运行失败").trim();
  return err || "LangGraph 运行失败";
}

var LG_HISTORY_MAX = 40;
var LG_HISTORY_RESULT_MAX = 8000;
// "project" = only active project; "all" = every slug
var langGraphHistoryScope = "project";

/** Pure: truncate stored result to keep localStorage small. */
function truncateLangGraphHistoryResult(result, maxChars) {
  maxChars = maxChars || LG_HISTORY_RESULT_MAX;
  var s = String(result == null ? "" : result);
  if (s.length <= maxChars) return s;
  return s.slice(0, maxChars) + "\n…(历史截断)";
}

/**
 * Pure: prepend a history entry and cap list length.
 * entry: { id, ts, project_id, prompt, ok, result }
 */
function reduceLangGraphHistory(list, entry, max) {
  max = max || LG_HISTORY_MAX;
  var out = Array.isArray(list) ? list.slice() : [];
  if (entry && typeof entry === "object") {
    out.unshift({
      id: String(entry.id || ("lg_" + Date.now())),
      ts: Number(entry.ts) || Date.now(),
      project_id: String(entry.project_id || ""),
      prompt: String(entry.prompt || "").slice(0, 500),
      ok: !!entry.ok,
      result: truncateLangGraphHistoryResult(entry.result, LG_HISTORY_RESULT_MAX),
    });
  }
  return out.slice(0, max);
}

/**
 * Pure: filter history by project slug.
 * scope: "all" | "project"
 * When scope is project and projectId is empty, returns entries with empty project_id only.
 */
function filterLangGraphHistory(list, projectId, scope) {
  list = Array.isArray(list) ? list : [];
  if (scope === "all") return list.slice();
  var pid = String(projectId || "").trim();
  return list.filter(function (item) {
    var ip = String((item && item.project_id) || "").trim();
    return ip === pid;
  });
}

/**
 * Pure: build workspace export document for LangGraph history.
 * meta: { project_id, scope, exported_at? }
 */
function buildLangGraphHistoryExport(list, meta) {
  meta = meta || {};
  list = Array.isArray(list) ? list : [];
  var entries = list.map(function (item) {
    item = item || {};
    return {
      id: String(item.id || ""),
      ts: Number(item.ts) || 0,
      project_id: String(item.project_id || ""),
      prompt: String(item.prompt || ""),
      ok: !!item.ok,
      result: String(item.result || ""),
    };
  });
  return {
    kind: "lumen.langgraph.history",
    version: 1,
    exported_at: meta.exported_at || new Date().toISOString(),
    project_id: String(meta.project_id || ""),
    scope: String(meta.scope || "all"),
    count: entries.length,
    entries: entries,
  };
}

/**
 * Pure: single-run note Markdown (current result → workspace file).
 * meta: { project_id, saved_at?, prompt? }
 */
function buildLangGraphRunNote(prompt, result, meta) {
  meta = meta || {};
  var lines = [];
  lines.push("# LangGraph 运行笔记");
  lines.push("");
  lines.push("- 保存时间: " + String(meta.saved_at || new Date().toISOString()));
  lines.push("- 课题: " + (meta.project_id ? String(meta.project_id) : "（未指定）"));
  lines.push("");
  lines.push("## 提示词");
  lines.push("");
  lines.push(String(prompt || "_(空)_").trim() || "_(空)_");
  lines.push("");
  lines.push("## 结果");
  lines.push("");
  var body = String(result || "_(无)_");
  if (body.length > 12000) {
    body = body.slice(0, 12000) + "\n\n…(笔记截断)";
  }
  lines.push(body);
  lines.push("");
  return lines.join("\n");
}

/**
 * Pure: human-readable Markdown summary of history for workspace notes.
 * meta: { project_id, scope, exported_at?, json_path? }
 */
function buildLangGraphHistoryMarkdown(list, meta) {
  meta = meta || {};
  list = Array.isArray(list) ? list : [];
  var lines = [];
  lines.push("# LangGraph 运行历史");
  lines.push("");
  lines.push("- 导出时间: " + String(meta.exported_at || new Date().toISOString()));
  lines.push("- 课题: " + (meta.project_id ? String(meta.project_id) : "（未指定）"));
  lines.push("- 范围: " + String(meta.scope || "all"));
  lines.push("- 条数: " + list.length);
  if (meta.json_path) {
    lines.push("- JSON: `" + String(meta.json_path) + "`");
  }
  lines.push("");
  if (!list.length) {
    lines.push("_（无记录）_");
    lines.push("");
    return lines.join("\n");
  }
  list.forEach(function (item, i) {
    item = item || {};
    var when = "";
    try {
      when = item.ts ? new Date(Number(item.ts)).toISOString() : "";
    } catch (_) {
      when = String(item.ts || "");
    }
    var status = item.ok ? "成功" : "失败";
    lines.push("## " + (i + 1) + ". " + status + (item.project_id ? " · `" + item.project_id + "`" : ""));
    lines.push("");
    if (when) lines.push("- 时间: " + when);
    if (item.id) lines.push("- id: `" + item.id + "`");
    lines.push("");
    lines.push("### 提示词");
    lines.push("");
    lines.push(String(item.prompt || "_(空)_"));
    lines.push("");
    lines.push("### 结果");
    lines.push("");
    var result = String(item.result || "_(无)_");
    // Cap very long results in MD export for readability
    if (result.length > 6000) {
      result = result.slice(0, 6000) + "\n\n…(Markdown 导出截断)";
    }
    lines.push(result);
    lines.push("");
    lines.push("---");
    lines.push("");
  });
  return lines.join("\n");
}

/**
 * Pure: merge imported export document into existing history list.
 * Returns { list, added } or { error }. Dedupes by id; newest ts first.
 */
function mergeLangGraphHistoryImport(existing, doc, max) {
  max = max || LG_HISTORY_MAX;
  if (!doc || doc.kind !== "lumen.langgraph.history" || !Array.isArray(doc.entries)) {
    return { error: "无效的历史导出文件（需要 kind=lumen.langgraph.history）" };
  }
  var byId = {};
  (Array.isArray(existing) ? existing : []).forEach(function (e) {
    if (!e) return;
    var id = String(e.id || "");
    if (!id) return;
    byId[id] = e;
  });
  var added = 0;
  doc.entries.forEach(function (item) {
    if (!item) return;
    var id = String(item.id || ("imp_" + (item.ts || Date.now()) + "_" + Math.random().toString(36).slice(2, 7)));
    if (byId[id]) return;
    byId[id] = {
      id: id,
      ts: Number(item.ts) || Date.now(),
      project_id: String(item.project_id || ""),
      prompt: String(item.prompt || "").slice(0, 500),
      ok: !!item.ok,
      result: truncateLangGraphHistoryResult(item.result, LG_HISTORY_RESULT_MAX),
    };
    added++;
  });
  var list = Object.keys(byId).map(function (k) { return byId[k]; });
  list.sort(function (a, b) {
    return (Number(b.ts) || 0) - (Number(a.ts) || 0);
  });
  return { list: list.slice(0, max), added: added };
}

/** One-line tool args for OCS-style tool card headers. */
function formatToolArgsPreview(args, maxLen) {
  maxLen = maxLen || 72;
  if (args == null || args === "") return "";
  var s = typeof args === "string" ? args : "";
  if (!s && typeof args === "object") {
    try { s = JSON.stringify(args); } catch (_) { s = String(args); }
  }
  s = String(s).replace(/\s+/g, " ").trim();
  if (!s) return "";
  try {
    var o = JSON.parse(s);
    if (o && typeof o === "object") {
      if (o.command) s = String(o.command);
      else if (o.path) s = String(o.path) + (o.content != null ? " (write)" : "");
      else if (o.query) s = String(o.query);
      else if (o.pattern) s = String(o.pattern);
      else if (o.url) s = String(o.url);
      else {
        var keys = Object.keys(o);
        if (keys.length === 1) s = keys[0] + "=" + String(o[keys[0]]).slice(0, 40);
        else s = keys.slice(0, 3).join(", ") + (keys.length > 3 ? "…" : "");
      }
    }
  } catch (_) {}
  s = String(s).replace(/\s+/g, " ").trim();
  if (s.length > maxLen) s = s.slice(0, maxLen - 1) + "…";
  return s;
}

window.LabUI = {
  escHtml: escHtml,
  renderMarkdown: renderMarkdown,
  reduceSSE: reduceSSE,
  formatToolArgsPreview: formatToolArgsPreview,
  parseAttachmentPaths: parseAttachmentPaths,
  fileTemplateContent: fileTemplateContent,
  normalizeMolPath: normalizeMolPath,
  validateMolContent: validateMolContent,
  buildComputeJobBody: buildComputeJobBody,
  buildLangGraphBody: buildLangGraphBody,
  formatLangGraphResult: formatLangGraphResult,
  linkifyPathTokens: linkifyPathTokens,
  linkifyLangGraphPaths: linkifyLangGraphPaths,
  truncateLangGraphHistoryResult: truncateLangGraphHistoryResult,
  reduceLangGraphHistory: reduceLangGraphHistory,
  filterLangGraphHistory: filterLangGraphHistory,
  buildLangGraphHistoryExport: buildLangGraphHistoryExport,
  buildLangGraphHistoryMarkdown: buildLangGraphHistoryMarkdown,
  buildLangGraphRunNote: buildLangGraphRunNote,
  mergeLangGraphHistoryImport: mergeLangGraphHistoryImport,
};

/* ── 3. Global state ── */
var activeProject = null;
var threads = []; // {id,title,turn_count} from API sessions
var activeThread = "";
var currentAbort = null;
var fileCwd = ".";
var sseState = null; // per-turn SSE accumulator
var turnTasks = []; // this-turn tools for tasks pane
var skillsCache = [];
var atMenuState = { active: false, start: -1, items: [], idx: 0 };

/* ── 4. API functions ── */

async function refreshHealth() {
  try {
    var h = await api("/api/lab/health");
    var pack = h.research_pack || {};
    var f = h.fleet || {};
    $("packBadge").textContent = pack.healthy ? (pack.domain_tools || 0) + " tools · " + (pack.skills || 0) + " skills" : "未安装 Research Pack";
    $("fleetBadge").textContent = "⚡ " + (f.connected_total || 0) + "/" + (f.cs_domains || 0) + " fleet";
    $("modeHint").textContent = h.science_mode || "hybrid";
    var ib = $("inspectorBody");
    if (ib) {
      var rows = [
        '<div class="sr"><span class="sr k">状态</span><span class="sr v ok">● 在线</span></div>',
        '<div class="sr"><span class="sr k">版本</span><span class="sr v">' + escHtml(h.version || "dev") + '</span></div>',
        '<div class="sr"><span class="sr k">模式</span><span class="sr v">' + escHtml(h.science_mode || "hybrid") + '</span></div>',
        '<div class="sr-div"></div>',
        '<div class="sr"><span class="sr k">Research</span><span class="sr v ' + (pack.healthy ? "ok" : "") + '">' + (pack.healthy ? "✓" : "✗") + " " + (pack.domain_tools || 0) + ' tools</span></div>',
        '<div class="sr"><span class="sr k">CS fleet</span><span class="sr v">' + (f.cs_connected || 0) + "/" + (f.cs_domains || 0) + '</span></div>',
        '<div class="sr"><span class="sr k">原生 fleet</span><span class="sr v">' + (f.lumen_native || 0) + '</span></div>',
        '<div class="sr-div"></div>',
        '<div class="sr"><span class="sr k">模型</span><span class="sr v">' + escHtml((h.provider && h.provider.masked) || "—") + '</span></div>',
      ];
      var cap = h.capacity || {};
      rows.push('<div class="sr-div"></div>');
      rows.push('<div class="sr"><span class="sr k">并发回合</span><span class="sr v">' + (cap.turns_active || 0) + "/" + (cap.turns_capacity || "—") + "</span></div>");
      rows.push('<div class="sr"><span class="sr k">审批累计</span><span class="sr v">' + (cap.approvals_total || 0) + "</span></div>");
      rows.push('<div class="sr-div"></div>');
      rows.push('<div class="set-k soft" style="margin:6px 0 4px">自检清单</div>');
      var checks = buildReadinessChecks(h, pack, f);
      checks.forEach(function (c) {
        rows.push('<div class="sr ready-row"><span class="sr k">' + escHtml(c.label) + '</span><span class="sr v ' + (c.ok ? "ok" : "") + '">' + (c.ok ? "✓" : "·") + " " + escHtml(c.detail) + "</span></div>");
      });
      if (activeProject) {
        rows.push('<div class="sr-div"></div>');
        rows.push('<div class="sr"><span class="sr k">课题</span><span class="sr v">' + escHtml(activeProject.title || activeProject.slug) + "</span></div>");
        rows.push('<div class="sr" id="wsStatsRow"><span class="sr k">工作区</span><span class="sr v" id="wsStatsVal">加载中…</span></div>');
      }
      ib.innerHTML = rows.join("");
      if (activeProject) loadWorkspaceStats();
    }
    return h;
  } catch (e) {
    var ib2 = $("inspectorBody");
    if (ib2) ib2.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

async function loadWorkspaceStats() {
  var el = $("wsStatsVal");
  if (!el || !activeProject) return;
  try {
    var s = await api("/api/lab/files/stats?project_id=" + activeProject.slug);
    el.textContent = (s.files || 0) + " 文件 · " + (s.dirs || 0) + " 目录 · " + fmtSize(s.bytes || 0) +
      (s.truncated ? " …" : "");
  } catch (e) {
    el.textContent = "—";
  }
}


function buildReadinessChecks(h, pack, f) {
  pack = pack || {};
  f = f || {};
  var prov = (h && h.provider) || {};
  var ket = (h && h.ketcher) || {};
  var jup = (h && h.jupyter) || {};
  var oo = (h && h.onlyoffice) || {};
  var lg = (h && h.langgraph) || {};
  return [
    { label: "Lab 在线", ok: true, detail: "v" + ((h && h.version) || "dev") },
    { label: "模型", ok: !!prov.set, detail: prov.set ? (prov.masked || "已配置") : "未配置" },
    { label: "Research Pack", ok: !!pack.healthy, detail: pack.healthy ? ((pack.domain_tools || 0) + " tools") : "未安装/不完整" },
    { label: "Fleet", ok: (f.connected_total || 0) > 0 || (f.lumen_native || 0) > 0, detail: (f.connected_total || 0) + " 连接 · native " + (f.lumen_native || 0) },
    { label: "同域 Ketcher", ok: !!ket.same_origin, detail: ket.same_origin ? "✓ /ketcher/" : "未部署（可用 MOL 路径）" },
    { label: "Jupyter", ok: !!jup.available, detail: jup.available ? "可用" : "未安装" },
    { label: "OnlyOffice", ok: !!oo.configured, detail: oo.configured ? (oo.url || "已配置") : "设 LUMEN_ONLYOFFICE_URL" },
    { label: "LangGraph", ok: !!lg.available, detail: lg.available ? (lg.hint || "可用") : (lg.hint || "未启用") },
    { label: "课题", ok: !!activeProject, detail: activeProject ? (activeProject.title || activeProject.slug) : "未选择" },
    { label: "会话", ok: !!activeThread, detail: activeThread ? "已打开" : "未打开" },
  ];
}

function copyConversationMarkdown() {
  var scroll = $("chatScroll");
  if (!scroll) return;
  var parts = [];
  scroll.querySelectorAll(".chat-msg.user, .chat-msg.agent-wrap").forEach(function (node) {
    if (node.classList.contains("user")) {
      var t = node._rawPrompt || (node.querySelector(".msg-user-text") && node.querySelector(".msg-user-text").innerText) || node.innerText || "";
      parts.push("## User\n\n" + t.trim() + "\n");
    } else {
      var at = node.querySelector(".agent-text");
      var text = at ? (at.innerText || at.textContent || "") : "";
      parts.push("## Assistant\n\n" + text.trim() + "\n");
    }
  });
  var md = parts.join("\n") || "(空对话)";
  copyTextToClipboard(md, $("copyConvBtn"));
  showLabToast("已复制对话", parts.length + " 段");
}

function touchProjectRecent(slug) {
  if (!slug) return;
  try {
    var key = "lumen-project-recent";
    var list = JSON.parse(localStorage.getItem(key) || "[]");
    list = list.filter(function (s) { return s !== slug; });
    list.unshift(slug);
    localStorage.setItem(key, JSON.stringify(list.slice(0, 40)));
  } catch (_) {}
}
function sortProjectsByRecent(list) {
  var order = [];
  try { order = JSON.parse(localStorage.getItem("lumen-project-recent") || "[]"); } catch (_) {}
  return (list || []).slice().sort(function (a, b) {
    var ai = order.indexOf(a.slug);
    var bi = order.indexOf(b.slug);
    if (ai < 0) ai = 9999;
    if (bi < 0) bi = 9999;
    if (ai !== bi) return ai - bi;
    return String(b.updated_at || "").localeCompare(String(a.updated_at || ""));
  });
}

/** Hide engineering/smoke/acceptance projects from the sidebar (data kept on server). */
function isSmokeLikeProject(p) {
  if (!p) return false;
  var blob = (String(p.title || "") + " " + String(p.slug || "")).toLowerCase();
  if (/product-smoke|live-smoke|\bsmoke\b/.test(blob)) return true;
  if (/accept|验收/.test(blob)) return true;
  return false;
}

function emptySessionHeroHtml(kind) {
  if (kind === "new") {
    return '<section class="hero" id="welcome">' +
      "<h2>新对话</h2>" +
      "<p>描述你的科研任务 — 阅读文献、分析数据、检查工作区，或迭代代码。会话会保存在当前项目中。</p>" +
      '<div class="chips">' +
      '<button class="chip" type="button" data-prompt="用通俗中文介绍你自己：你能帮研究者做哪些事，并问我今天想先做什么。">介绍你自己</button>' +
      '<button class="chip" type="button" data-prompt="浏览当前项目工作区，说明有什么文件并建议下一步。">检查工作区</button>' +
      "</div></section>";
  }
  if (kind === "deleted-project") {
    return '<section class="hero" id="welcome"><h2>项目已删除</h2><p>新建或选择其它项目继续研究。</p></section>';
  }
  if (kind === "deleted-session") {
    return '<section class="hero" id="welcome"><h2>会话已删除</h2><p>新建或选择其它会话继续。</p></section>';
  }
  return '<section class="hero" id="welcome">' +
    "<h2>开始一段研究对话</h2>" +
    "<p>发送消息开始；右侧可浏览项目文件。历史会持久化，刷新后可恢复。</p>" +
    '<div class="chips">' +
    '<button class="chip" type="button" data-prompt="用通俗中文介绍你自己：你能帮研究者做哪些事，并问我今天想先做什么。">介绍你自己</button>' +
    '<button class="chip" type="button" data-prompt="检索 PubMed 上阿司匹林相关最新文献，给出 3 条要点与中文摘要。">文献检索</button>' +
    "</div></section>";
}

function bindWelcomeChips(root) {
  var host = root || document;
  if (!host || !host.querySelectorAll) return;
  host.querySelectorAll(".chip[data-prompt], .chip[data-brief]").forEach(function (chip) {
    if (typeof bindChipButton === "function") bindChipButton(chip);
  });
}

async function loadProjects() {
  try {
    var list = await api("/api/lab/projects");
    list = sortProjectsByRecent(list);
    var visible = list.filter(function (p) { return !isSmokeLikeProject(p); });
    var nav = $("projectList");
    if (!nav) return;
    nav.innerHTML = "";
    if (!visible.length) {
      nav.innerHTML = '<div class="hint" style="padding:8px 4px">暂无项目 — 点「新建」开始一段研究</div>';
    }
    visible.forEach(function (p) {
      var row = document.createElement("div");
      row.className = "proj-row";
      var btn = document.createElement("button");
      btn.type = "button";
      btn.className = "proj-pick" + ((activeProject && activeProject.slug === p.slug) ? " active" : "");
      btn.textContent = p.title;
      btn.addEventListener("click", function () {
        saveComposerDraft();
        activeProject = p;
        touchProjectRecent(p.slug);
        activeThread = "";
        loadProjects();
        refreshFiles();
        loadSessions().then(function () {
          if (activeThread) openSession(activeThread);
          else {
            var inp = $("promptInput");
            if (inp) inp.value = "";
            loadComposerDraft();
          }
        });
        loadSkills();
        loadComputeHosts();
        loadComputeJobs();
        renderLangGraphHistory();
        var nm = $("activeProjectName");
        var mt = $("activeProjectMeta");
        if (nm) nm.textContent = p.title;
        if (mt) mt.textContent = p.slug;
      });
      var ren = document.createElement("button");
      ren.type = "button";
      ren.className = "btn sm proj-ren";
      ren.title = "重命名项目";
      ren.textContent = "✎";
      ren.addEventListener("click", function (e) {
        e.stopPropagation();
        renameProject(p.slug, p.title);
      });
      var del = document.createElement("button");
      del.type = "button";
      del.className = "btn sm proj-del";
      del.title = "删除项目";
      del.textContent = "×";
      del.addEventListener("click", function (e) {
        e.stopPropagation();
        deleteProject(p.slug, p.title);
      });
      row.appendChild(btn);
      row.appendChild(ren);
      row.appendChild(del);
      nav.appendChild(row);
    });
    // Drop smoke projects from the active selection so UI stays research-first.
    if (activeProject && isSmokeLikeProject(activeProject)) {
      activeProject = null;
      activeThread = "";
    }
    if (!activeProject && visible.length) {
      activeProject = visible[0];
      touchProjectRecent(visible[0].slug);
      refreshFiles();
      loadSessions().then(function () {
        if (activeThread) openSession(activeThread);
      });
      loadSkills();
      loadComputeHosts();
      loadComputeJobs();
      var nm = $("activeProjectName");
      var mt = $("activeProjectMeta");
      if (nm) nm.textContent = visible[0].title;
      if (mt) mt.textContent = visible[0].slug;
    } else if (!activeProject) {
      var nm0 = $("activeProjectName");
      var mt0 = $("activeProjectMeta");
      if (nm0) nm0.textContent = "未选择";
      if (mt0) mt0.textContent = "";
    }
  } catch (e) {
    var nav = $("projectList");
    if (nav) nav.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

async function renameProject(slug, curTitle) {
  if (!slug) return;
  var title = prompt("项目名称", curTitle || "");
  if (title == null) return;
  title = title.trim();
  if (!title) return;
  try {
    var p = await api("/api/lab/projects/" + encodeURIComponent(slug), {
      method: "PATCH",
      body: JSON.stringify({ title: title }),
    });
    if (activeProject && activeProject.slug === slug) {
      activeProject.title = p.title || title;
      var nm = $("activeProjectName");
      if (nm) nm.textContent = activeProject.title;
    }
    await loadProjects();
  } catch (e) {
    alert("重命名失败: " + e.message);
  }
}

async function deleteProject(slug, title) {
  if (!slug) return;
  if (!confirm("删除项目「" + (title || slug) + "」？工作区与会话不可恢复。")) return;
  try {
    await api("/api/lab/projects/" + encodeURIComponent(slug), { method: "DELETE" });
    if (activeProject && activeProject.slug === slug) {
      activeProject = null;
      activeThread = "";
      clearChatScroll(emptySessionHeroHtml("deleted-project"));
      bindWelcomeChips($("chatScroll"));
      var nm = $("activeProjectName");
      var mt = $("activeProjectMeta");
      if (nm) nm.textContent = "未选择";
      if (mt) mt.textContent = "";
      var tree = $("fileTree");
      if (tree) tree.innerHTML = "";
    }
    await loadProjects();
  } catch (e) {
    alert("删除失败: " + e.message);
  }
}

async function ensureProject() {
  if (activeProject) return activeProject;
  var p = await api("/api/lab/projects", { method: "POST", body: JSON.stringify({ title: "默认项目" }) });
  activeProject = p;
  await loadProjects();
  return p;
}

async function loadSkills() {
  var el = $("skillsBody");
  if (!el) return;
  try {
    var slug = activeProject ? activeProject.slug : "";
    var d = await api("/api/lab/skills?project_id=" + slug);
    skillsCache = d.skills || [];
    var s = skillsCache;
    if (!s.length) {
      el.innerHTML = '<div class="hint">暂无技能 — 安装 Research Pack 或 ~/.lumen/skills</div>';
      return;
    }
    el.innerHTML = s.map(function (sk) {
      var name = sk.name || sk;
      var en = sk.enabled !== false;
      return '<div class="skill-row">' +
        '<label class="skill-en"><input type="checkbox" data-skill="' + escHtml(name) + '"' + (en ? " checked" : "") + " /> 启用</label>" +
        '<div class="skill-actions">' +
        '<button type="button" class="skill-name" data-inject="' + escHtml(name) + '">📋 ' + escHtml(name) + "</button>" +
        '<button type="button" class="btn sm skill-run" data-run="' + escHtml(name) + '">运行</button>' +
        "</div>" +
        '<div class="skill-desc hint">' + escHtml(sk.description || "") + "</div>" +
        '<div class="skill-src hint">' + escHtml(sk.scope || sk.source || "") + "</div></div>";
    }).join("");
    el.querySelectorAll("[data-inject]").forEach(function (btn) {
      btn.addEventListener("click", function () { injectSkill(btn.getAttribute("data-inject"), false); });
    });
    el.querySelectorAll("[data-run]").forEach(function (btn) {
      btn.addEventListener("click", function () { injectSkill(btn.getAttribute("data-run"), true); });
    });
  } catch (e) {
    el.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

function injectSkill(name, andSend) {
  var inp = $("promptInput");
  if (!inp) return;
  var sk = null;
  for (var i = 0; i < skillsCache.length; i++) {
    if ((skillsCache[i].name || skillsCache[i]) === name) { sk = skillsCache[i]; break; }
  }
  var desc = (sk && sk.description) ? sk.description : "按该技能的标准流程执行";
  var prompt = "请使用技能「" + name + "」：" + desc + "\n请开始执行，并说明你将调用的工具。";
  inp.value = prompt;
  inp.focus();
  var hint = $("skillsHint");
  if (hint) hint.textContent = andSend ? ("运行 " + name + "…") : ("已注入 " + name);
  if (andSend) {
    submitPrompt();
  }
}

async function saveSkillsEnabled() {
  if (!activeProject) return;
  var enabled = [];
  document.querySelectorAll("#skillsBody input[data-skill]").forEach(function (b) {
    if (b.checked) enabled.push(b.getAttribute("data-skill"));
  });
  try {
    await api("/api/lab/skills?project_id=" + activeProject.slug, {
      method: "PUT",
      body: JSON.stringify({ project_id: activeProject.slug, enabled: enabled }),
    });
    var hint = $("skillsHint");
    if (hint) hint.textContent = "已保存 " + enabled.length + " 项";
  } catch (e) {
    alert(e.message);
  }
}

/* ── 5. Sessions (persisted) ── */

function renderThreadTabs() {
  var host = $("convTabs");
  if (!host) return;
  if (!threads.length) {
    host.innerHTML = '<span class="hint" style="padding:8px">无会话 — 点 ＋ 新建</span>';
    return;
  }
  host.innerHTML = threads
    .map(function (t) {
      var count = t.turn_count != null ? t.turn_count : (t.turns ? t.turns.length : 0);
      return '<button type="button" class="ctr-tab' + (t.id === activeThread ? " active" : "") +
        '" data-id="' + escHtml(t.id) + '" title="双击重命名">' +
        '<span>' + escHtml(t.title || t.id) + (count ? " · " + count : "") + "</span></button>";
    })
    .join("");
  host.querySelectorAll(".ctr-tab").forEach(function (btn) {
    btn.addEventListener("click", function () {
      openSession(btn.dataset.id);
    });
    btn.addEventListener("dblclick", function (e) {
      e.preventDefault();
      renameSession(btn.dataset.id);
    });
  });
  renderSessionListSide();
}

function pinKey() {
  return "lumen-pins:" + ((activeProject && activeProject.slug) || "none");
}
function loadPins() {
  try {
    return JSON.parse(localStorage.getItem(pinKey()) || "[]");
  } catch (_) {
    return [];
  }
}
function savePins(ids) {
  try { localStorage.setItem(pinKey(), JSON.stringify(ids.slice(0, 50))); } catch (_) {}
}
function isPinned(id) {
  return loadPins().indexOf(id) >= 0;
}
function togglePin(id) {
  var pins = loadPins();
  var i = pins.indexOf(id);
  if (i >= 0) pins.splice(i, 1);
  else pins.unshift(id);
  savePins(pins);
  renderThreadTabs();
}

function sortThreadsWithPins(list) {
  var pins = loadPins();
  return list.slice().sort(function (a, b) {
    var ap = pins.indexOf(a.id);
    var bp = pins.indexOf(b.id);
    var aPin = ap >= 0 ? 1 : 0;
    var bPin = bp >= 0 ? 1 : 0;
    if (aPin !== bPin) return bPin - aPin;
    if (aPin && bPin) return ap - bp;
    return 0;
  });
}

function renderSessionListSide() {
  var el = $("sessionList");
  if (!el) return;
  if (!activeProject) {
    el.innerHTML = '<div class="hint">选择课题后加载…</div>';
    return;
  }
  if (!threads.length) {
    el.innerHTML = '<div class="hint">暂无会话</div>';
    return;
  }
  var ordered = sortThreadsWithPins(threads);
  el.innerHTML = ordered.map(function (t) {
    var pin = isPinned(t.id);
    return '<div class="sess-item-row">' +
      '<button type="button" class="sess-item' + (t.id === activeThread ? " active" : "") + '" data-id="' + escHtml(t.id) + '">' +
      (pin ? '<span class="pin-mark">📌</span>' : "") +
      '<span class="sess-title">' + escHtml(t.title || t.id) + "</span>" +
      '<span class="sess-meta">' + (t.turn_count || 0) + " 轮</span></button>" +
      '<button type="button" class="btn sm sess-pin" data-pin="' + escHtml(t.id) + '" title="置顶">' + (pin ? "★" : "☆") + "</button>" +
      '<button type="button" class="btn sm sess-fork" data-fork="' + escHtml(t.id) + '" title="分支会话">⎇</button>' +
      '<button type="button" class="btn sm sess-ren" data-ren="' + escHtml(t.id) + '" title="重命名">✎</button>' +
      '<button type="button" class="btn sm sess-del" data-del="' + escHtml(t.id) + '" title="删除">×</button></div>';
  }).join("");
  el.querySelectorAll(".sess-item").forEach(function (btn) {
    btn.addEventListener("click", function () { openSession(btn.dataset.id); });
  });
  el.querySelectorAll(".sess-pin").forEach(function (btn) {
    btn.addEventListener("click", function (e) {
      e.stopPropagation();
      togglePin(btn.getAttribute("data-pin"));
    });
  });
  el.querySelectorAll(".sess-fork").forEach(function (btn) {
    btn.addEventListener("click", function (e) {
      e.stopPropagation();
      forkSession(btn.getAttribute("data-fork"));
    });
  });
  el.querySelectorAll(".sess-ren").forEach(function (btn) {
    btn.addEventListener("click", function (e) {
      e.stopPropagation();
      renameSession(btn.getAttribute("data-ren"));
    });
  });
  el.querySelectorAll(".sess-del").forEach(function (btn) {
    btn.addEventListener("click", function (e) {
      e.stopPropagation();
      deleteSession(btn.getAttribute("data-del"));
    });
  });
}

async function forkSession(id) {
  if (!activeProject || !id) return;
  var cur = threads.find(function (t) { return t.id === id; });
  var title = prompt("分支会话标题", "分支 · " + ((cur && cur.title) || "对话"));
  if (title == null) return;
  try {
    var sess = await api("/api/lab/projects/" + activeProject.slug + "/sessions/" + encodeURIComponent(id) + "/fork", {
      method: "POST",
      body: JSON.stringify({ title: title.trim() || undefined }),
    });
    await loadSessions();
    if (sess.id) openSession(sess.id);
  } catch (e) {
    alert("分支失败: " + e.message);
  }
}

async function renameSession(id) {
  if (!activeProject || !id) return;
  var cur = threads.find(function (t) { return t.id === id; });
  var title = prompt("会话标题", (cur && cur.title) || "");
  if (title == null) return;
  title = title.trim();
  if (!title) return;
  try {
    var sess = await api("/api/lab/projects/" + activeProject.slug + "/sessions/" + encodeURIComponent(id), {
      method: "PATCH",
      body: JSON.stringify({ title: title }),
    });
    threads.forEach(function (t) {
      if (t.id === id) t.title = sess.title || title;
    });
    renderThreadTabs();
  } catch (e) {
    alert("重命名失败: " + e.message);
  }
}

async function deleteSession(id) {
  if (!activeProject || !id) return;
  if (!confirm("删除会话 " + id + "？不可恢复。")) return;
  try {
    await api("/api/lab/projects/" + activeProject.slug + "/sessions/" + encodeURIComponent(id), {
      method: "DELETE",
    });
    if (activeThread === id) {
      activeThread = "";
      clearChatScroll(emptySessionHeroHtml("deleted-session"));
      bindWelcomeChips($("chatScroll"));
    }
    await loadSessions();
    if (activeThread) openSession(activeThread);
    else if (threads.length) openSession(threads[0].id);
  } catch (e) {
    alert(e.message);
  }
}

async function loadSessions() {
  if (!activeProject) return;
  try {
    var data = await api("/api/lab/projects/" + activeProject.slug + "/sessions");
    threads = data.sessions || [];
    if (!threads.length) {
      var created = await api("/api/lab/projects/" + activeProject.slug + "/sessions", {
        method: "POST",
        body: JSON.stringify({ title: "对话" }),
      });
      threads = [created];
    }
    if (!activeThread || !threads.some(function (t) { return t.id === activeThread; })) {
      activeThread = threads[0].id;
    }
    renderThreadTabs();
  } catch (e) {
    var el = $("sessionList");
    if (el) el.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

async function openSession(id) {
  if (!activeProject || !id) return;
  // persist draft for previous session before switching
  saveComposerDraft();
  activeThread = id;
  sseState = null;
  turnTasks = [];
  renderTasksPane();
  renderThreadTabs();
  try {
    var sess = await api("/api/lab/projects/" + activeProject.slug + "/sessions/" + encodeURIComponent(id));
    renderHistory(sess.turns || []);
  } catch (e) {
    clearChatScroll('<div class="ft-err">加载会话失败: ' + escHtml(e.message) + "</div>");
  }
  // load draft for new session
  var inp = $("promptInput");
  if (inp) inp.value = "";
  loadComposerDraft();
}

function clearChatScroll(html) {
  var scroll = $("chatScroll");
  if (!scroll) return;
  scroll.innerHTML = html || "";
}

function renderHistory(turns) {
  clearChatScroll("");
  if (!turns || !turns.length) {
    clearChatScroll(emptySessionHeroHtml("empty"));
    bindWelcomeChips($("chatScroll"));
    return;
  }
  turns.forEach(function (t) {
    if (t.role === "user") {
      $("chatScroll").appendChild(renderUserBubble(t.text || ""));
      return;
    }
    if (t.role === "assistant") {
      var bubble = createAgentBubble();
      if (t.text) {
        bubble.textDiv.innerHTML = renderMarkdown(t.text);
        bindWorkspaceRefs(bubble.textDiv);
        bindCodeCopy(bubble.textDiv);
      }
      (t.tools || []).forEach(function (tool) {
        upsertToolCard(bubble.toolLog, {
          id: tool.id || tool.name,
          name: tool.name,
          args: tool.args || "",
          output: tool.output || "",
          err: tool.err || "",
          status: tool.status || (tool.err ? "error" : "done"),
        });
      });
    }
  });
  $("chatScroll").scrollTop = $("chatScroll").scrollHeight;
}

async function newConv() {
  if (!activeProject) {
    try { await ensureProject(); } catch (e) { alert(e.message); return; }
  }
  try {
    var sess = await api("/api/lab/projects/" + activeProject.slug + "/sessions", {
      method: "POST",
      body: JSON.stringify({ title: "对话 " + (threads.length + 1) }),
    });
    threads.unshift({ id: sess.id, title: sess.title, turn_count: 0 });
    activeThread = sess.id;
    renderThreadTabs();
    clearChatScroll(emptySessionHeroHtml("new"));
    bindWelcomeChips($("chatScroll"));
  } catch (e) {
    alert(e.message);
  }
}

function renderTasksPane() {
  var el = $("tasksBody");
  if (!el) return;
  if (!turnTasks.length) {
    el.innerHTML = '<div class="hint">本回合尚无工具调用</div>';
    renderSubagentTimeline();
    return;
  }
  el.innerHTML = turnTasks.map(function (t) {
    var nest = t.parentId ? " nested" : "";
    return '<div class="task-row status-' + escHtml(t.status || "") + nest + '">' +
      '<span class="task-name">' + (t.parentId ? "↳ " : "") + "⚙ " + escHtml(t.name || t.id) + "</span>" +
      '<span class="task-st">' + escHtml(statusLabel(t.status)) + "</span></div>";
  }).join("");
  renderSubagentTimeline();
}

/* ── 6. DOM rendering helpers ── */

function createAgentBubble() {
  $("welcome") && $("welcome").remove();
  var wrap = document.createElement("div");
  wrap.className = "chat-msg agent-wrap";

  // Think block (hidden by default)
  var think = document.createElement("details");
  think.className = "think-block";
  think.style.display = "none";
  think.innerHTML = '<summary class="think-summary"><span class="think-dot"></span>思考中…</summary><div class="think-body"></div>';
  wrap.appendChild(think);

  // Tool log first (OCS: tools appear in-stream before final answer text)
  var toolLog = document.createElement("div");
  toolLog.className = "tool-log";
  wrap.appendChild(toolLog);

  // Agent text
  var textDiv = document.createElement("div");
  textDiv.className = "agent-text agent-md";
  wrap.appendChild(textDiv);

  // Footer actions
  var actions = document.createElement("div");
  actions.className = "agent-actions";
  actions.innerHTML =
    '<button type="button" class="btn sm agent-copy">复制回复</button> ' +
    '<button type="button" class="btn sm agent-quote">引用到输入框</button> ' +
    '<button type="button" class="btn sm agent-continue">继续</button> ' +
    '<button type="button" class="btn sm agent-tools-toggle">展开工具</button>';
  wrap.appendChild(actions);
  actions.querySelector(".agent-copy").addEventListener("click", function () {
    var text = textDiv.innerText || textDiv.textContent || "";
    copyTextToClipboard(text, actions.querySelector(".agent-copy"));
  });
  actions.querySelector(".agent-quote").addEventListener("click", function () {
    var text = textDiv.innerText || textDiv.textContent || "";
    quoteIntoComposer(text);
  });
  actions.querySelector(".agent-continue").addEventListener("click", function () {
    var inp = $("promptInput");
    if (inp) {
      if (!inp.value.trim()) inp.value = "请继续，补充细节并给出下一步可执行建议。";
      inp.focus();
    }
  });
  actions.querySelector(".agent-tools-toggle").addEventListener("click", function () {
    var cards = toolLog.querySelectorAll(".tool-card");
    if (!cards.length) return;
    var anyOpen = toolLog.querySelector(".tool-card.is-open");
    var open = !anyOpen;
    cards.forEach(function (c) { setToolCardOpen(c, open); });
    actions.querySelector(".agent-tools-toggle").textContent = open ? "折叠工具" : "展开工具";
  });

  $("chatScroll").appendChild(wrap);
  wrap.scrollIntoView({ behavior: "smooth", block: "end" });
  return { wrap: wrap, think: think, thinkBody: think.querySelector(".think-body"), textDiv: textDiv, toolLog: toolLog };
}

function copyTextToClipboard(text, btn) {
  if (!text) return;
  var done = function () {
    if (!btn) return;
    var old = btn.textContent;
    btn.textContent = "已复制";
    setTimeout(function () { btn.textContent = old; }, 1200);
  };
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(text).then(done).catch(function () {
      try {
        var ta = document.createElement("textarea");
        ta.value = text;
        document.body.appendChild(ta);
        ta.select();
        document.execCommand("copy");
        document.body.removeChild(ta);
        done();
      } catch (_) {}
    });
  }
}

function quoteIntoComposer(text) {
  var inp = $("promptInput");
  if (!inp || !text) return;
  var clipped = text.trim();
  if (clipped.length > 2500) clipped = clipped.slice(0, 2500) + "…";
  var quoted = clipped.split("\n").map(function (l) { return "> " + l; }).join("\n");
  var prefix = quoted + "\n\n";
  inp.value = prefix + (inp.value || "");
  inp.focus();
  try {
    inp.setSelectionRange(inp.value.length, inp.value.length);
  } catch (_) {}
  saveComposerDraft();
}

function draftKey() {
  var slug = (activeProject && activeProject.slug) || "none";
  var sid = activeThread || "none";
  return "lumen-draft:" + slug + ":" + sid;
}

function saveComposerDraft() {
  var inp = $("promptInput");
  if (!inp) return;
  try {
    var v = inp.value || "";
    if (v) localStorage.setItem(draftKey(), v);
    else localStorage.removeItem(draftKey());
  } catch (_) {}
}

function loadComposerDraft() {
  var inp = $("promptInput");
  if (!inp) return;
  try {
    var v = localStorage.getItem(draftKey()) || "";
    // don't clobber if user already typed
    if (!inp.value && v) inp.value = v;
  } catch (_) {}
}

function clearComposerDraft() {
  try { localStorage.removeItem(draftKey()); } catch (_) {}
}

function findToolCard(toolLog, id) {
  if (!toolLog || id == null) return null;
  var cards = toolLog.querySelectorAll(".tool-card");
  for (var i = 0; i < cards.length; i++) {
    if (cards[i].getAttribute("data-tool-id") === String(id)) return cards[i];
  }
  return null;
}

function setToolCardOpen(card, open) {
  if (!card) return;
  var body = card.querySelector(".tool-card-body");
  if (body) body.style.display = open ? "block" : "none";
  card.classList.toggle("is-open", !!open);
}

function upsertToolCard(toolLog, tool) {
  var id = tool.id;
  var existing = findToolCard(toolLog, id);
  if (existing) {
    updateToolCardDOM(existing, tool);
    return existing;
  }
  // Create new card (nested if parent_id)
  var card = document.createElement("div");
  var nested = !!(tool.parentId || tool.parent_id);
  var status = tool.status || "running";
  card.className = "tool-card status-" + status + (nested ? " nested" : "");
  card.setAttribute("data-tool-id", String(id));
  if (nested) card.setAttribute("data-parent-id", String(tool.parentId || tool.parent_id));
  var isSub = nested || /^(task|run_skill|subagent)/i.test(tool.name || "");
  var preview = formatToolArgsPreview(tool.args);
  card.innerHTML =
    '<div class="tool-card-hd">' +
    '<span class="tool-card-arrow">▸</span>' +
    '<span class="tool-card-icon">' + (isSub ? "🧩" : "⚙") + "</span>" +
    '<span class="tool-card-title">' +
    '<span class="tool-card-name">' + escHtml(tool.name || "tool") +
    (nested ? ' <span class="hint">子调用</span>' : "") +
    (isSub && !nested ? ' <span class="hint">子代理/技能</span>' : "") +
    "</span>" +
    (preview ? '<span class="tool-card-preview" title="' + escHtml(preview) + '">' + escHtml(preview) + "</span>" : '<span class="tool-card-preview" hidden></span>') +
    "</span>" +
    '<span class="tool-card-status">' + statusLabel(status) + "</span>" +
    "</div>" +
    '<div class="tool-card-body" style="display:none">' +
    '<div class="tool-card-section tool-card-args-section" style="' + (tool.args ? "" : "display:none") + '"><div class="tool-card-label">参数</div><pre class="tool-card-args">' + escHtml(typeof tool.args === "string" ? tool.args : (tool.args ? JSON.stringify(tool.args, null, 2) : "")) + "</pre></div>" +
    (tool.description ? '<div class="tool-card-section"><div class="tool-card-label">说明</div><div>' + escHtml(tool.description) + "</div></div>" : "") +
    '<div class="tool-card-section tool-card-output-section" style="display:none"><div class="tool-card-label">输出 <button type="button" class="btn sm tool-copy-out">复制</button></div><pre class="tool-card-output"></pre></div>' +
    '<div class="tool-card-section tool-card-err-section" style="display:none"><div class="tool-card-label">错误</div><pre class="tool-card-err"></pre></div>' +
    '<div class="tool-card-children"></div>' +
    "</div>";

  card.querySelector(".tool-card-hd").addEventListener("click", function () {
    setToolCardOpen(card, !card.classList.contains("is-open"));
  });
  var copyOut = card.querySelector(".tool-copy-out");
  if (copyOut) {
    copyOut.addEventListener("click", function (e) {
      e.stopPropagation();
      var pre = card.querySelector(".tool-card-output");
      copyTextToClipboard((pre && pre.textContent) || "", copyOut);
    });
  }

  var parentId = tool.parentId || tool.parent_id;
  var parent = parentId ? findToolCard(toolLog, parentId) : null;
  if (parent) {
    var kids = parent.querySelector(".tool-card-children");
    if (!kids) {
      kids = document.createElement("div");
      kids.className = "tool-card-children";
      parent.querySelector(".tool-card-body").appendChild(kids);
    }
    setToolCardOpen(parent, true);
    kids.appendChild(card);
  } else {
    toolLog.appendChild(card);
  }
  // Auto-expand running tools and subagents (OCS-like visibility)
  if (status === "running" || isSub || tool.err) setToolCardOpen(card, true);
  if (tool.output || tool.err) {
    updateToolCardDOM(card, tool);
  }
  card.scrollIntoView({ behavior: "smooth", block: "nearest" });
  return card;
}

function updateToolCardDOM(card, tool) {
  var open = card.classList.contains("is-open");
  var status = tool.status || "running";
  card.className = "tool-card status-" + status + (open ? " is-open" : "") +
    (card.classList.contains("nested") ? " nested" : "");
  var statusEl = card.querySelector(".tool-card-status");
  if (statusEl) statusEl.textContent = statusLabel(status);
  var nameEl = card.querySelector(".tool-card-name");
  if (nameEl && tool.name) {
    // keep nested/subagent hints if present
    var hints = nameEl.querySelectorAll(".hint");
    var hintHtml = "";
    for (var hi = 0; hi < hints.length; hi++) hintHtml += " " + hints[hi].outerHTML;
    nameEl.innerHTML = escHtml(tool.name) + hintHtml;
  }
  var prevEl = card.querySelector(".tool-card-preview");
  if (prevEl && tool.args) {
    var preview = formatToolArgsPreview(tool.args);
    if (preview) {
      prevEl.hidden = false;
      prevEl.textContent = preview;
      prevEl.title = preview;
    }
  }

  if (tool.args) {
    var argsSec = card.querySelector(".tool-card-args-section");
    var argsPre = card.querySelector(".tool-card-args");
    if (argsSec) argsSec.style.display = "block";
    if (argsPre) {
      argsPre.textContent = typeof tool.args === "string"
        ? tool.args
        : JSON.stringify(tool.args, null, 2);
    }
  }
  if (tool.output) {
    var outSec = card.querySelector(".tool-card-output-section");
    var outPre = card.querySelector(".tool-card-output");
    if (outSec) outSec.style.display = "block";
    if (outPre) outPre.textContent = tool.output;
    if (status === "error" || tool.err) setToolCardOpen(card, true);
  }
  if (tool.err) {
    var errSec = card.querySelector(".tool-card-err-section");
    var errPre = card.querySelector(".tool-card-err");
    if (errSec) errSec.style.display = "block";
    setToolCardOpen(card, true);
    if (errPre) errPre.textContent = tool.err;
  }
  // Auto-expand when finished with content or on error
  if (tool.status === "error" || (tool.status === "done" && (tool.output || tool.err))) {
    setToolCardOpen(card, true);
  }
}

function statusLabel(status) {
  switch (status) {
    case "running": return "运行中";
    case "done": return "完成";
    case "error": return "错误";
    default: return status || "";
  }
}

var runTimerId = null;
var runStartedAt = 0;
var lastUsage = null;

function formatElapsed(ms) {
  var s = Math.floor(ms / 1000);
  var m = Math.floor(s / 60);
  s = s % 60;
  return m > 0 ? (m + ":" + String(s).padStart(2, "0")) : (s + "s");
}

function setRunStatus(running) {
  var dot = $("liveDot");
  var label = $("runStatus");
  var stopBtn = $("btnStop");
  var sendBtn = $("btnSend");
  var hint = $("composerHint");
  if (running) {
    if (dot) dot.style.background = "#f59e0b";
    if (label) label.textContent = "运行中";
    if (hint) hint.textContent = "生成中… Enter 发送 · Shift+Enter 换行";
    if (stopBtn) stopBtn.disabled = false;
    if (sendBtn) sendBtn.disabled = true;
    runStartedAt = Date.now();
    if (runTimerId) clearInterval(runTimerId);
    runTimerId = setInterval(function () {
      if (!label) return;
      var t = formatElapsed(Date.now() - runStartedAt);
      var u = lastUsage && (lastUsage.total_tokens || lastUsage.total) ?
        " · " + (lastUsage.total_tokens || lastUsage.total) + " tok" : "";
      label.textContent = "运行中 " + t + u;
    }, 500);
  } else {
    if (runTimerId) { clearInterval(runTimerId); runTimerId = null; }
    if (dot) dot.style.background = "";
    var doneLabel = "就绪";
    if (runStartedAt) {
      doneLabel = "就绪 · " + formatElapsed(Date.now() - runStartedAt);
      if (lastUsage && (lastUsage.total_tokens || lastUsage.total)) {
        doneLabel += " · " + (lastUsage.total_tokens || lastUsage.total) + " tok";
      }
    }
    if (label) label.textContent = doneLabel;
    if (hint) hint.textContent = "就绪 · Enter 发送 · Shift+Enter 换行";
    if (stopBtn) stopBtn.disabled = true;
    if (sendBtn) sendBtn.disabled = false;
  }
}

function maybeScrollChat() {
  var scroll = $("chatScroll");
  if (!scroll) return;
  var dist = scroll.scrollHeight - scroll.scrollTop - scroll.clientHeight;
  if (dist < 160) scroll.scrollTop = scroll.scrollHeight;
}

function showLabToast(title, body) {
  var host = $("toastHost");
  if (!host) {
    host = document.createElement("div");
    host.id = "toastHost";
    host.className = "toast-host";
    document.body.appendChild(host);
  }
  var el = document.createElement("div");
  el.className = "lab-toast";
  el.innerHTML = '<div class="toast-t">' + escHtml(title || "") + '</div>' +
    (body ? '<div class="toast-b">' + escHtml(body) + "</div>" : "");
  host.appendChild(el);
  setTimeout(function () {
    el.classList.add("out");
    setTimeout(function () { el.remove(); }, 300);
  }, 3500);
}

function toggleCompactMode() {
  var on = document.documentElement.classList.toggle("compact");
  try { localStorage.setItem("lumen-compact", on ? "1" : "0"); } catch (_) {}
  var btn = $("btnCompact");
  if (btn) btn.textContent = on ? "宽松" : "紧凑";
}

function addErrorBubble(toolLog, message) {
  var div = document.createElement("div");
  div.className = "chat-msg error-msg";
  div.textContent = "⚠ " + message;
  toolLog.appendChild(div);
}

/* ── 7. SSE streaming + chat ── */

function handleSSEEvent(ev, state, bubble) {
  state = reduceSSE(state, ev);
  var kind = ev.kind || "";

  // Update reasoning (think block)
  if (state.reasoning && bubble.think) {
    bubble.think.style.display = "block";
    bubble.thinkBody.textContent = state.reasoning;
    if (kind === "turn_done" || (kind === "text" && state.text)) {
      bubble.think.open = false;
    }
  }

  // Update markdown text
  if (state.text && bubble.textDiv) {
    bubble.textDiv.innerHTML = renderMarkdown(state.text) + '<span class="cursor">|</span>';
    bindWorkspaceRefs(bubble.textDiv);
    bindCodeCopy(bubble.textDiv);
    maybeScrollChat();
  }

  // Token usage
  if (kind === "usage" && state.usage) {
    lastUsage = state.usage;
    var usageEl = $("usageBadge");
    if (usageEl) {
      var tot = state.usage.total_tokens || ((state.usage.input_tokens || 0) + (state.usage.output_tokens || 0));
      usageEl.textContent = tot ? (tot + " tok") : "—";
      usageEl.title = "in " + (state.usage.input_tokens || 0) + " / out " + (state.usage.output_tokens || 0);
    }
  }

  // Session id from server
  if (kind === "session" && ev.id) {
    activeThread = ev.id;
    var found = false;
    threads.forEach(function (t) {
      if (t.id === ev.id) {
        found = true;
        if (ev.title) t.title = ev.title;
      }
    });
    if (!found) threads.unshift({ id: ev.id, title: ev.title || "对话", turn_count: 0 });
    renderThreadTabs();
  }

  // Tool dispatch
  if (kind === "tool_dispatch" && ev.tool && ev.tool.id) {
    upsertToolCard(bubble.toolLog, state.tools[ev.tool.id]);
    syncTurnTask(state.tools[ev.tool.id]);
    focusActivityPane("tasks");
  }

  // Tool result
  if (kind === "tool_result" && ev.tool && ev.tool.id) {
    var card = findToolCard(bubble.toolLog, ev.tool.id);
    if (card) {
      updateToolCardDOM(card, state.tools[ev.tool.id]);
    } else {
      upsertToolCard(bubble.toolLog, state.tools[ev.tool.id]);
    }
    syncTurnTask(state.tools[ev.tool.id]);
  }

  // Tool progress
  if (kind === "tool_progress" && ev.tool && ev.tool.id) {
    var tcard = findToolCard(bubble.toolLog, ev.tool.id);
    if (tcard) updateToolCardDOM(tcard, state.tools[ev.tool.id]);
  }

  // Approval
  if (kind === "approval_request") {
    renderApprovalCard(bubble.toolLog, ev);
    focusActivityPane("tasks");
    updateApprovalBanner();
    try {
      if (typeof Notification !== "undefined" && Notification.permission === "granted") {
        new Notification("Lumen Lab · 需要批准工具", {
          body: (ev.tool || "tool") + (ev.summary ? ": " + ev.summary : ""),
        });
      }
    } catch (_) {}
  }

  // Error
  if (kind === "error") {
    addErrorBubble(bubble.toolLog, ev.text || "未知错误");
  }

  // Turn lifecycle
  if (kind === "turn_started") {
    setRunStatus(true);
  }
  if (kind === "turn_done") {
    setRunStatus(false);
    if (bubble.textDiv) {
      var cur = bubble.textDiv.querySelector(".cursor");
      if (cur) cur.remove();
    }
    maybeScrollChat();
    refreshFiles();
    loadSessions();
    currentAbort = null;
    if (document.hidden) {
      showLabToast("回合完成", (state.text || "").slice(0, 80) || "Agent 已回复");
      try {
        if (typeof Notification !== "undefined" && Notification.permission === "granted") {
          new Notification("Lumen Lab", { body: "回合完成", silent: true });
        }
      } catch (_) {}
    }
  }

  return state;
}

function syncTurnTask(tool) {
  if (!tool) return;
  var i, found = -1;
  for (i = 0; i < turnTasks.length; i++) {
    if (turnTasks[i].id === tool.id) { found = i; break; }
  }
  if (found >= 0) turnTasks[found] = tool;
  else turnTasks.push(tool);
  renderTasksPane();
  renderSubagentTimeline();
}

function renderSubagentTimeline() {
  var el = $("subagentTimeline");
  if (!el) return;
  var roots = turnTasks.filter(function (t) {
    return !t.parentId && (/^(task|run_skill)/i.test(t.name || "") || turnTasks.some(function (c) {
      return c.parentId === t.id;
    }));
  });
  var nested = turnTasks.filter(function (t) { return !!t.parentId; });
  if (!roots.length && !nested.length) {
    el.innerHTML = '<div class="hint">本回合无子代理 / 嵌套工具</div>';
    return;
  }
  // Build parent → children map
  var byParent = {};
  turnTasks.forEach(function (t) {
    var p = t.parentId || "_root_";
    if (!byParent[p]) byParent[p] = [];
    byParent[p].push(t);
  });
  function renderNode(t, depth) {
    var kids = byParent[t.id] || [];
    var pad = depth * 14;
    var isSub = /^(task|run_skill)/i.test(t.name || "") || depth > 0;
    return '<div class="tl-node status-' + escHtml(t.status || "") + '" style="padding-left:' + pad + 'px">' +
      '<span class="tl-icon">' + (isSub ? "🧩" : "⚙") + "</span>" +
      '<span class="tl-name">' + escHtml(t.name || t.id) + "</span>" +
      '<span class="tl-st">' + escHtml(statusLabel(t.status)) + "</span></div>" +
      kids.map(function (c) { return renderNode(c, depth + 1); }).join("");
  }
  var top = byParent["_root_"] || turnTasks.filter(function (t) { return !t.parentId; });
  el.innerHTML = top.map(function (t) { return renderNode(t, 0); }).join("") ||
    nested.map(function (t) { return renderNode(t, 1); }).join("");
}

function renderUserBubble(text) {
  var ue = document.createElement("div");
  ue.className = "chat-msg user";
  var paths = parseAttachmentPaths(text);
  var body = text;
  // strip attachment lines for cleaner text if we show cards
  if (paths.length) {
    body = text.replace(/\[(附件|产物)\]\s*[^\n\r]+\n?/g, "").trim();
  }
  var html = "";
  if (paths.length) {
    html += '<div class="msg-attach-list">' + paths.map(function (p) {
      return '<button type="button" class="msg-attach-card" data-path="' + escHtml(p) + '">' +
        '<span class="msg-attach-icon">📎</span><span class="msg-attach-path">' + escHtml(p) + "</span></button>";
    }).join("") + "</div>";
  }
  if (body) {
    html += '<div class="msg-user-text">' + escHtml(body).replace(/\n/g, "<br>") + "</div>";
  }
  html += '<div class="user-actions">' +
    '<button type="button" class="btn sm user-edit">编辑到输入框</button> ' +
    '<button type="button" class="btn sm user-resend">重发</button></div>';
  ue.innerHTML = html || escHtml(text);
  ue._rawPrompt = text;
  ue.querySelectorAll(".msg-attach-card").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var p = btn.getAttribute("data-path");
      var filesTab = document.querySelector('.insp-tab[data-pane="files"]');
      if (filesTab) filesTab.click();
      previewFile(p, "");
    });
  });
  var editBtn = ue.querySelector(".user-edit");
  if (editBtn) {
    editBtn.addEventListener("click", function (e) {
      e.stopPropagation();
      var inp = $("promptInput");
      if (!inp) return;
      inp.value = text || "";
      inp.focus();
      saveComposerDraft();
    });
  }
  var resendBtn = ue.querySelector(".user-resend");
  if (resendBtn) {
    resendBtn.addEventListener("click", function (e) {
      e.stopPropagation();
      if (!text) return;
      var mode = ($("chatMode") && $("chatMode").value) || "agent";
      streamChat(text, mode).catch(function (err) {
        addErrorBubble($("chatScroll"), err.message);
      });
    });
  }
  return ue;
}

async function streamChat(prompt, mode) {
  mode = mode || "agent";
  var p;
  try { p = await ensureProject(); } catch (e) { addErrorBubble($("chatScroll"), "无法获取课题: " + e.message); return; }
  if (!activeThread) {
    try { await loadSessions(); } catch (_) {}
  }
  $("welcome") && $("welcome").remove();

  // User bubble (structured attachments)
  var ue = renderUserBubble(prompt);
  $("chatScroll").appendChild(ue);
  ue.scrollIntoView({ behavior: "smooth", block: "end" });

  // Agent bubble
  var bubble = createAgentBubble();

  // Reset state
  sseState = { text: "", reasoning: "", tools: {}, approvals: [], errors: [], turn: null };
  turnTasks = [];
  renderTasksPane();
  setRunStatus(true);
  currentAbort = new AbortController();
  try {
    if (typeof Notification !== "undefined" && Notification.permission === "default") {
      Notification.requestPermission().catch(function () {});
    }
  } catch (_) {}

  try {
    var res = await fetch(labPath("/api/lab/chat"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ project_id: p.slug, prompt: prompt, mode: mode, session_id: activeThread || "" }),
      signal: currentAbort.signal,
    });

    if (!res.ok) {
      bubble.textDiv.innerHTML = '<span class="error-text">错误: HTTP ' + res.status + "</span>";
      setRunStatus(false);
      currentAbort = null;
      return;
    }

    var reader = res.body.getReader();
    var dec = new TextDecoder();
    var buf = "";

    while (true) {
      var chunk;
      try { chunk = await reader.read(); } catch (e) {
        if (e.name === "AbortError") break;
        throw e;
      }
      if (chunk.done) break;
      buf += dec.decode(chunk.value, { stream: true });
      var lines = buf.split("\n");
      buf = lines.pop();
      for (var i = 0; i < lines.length; i++) {
        var line = lines[i];
        if (line.indexOf("data:") !== 0) continue;
        var json = line.slice(5).trim();
        if (!json.startsWith("{")) continue;
        var ev;
        try { ev = JSON.parse(json); } catch (_) { continue; }
        sseState = handleSSEEvent(ev, sseState, bubble);
      }
    }
  } catch (e) {
    if (e.name === "AbortError") {
      bubble.textDiv.innerHTML = (bubble.textDiv.innerHTML || "") + '<div class="stop-notice">⏹ 已停止</div>';
    } else {
      bubble.textDiv.innerHTML = '<span class="error-text">错误: ' + escHtml(e.message) + "</span>";
    }
  } finally {
    setRunStatus(false);
    // Remove cursor
    if (bubble.textDiv) {
      var cur = bubble.textDiv.querySelector(".cursor");
      if (cur) cur.remove();
    }
    currentAbort = null;
    try { await refreshFiles(); } catch (_) {}
  }

  $("chatScroll").scrollTop = $("chatScroll").scrollHeight;
}

/* ── 8. Approval card (editable args applied on allow) ── */

function focusActivityPane(pane) {
  try {
    var tab = document.querySelector('.insp-tab[data-pane="' + (pane || "tasks") + '"]');
    if (tab && !tab.classList.contains("active")) tab.click();
  } catch (_) {}
}

function pendingApprovalCards() {
  return Array.prototype.slice.call(document.querySelectorAll(".approval-card.pending"));
}

function updateApprovalBanner() {
  var banner = $("approvalBanner");
  var text = $("approvalBannerText");
  var hint = $("composerHint");
  var pending = pendingApprovalCards();
  if (!banner) return;
  if (!pending.length) {
    banner.hidden = true;
    document.body.classList.remove("awaiting-approval");
    if (hint && /批准|等待工具/.test(hint.textContent || "")) {
      hint.textContent = "就绪 · Agent 模式写文件/bash 需批准";
    }
    return;
  }
  banner.hidden = false;
  document.body.classList.add("awaiting-approval");
  var first = pending[0];
  var tool = (first && first.getAttribute("data-tool")) || "tool";
  var sum = (first && first.getAttribute("data-summary")) || "";
  if (text) {
    text.textContent = pending.length > 1
      ? (pending.length + " 个工具等待确认 · 当前: " + tool)
      : ("需要确认: " + tool + (sum ? " — " + sum : ""));
  }
  if (hint) hint.textContent = "等待工具批准… 允许后 Agent 继续";
}

function replyLatestPendingApproval(allow) {
  var pending = pendingApprovalCards();
  if (!pending.length) return;
  var card = pending[0];
  var btn = card.querySelector(allow ? ".appr-yes" : ".appr-no");
  if (btn && !btn.disabled) btn.click();
}

function renderApprovalCard(toolLog, ev) {
  var card = document.createElement("div");
  card.className = "approval-card pending";
  card.setAttribute("data-appr-id", String(ev.id || ""));
  card.setAttribute("data-tool", String(ev.tool || "tool"));
  card.setAttribute("data-summary", String(ev.summary || "").slice(0, 200));
  var argsText = ev.args || "";
  if (typeof argsText !== "string") {
    try { argsText = JSON.stringify(argsText, null, 2); } catch (_) { argsText = String(argsText); }
  }
  var originalArgs = argsText;
  try {
    var parsed = JSON.parse(argsText);
    argsText = JSON.stringify(parsed, null, 2);
    originalArgs = argsText;
  } catch (_) {}
  card.innerHTML =
    '<div class="appr-hd">' +
    '<div class="appr-title">⚠ 需要你确认工具</div>' +
    '<div class="appr-tool">' + escHtml(ev.tool || "tool") + "</div>" +
    "</div>" +
    '<div class="appr-sum">' + escHtml(ev.summary || "Agent 请求执行此工具") + "</div>" +
    (argsText
      ? '<div class="appr-args-label">参数（可编辑 JSON；允许时按编辑后内容执行）</div>' +
        '<textarea class="appr-args" rows="5" spellcheck="false">' + escHtml(argsText) + "</textarea>"
      : "") +
    '<div class="appr-actions">' +
    '<button type="button" class="btn primary sm appr-yes">允许执行</button>' +
    '<button type="button" class="btn sm appr-no">拒绝</button>' +
    '<button type="button" class="btn sm appr-copy">复制参数</button>' +
    '<button type="button" class="btn sm appr-reset">重置</button>' +
    "</div>";

  var yesBtn = card.querySelector(".appr-yes");
  var noBtn = card.querySelector(".appr-no");
  var copyBtn = card.querySelector(".appr-copy");
  var resetBtn = card.querySelector(".appr-reset");
  var ta = card.querySelector(".appr-args");
  var done = false;

  async function reply(allow) {
    if (done) return;
    done = true;
    yesBtn.disabled = true;
    noBtn.disabled = true;
    if (copyBtn) copyBtn.disabled = true;
    if (resetBtn) resetBtn.disabled = true;
    if (ta) ta.readOnly = true;
    var body = { id: ev.id, allow: allow };
    if (allow && ta) {
      var edited = ta.value.trim();
      if (edited && edited !== originalArgs) {
        try {
          body.args = JSON.parse(edited);
        } catch (e) {
          done = false;
          yesBtn.disabled = false;
          noBtn.disabled = false;
          if (copyBtn) copyBtn.disabled = false;
          if (resetBtn) resetBtn.disabled = false;
          if (ta) ta.readOnly = false;
          showLabToast("参数 JSON 无效", e.message || String(e));
          updateApprovalBanner();
          return;
        }
      }
    }
    try {
      await api("/api/lab/approve", {
        method: "POST",
        body: JSON.stringify(body),
      });
      card.classList.remove("pending");
      card.classList.add(allow ? "ok" : "deny");
      var note = allow ? "✓ 已允许，Agent 继续" : "✕ 已拒绝";
      if (allow && body.args) note += "（参数已改）";
      card.querySelector(".appr-actions").textContent = note;
    } catch (e) {
      card.classList.remove("pending");
      card.querySelector(".appr-actions").innerHTML =
        '<span class="ft-err">提交失败: ' + escHtml(e.message) + "</span>";
    }
    updateApprovalBanner();
  }

  yesBtn.addEventListener("click", function () { reply(true); });
  noBtn.addEventListener("click", function () { reply(false); });
  if (copyBtn) {
    copyBtn.addEventListener("click", function () {
      if (ta) copyTextToClipboard(ta.value, copyBtn);
    });
  }
  if (resetBtn && ta) {
    resetBtn.addEventListener("click", function () { ta.value = originalArgs; });
  }
  toolLog.appendChild(card);
  card.scrollIntoView({ behavior: "smooth", block: "nearest" });
  updateApprovalBanner();
  try { yesBtn.focus(); } catch (_) {}
}

/* ── 9. File panel ── */

var fileSortMode = "name";
try { fileSortMode = localStorage.getItem("lumen-file-sort") || "name"; } catch (_) {}

function sortFileEntries(files, mode) {
  mode = mode || "name";
  var list = (files || []).slice();
  list.sort(function (a, b) {
    if (!!a.isDir !== !!b.isDir) return a.isDir ? -1 : 1;
    if (mode === "size") return (b.size || 0) - (a.size || 0);
    if (mode === "mtime") return String(b.mtime || "").localeCompare(String(a.mtime || ""));
    return String(a.name || a.path || "").localeCompare(String(b.name || b.path || ""), "zh");
  });
  return list;
}

async function refreshFiles() {
  var tree = $("fileTree");
  var cwdEl = $("fileCwd");
  if (!tree || !activeProject) return;

  renderFileBreadcrumb();
  loadFileRecent();

  try {
    var data = await api("/api/lab/files?project_id=" + activeProject.slug + "&path=" + encodeURIComponent(fileCwd || "."));
    var files = data.files || [];
    files = sortFileEntries(files, fileSortMode);
    tree.innerHTML =
      '<div class="file-sel-bar">' +
      '<button type="button" class="btn sm" id="fileSelAll">全选</button> ' +
      '<button type="button" class="btn sm" id="fileSelNone">清除</button> ' +
      '<select id="fileSort" class="mode-sel" title="排序" style="height:28px;font-size:11px">' +
      '<option value="name"' + (fileSortMode === "name" ? " selected" : "") + ">名</option>" +
      '<option value="mtime"' + (fileSortMode === "mtime" ? " selected" : "") + ">时</option>" +
      '<option value="size"' + (fileSortMode === "size" ? " selected" : "") + ">大小</option>" +
      "</select> " +
      '<button type="button" class="btn sm" id="fileMkdir">新建目录</button> ' +
      '<button type="button" class="btn sm" id="fileNew">新建文件</button> ' +
      '<button type="button" class="btn sm" id="fileSelRename">重命名</button> ' +
      '<button type="button" class="btn sm" id="fileSelCopy">复制</button> ' +
      '<button type="button" class="btn sm" id="fileSelDownload">下载</button> ' +
      '<button type="button" class="btn sm" id="fileSelZip">打包 ZIP</button> ' +
      '<button type="button" class="btn sm" id="fileSelDelete">删除所选</button> ' +
      '<button type="button" class="btn sm" id="fileSelDiff">对比所选</button> ' +
      '<button type="button" class="btn sm" id="fileSelAttach">引用到对话</button> ' +
      '<button type="button" class="btn sm" id="fileSelStar">☆ 收藏</button> ' +
      '<span class="hint" id="fileSelCount">0 选中</span></div>' +
      files.map(function (f) {
        var icon = f.isDir ? "📁" : fileIcon(f.name || f.path);
        var path = f.path || f.name;
        var meta = f.isDir ? "" : fmtSize(f.size);
        if (!f.isDir && f.mtime) meta += " · " + String(f.mtime).slice(0, 16).replace("T", " ");
        var starred = isStarredFile(path);
        return '<div class="ft-row' + (f.isDir ? " dir" : "") + (starred ? " starred" : "") + '" data-path="' + escHtml(path) + '" data-isdir="' + (f.isDir ? "1" : "0") + '" data-preview="' + escHtml(f.previewKind || "") + '">' +
          '<input type="checkbox" class="ft-check" data-path="' + escHtml(path) + '" />' +
          '<span style="flex-shrink:0;font-size:.9rem">' + icon + "</span>" +
          '<span class="ft-name">' + (starred ? "★ " : "") + escHtml(f.name || f.path) + "</span>" +
          (meta ? '<span class="ft-size">' + escHtml(meta) + "</span>" : "") +
          "</div>";
      }).join("");
    var sortEl = $("fileSort");
    if (sortEl) {
      sortEl.addEventListener("change", function () {
        fileSortMode = sortEl.value || "name";
        try { localStorage.setItem("lumen-file-sort", fileSortMode); } catch (_) {}
        refreshFiles();
      });
    }

    function updateSelCount() {
      var n = tree.querySelectorAll(".ft-check:checked").length;
      var c = $("fileSelCount");
      if (c) c.textContent = n + " 选中";
    }
    tree.querySelectorAll(".ft-check").forEach(function (cb) {
      cb.addEventListener("click", function (e) { e.stopPropagation(); updateSelCount(); });
    });
    var selAll = $("fileSelAll");
    if (selAll) selAll.addEventListener("click", function () {
      tree.querySelectorAll(".ft-check").forEach(function (cb) { cb.checked = true; });
      updateSelCount();
    });
    var selNone = $("fileSelNone");
    if (selNone) selNone.addEventListener("click", function () {
      tree.querySelectorAll(".ft-check").forEach(function (cb) { cb.checked = false; });
      updateSelCount();
    });
    var selDel = $("fileSelDelete");
    if (selDel) selDel.addEventListener("click", function () { batchDeleteSelected(); });
    var mkdirBtn = $("fileMkdir");
    if (mkdirBtn) mkdirBtn.addEventListener("click", function () { fileMkdirPrompt(); });
    var newFileBtn = $("fileNew");
    if (newFileBtn) newFileBtn.addEventListener("click", function () { fileNewPrompt(); });
    var renameBtn = $("fileSelRename");
    if (renameBtn) renameBtn.addEventListener("click", function () { fileRenameSelected(); });
    var copyBtn = $("fileSelCopy");
    if (copyBtn) copyBtn.addEventListener("click", function () { fileCopySelected(); });
    var dlBtn = $("fileSelDownload");
    if (dlBtn) dlBtn.addEventListener("click", function () { fileDownloadSelected(); });
    var zipBtn = $("fileSelZip");
    if (zipBtn) zipBtn.addEventListener("click", function () { fileZipSelected(); });
    var attachBtn = $("fileSelAttach");
    if (attachBtn) attachBtn.addEventListener("click", function () { attachSelectedToComposer(); });
    var starBtn = $("fileSelStar");
    if (starBtn) starBtn.addEventListener("click", function () { starSelectedFiles(); });
    var selDiff = $("fileSelDiff");
    if (selDiff) selDiff.addEventListener("click", function () {
      var paths = [];
      tree.querySelectorAll(".ft-check:checked").forEach(function (cb) {
        paths.push(cb.getAttribute("data-path"));
      });
      if (paths.length !== 2) {
        alert("请恰好勾选 2 个文件做 Diff");
        return;
      }
      if ($("diffPathA")) $("diffPathA").value = paths[0];
      if ($("diffPathB")) $("diffPathB").value = paths[1];
      var tab = document.querySelector('.insp-tab[data-pane="diff"]');
      if (tab) tab.click();
      runFileDiff();
    });

    tree.querySelectorAll(".ft-row").forEach(function (row) {
      row.addEventListener("click", function (e) {
        if (e.target && e.target.classList && e.target.classList.contains("ft-check")) return;
        var p = row.dataset.path;
        var isDir = row.dataset.isdir === "1";
        var pk = row.dataset.preview;
        if (isDir) {
          fileCwd = p;
          refreshFiles();
        } else {
          previewFile(p, pk);
        }
      });
    });
  } catch (e) {
    tree.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

async function batchDeleteSelected() {
  var tree = $("fileTree");
  if (!tree || !activeProject) return;
  var paths = [];
  tree.querySelectorAll(".ft-check:checked").forEach(function (cb) {
    paths.push(cb.getAttribute("data-path"));
  });
  if (!paths.length) {
    alert("请先勾选文件");
    return;
  }
  if (!confirm("删除 " + paths.length + " 项？不可恢复。")) return;
  try {
    await api("/api/lab/files/delete?project_id=" + activeProject.slug, {
      method: "POST",
      body: JSON.stringify({ paths: paths }),
    });
    refreshFiles();
    loadFileTree();
    loadArtifacts();
  } catch (e) {
    alert("删除失败: " + e.message);
  }
}

function fileUp() {
  if (!fileCwd || fileCwd === ".") return;
  // Get parent: "a/b/c" → "a/b"; "a" → "."
  var parts = fileCwd.split("/");
  parts.pop();
  fileCwd = parts.length === 0 ? "." : parts.join("/");
  refreshFiles();
}

function renderFileBreadcrumb() {
  var cwdEl = $("fileCwd");
  if (!cwdEl) return;
  var cwd = fileCwd || ".";
  if (cwd === ".") {
    cwdEl.innerHTML = '<button type="button" class="bc-seg" data-path=".">.</button>';
  } else {
    var parts = cwd.split("/").filter(Boolean);
    var html = '<button type="button" class="bc-seg" data-path=".">.</button>';
    var acc = [];
    parts.forEach(function (seg) {
      acc.push(seg);
      html += ' <span class="bc-sep">/</span> <button type="button" class="bc-seg" data-path="' +
        escHtml(acc.join("/")) + '">' + escHtml(seg) + "</button>";
    });
    cwdEl.innerHTML = html;
  }
  cwdEl.querySelectorAll(".bc-seg").forEach(function (btn) {
    btn.addEventListener("click", function () {
      fileCwd = btn.getAttribute("data-path") || ".";
      refreshFiles();
    });
  });
}

function selectedFilePaths() {
  var tree = $("fileTree");
  var paths = [];
  if (!tree) return paths;
  tree.querySelectorAll(".ft-check:checked").forEach(function (cb) {
    paths.push(cb.getAttribute("data-path"));
  });
  return paths;
}

async function fileMkdirPrompt() {
  if (!activeProject) return;
  var name = prompt("新建目录名（相对当前目录 " + (fileCwd || ".") + "）", "notes");
  if (!name) return;
  name = name.replace(/^\/+/, "").replace(/\.\./g, "");
  var path = (fileCwd && fileCwd !== ".") ? (fileCwd + "/" + name) : name;
  try {
    await api("/api/lab/files/mkdir?project_id=" + activeProject.slug, {
      method: "POST",
      body: JSON.stringify({ path: path }),
    });
    refreshFiles();
    loadFileTree();
  } catch (e) {
    alert("创建失败: " + e.message);
  }
}

async function fileRenameSelected() {
  if (!activeProject) return;
  var paths = selectedFilePaths();
  if (paths.length !== 1) {
    alert("请恰好勾选 1 项重命名");
    return;
  }
  var from = paths[0];
  var base = from.split("/").pop();
  var toName = prompt("重命名为（可含相对路径）", base);
  if (!toName) return;
  toName = toName.replace(/^\/+/, "").replace(/\.\./g, "");
  var dir = from.indexOf("/") >= 0 ? from.slice(0, from.lastIndexOf("/")) : "";
  var to = toName.indexOf("/") >= 0 ? toName : (dir ? dir + "/" + toName : toName);
  try {
    await api("/api/lab/files/rename?project_id=" + activeProject.slug, {
      method: "POST",
      body: JSON.stringify({ from: from, to: to }),
    });
    refreshFiles();
    loadFileTree();
  } catch (e) {
    alert("重命名失败: " + e.message);
  }
}

async function fileCopySelected() {
  if (!activeProject) return;
  var paths = selectedFilePaths();
  if (paths.length !== 1) {
    alert("请恰好勾选 1 项复制");
    return;
  }
  var from = paths[0];
  var base = from.split("/").pop();
  var def = from.replace(/(\.[^.]+)?$/, function (m) { return "_copy" + (m || ""); });
  if (def === from) def = from + "_copy";
  var to = prompt("复制到路径", def.indexOf(base) >= 0 ? def : from + "_copy");
  if (!to) return;
  to = to.replace(/^\/+/, "").replace(/\.\./g, "");
  try {
    await api("/api/lab/files/copy?project_id=" + activeProject.slug, {
      method: "POST",
      body: JSON.stringify({ from: from, to: to }),
    });
    refreshFiles();
    loadFileTree();
  } catch (e) {
    alert("复制失败: " + e.message);
  }
}

function fileDownloadSelected() {
  if (!activeProject) return;
  var paths = selectedFilePaths();
  if (!paths.length) {
    alert("请先勾选文件");
    return;
  }
  paths.forEach(function (p) {
    var url = labPath("/api/lab/files/download?project_id=" + activeProject.slug + "&path=" + encodeURIComponent(p));
    window.open(url, "_blank");
  });
}

async function fileZipSelected() {
  if (!activeProject) return;
  var paths = selectedFilePaths();
  if (!paths.length) {
    alert("请先勾选文件或目录");
    return;
  }
  try {
    var res = await fetch(labPath("/api/lab/files/zip?project_id=" + activeProject.slug), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ paths: paths }),
    });
    if (!res.ok) {
      var txt = await res.text();
      throw new Error(txt || ("HTTP " + res.status));
    }
    var blob = await res.blob();
    var a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = activeProject.slug + "-selection.zip";
    document.body.appendChild(a);
    a.click();
    setTimeout(function () {
      URL.revokeObjectURL(a.href);
      a.remove();
    }, 1000);
  } catch (e) {
    alert("打包失败: " + e.message);
  }
}

var previewEditPath = "";
var previewEditOriginal = "";

async function previewFile(path, previewKind) {
  var preview = $("filePreview");
  if (!preview || !activeProject) return Promise.resolve();
  previewEditPath = path || "";
  previewEditOriginal = "";
  try {
    var data = await api("/api/lab/files/content?project_id=" + activeProject.slug + "&path=" + encodeURIComponent(path));
    var prov = await loadProvenance(path);
    var kind = previewKind || data.previewKind || "text";
    var content = data.content || "";
    previewEditOriginal = content;

    var bodyHtml = "";
    var dl = labPath("/api/lab/files/download?project_id=" + activeProject.slug + "&path=" + encodeURIComponent(path));
    var canEdit = !(kind === "molecule" || kind === "office" || kind === "image" || kind === "pdf" || kind === "binary");
    if (content.length > 2 * 1024 * 1024) canEdit = false;

    switch (kind) {
      case "markdown":
        bodyHtml = '<div class="fp-view" id="fpView"><div class="fp-md">' + renderMarkdown(content) + "</div></div>" +
          '<textarea id="fpEditor" class="fp-editor" hidden></textarea>';
        break;
      case "image":
        bodyHtml = '<img src="' + dl + '" alt="' + escHtml(path) + '" style="max-width:100%;border-radius:8px" loading="lazy" />';
        canEdit = false;
        break;
      case "pdf":
        bodyHtml = '<div class="hint">📕 PDF (' + fmtSize(data.size) + ') — <a href="' + dl + '" target="_blank">下载 / 新窗口打开</a></div>' +
          '<iframe class="fp-pdf" src="' + dl + '" title="pdf"></iframe>';
        canEdit = false;
        break;
      case "office":
        bodyHtml = '<div class="hint">📄 Office 文本抽取 (' + escHtml(data.officeKind || "office") + ") · " + fmtSize(data.size) +
          ' — <a href="' + dl + '" target="_blank">下载原文件</a></div>' +
          (data.hint ? '<div class="hint">' + escHtml(data.hint) + "</div>" : "") +
          '<pre class="fp-body">' + escHtml(content || "(无文本)") + "</pre>";
        canEdit = false;
        break;
      case "molecule":
        bodyHtml = '<div class="hint">🧬 分子结构 — <button type="button" class="btn sm" id="fpOpenMol">在 3Dmol 中打开</button></div>' +
          '<pre class="fp-body">' + escHtml(content.slice(0, 2000)) + "</pre>";
        canEdit = false;
        break;
      case "binary":
        bodyHtml = '<div class="hint">不支持内联预览 (' + fmtSize(data.size) + ') — <a href="' + dl + '" target="_blank">下载</a></div>';
        canEdit = false;
        break;
      default:
        bodyHtml = '<div class="fp-view" id="fpView"><pre class="fp-body">' + escHtml(content) + "</pre></div>" +
          '<textarea id="fpEditor" class="fp-editor" hidden></textarea>';
        break;
    }
    preview.innerHTML =
      '<div class="fp-hd">📄 ' + escHtml(data.path || path) + " (" + fmtSize(data.size) + ") " +
      (canEdit ? '<button type="button" class="btn sm" id="fpEdit">编辑</button> ' +
        '<button type="button" class="btn sm primary" id="fpSave" hidden>保存</button> ' +
        '<button type="button" class="btn sm" id="fpAppend" hidden>追加保存</button> ' +
        '<button type="button" class="btn sm" id="fpCancelEdit" hidden>取消</button> ' : "") +
      '<button type="button" class="btn sm" id="fpCopyPath" title="复制路径">路径</button> ' +
      '<button type="button" class="btn sm" id="fpAtPath" title="插入到输入框">@引用</button> ' +
      '<button type="button" class="btn sm" id="fpExpand">全屏</button> ' +
      '<span class="hint" id="fpEditHint"></span></div>' +
      bodyHtml +
      '<div class="pv">' + prov + "</div>";
    bindWorkspaceRefs(preview);
    bindCodeCopy(preview);
    var exp = $("fpExpand");
    if (exp) exp.addEventListener("click", function () { openPreviewModal(path); });
    var cp = $("fpCopyPath");
    if (cp) cp.addEventListener("click", function () {
      copyTextToClipboard(path, cp);
    });
    var atp = $("fpAtPath");
    if (atp) atp.addEventListener("click", function () {
      var inp = $("promptInput");
      if (!inp) return;
      var tag = "@" + path + " ";
      inp.value = (inp.value ? inp.value + (/\s$/.test(inp.value) ? "" : " ") : "") + tag;
      inp.focus();
      saveComposerDraft();
    });
    var molBtn = $("fpOpenMol");
    if (molBtn) {
      molBtn.addEventListener("click", function () {
        openMoleculeFromContent(path, content, kind);
      });
    }
    var editBtn = $("fpEdit");
    var saveBtn = $("fpSave");
    var appendBtn = $("fpAppend");
    var cancelBtn = $("fpCancelEdit");
    var editor = $("fpEditor");
    var view = $("fpView");
    var hint = $("fpEditHint");
    if (editBtn && editor) {
      editBtn.addEventListener("click", function () {
        editor.value = previewEditOriginal;
        editor.hidden = false;
        if (view) view.hidden = true;
        editBtn.hidden = true;
        if (saveBtn) saveBtn.hidden = false;
        if (appendBtn) appendBtn.hidden = false;
        if (cancelBtn) cancelBtn.hidden = false;
        if (hint) hint.textContent = "编辑中…（保存=覆盖 · 追加保存=只追加新增段）";
        editor.focus();
      });
    }
    if (cancelBtn && editor) {
      cancelBtn.addEventListener("click", function () {
        editor.hidden = true;
        if (view) view.hidden = false;
        if (editBtn) editBtn.hidden = false;
        if (saveBtn) saveBtn.hidden = true;
        if (appendBtn) appendBtn.hidden = true;
        cancelBtn.hidden = true;
        if (hint) hint.textContent = "";
      });
    }
    if (saveBtn && editor) {
      saveBtn.addEventListener("click", async function () {
        try {
          await api("/api/lab/files/write?project_id=" + activeProject.slug, {
            method: "POST",
            body: JSON.stringify({ path: path, content: editor.value }),
          });
          if (hint) hint.textContent = "已保存";
          await previewFile(path, kind);
          loadFileRecent();
        } catch (e) {
          if (hint) hint.textContent = "保存失败";
          alert("保存失败: " + e.message);
        }
      });
    }
    if (appendBtn && editor) {
      appendBtn.addEventListener("click", async function () {
        // append only the suffix beyond original snapshot
        var full = editor.value || "";
        var orig = previewEditOriginal || "";
        var chunk = full;
        if (full.indexOf(orig) === 0) chunk = full.slice(orig.length);
        if (!chunk) {
          alert("没有相对原文的新增内容可追加（请在文末继续写，或用「保存」覆盖）");
          return;
        }
        try {
          await api("/api/lab/files/append?project_id=" + activeProject.slug, {
            method: "POST",
            body: JSON.stringify({ path: path, content: chunk }),
          });
          if (hint) hint.textContent = "已追加 " + chunk.length + " 字符";
          await previewFile(path, kind);
          loadFileRecent();
        } catch (e) {
          if (hint) hint.textContent = "追加失败";
          alert("追加失败: " + e.message);
        }
      });
    }
    // Auto-open molecule tab for pdb/mol files
    if (kind === "molecule" && content) {
      openMoleculeFromContent(path, content, kind);
    }
  } catch (e) {
    preview.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

async function fileNewPrompt() {
  if (!activeProject) return;
  var name = prompt("新建文件名（相对当前目录 " + (fileCwd || ".") + "）\n扩展名自动套模板: .md .py .r .sh .csv .json", "notes/draft.md");
  if (!name) return;
  name = name.replace(/^\/+/, "").replace(/\.\./g, "");
  var path = (fileCwd && fileCwd !== "." && name.indexOf("/") < 0) ? (fileCwd + "/" + name) : name;
  var content = fileTemplateContent(path);
  try {
    await api("/api/lab/files/write?project_id=" + activeProject.slug, {
      method: "POST",
      body: JSON.stringify({ path: path, content: content }),
    });
    refreshFiles();
    loadFileTree();
    previewFile(path, "");
    showLabToast("已创建", path + (content ? "（含模板）" : ""));
  } catch (e) {
    alert("新建失败: " + e.message);
  }
}

function starKey() {
  return "lumen-stars:" + ((activeProject && activeProject.slug) || "none");
}
function loadStarredFiles() {
  try { return JSON.parse(localStorage.getItem(starKey()) || "[]"); } catch (_) { return []; }
}
function saveStarredFiles(list) {
  try { localStorage.setItem(starKey(), JSON.stringify(list.slice(0, 80))); } catch (_) {}
}
function isStarredFile(path) {
  return loadStarredFiles().indexOf(path) >= 0;
}
function starSelectedFiles() {
  var paths = selectedFilePaths().filter(function (p) { return p && p !== "."; });
  if (!paths.length) { alert("请先勾选文件"); return; }
  var list = loadStarredFiles();
  paths.forEach(function (p) {
    if (list.indexOf(p) < 0) list.unshift(p);
  });
  saveStarredFiles(list);
  renderStarredFiles();
  refreshFiles();
  showLabToast("已收藏", paths.length + " 项");
}
function renderStarredFiles() {
  var el = $("fileStars");
  if (!el || !activeProject) return;
  var list = loadStarredFiles();
  if (!list.length) {
    el.innerHTML = '<div class="hint">暂无收藏 — 勾选后点 ☆ 收藏</div>';
    return;
  }
  el.innerHTML = list.map(function (p, i) {
    return '<div class="star-row">' +
      '<button type="button" class="btn sm star-open" data-path="' + escHtml(p) + '">★ ' + escHtml(p) + "</button>" +
      '<button type="button" class="btn sm star-del" data-i="' + i + '">×</button></div>';
  }).join("");
  el.querySelectorAll(".star-open").forEach(function (btn) {
    btn.addEventListener("click", function () {
      previewFile(btn.getAttribute("data-path"), "");
    });
  });
  el.querySelectorAll(".star-del").forEach(function (btn) {
    btn.addEventListener("click", function (e) {
      e.stopPropagation();
      var list = loadStarredFiles();
      list.splice(parseInt(btn.getAttribute("data-i"), 10), 1);
      saveStarredFiles(list);
      renderStarredFiles();
      refreshFiles();
    });
  });
}
function attachSelectedToComposer() {
  var paths = selectedFilePaths().filter(function (p) { return p && p !== "."; });
  if (!paths.length) { alert("请先勾选文件"); return; }
  paths.forEach(function (p) {
    if (pendingAttachments.indexOf(p) < 0) pendingAttachments.push(p);
  });
  renderAttachmentChips();
  var inp = $("promptInput");
  if (inp) {
    var tags = paths.map(function (p) { return "@" + p; }).join(" ") + " ";
    inp.value = (inp.value ? inp.value.replace(/\s*$/, " ") : "") + tags;
    inp.focus();
    saveComposerDraft();
  }
  showLabToast("已引用到对话", paths.length + " 个路径");
}

var molViewerInstance = null;

function openMoleculeFromContent(path, content, kind) {
  var tab = document.querySelector('.insp-tab[data-pane="molecule"]');
  if (tab) tab.click();
  ensureMoleculeViewer(function (viewer) {
    try {
      viewer.clear();
      var fmt = "pdb";
      var lower = (path || "").toLowerCase();
      if (lower.endsWith(".sdf") || lower.endsWith(".mol")) fmt = "sdf";
      else if (lower.endsWith(".cif")) fmt = "cif";
      viewer.addModel(content, fmt);
      viewer.setStyle({}, { stick: {}, sphere: { scale: 0.25 } });
      if (fmt === "pdb") viewer.setStyle({}, { cartoon: { color: "#c28b4b" }, stick: {} });
      viewer.zoomTo();
      viewer.render();
      var label = $("molLabel");
      if (label) label.textContent = path || "structure";
    } catch (e) {
      var el = $("molViewer");
      if (el) el.innerHTML = '<p class="hint">无法渲染: ' + escHtml(String(e.message || e)) + "</p>";
    }
  });
}

function ensureMoleculeViewer(cb) {
  var el = $("molViewer");
  if (!el) return;
  if (typeof $3Dmol !== "undefined" && molViewerInstance) {
    cb(molViewerInstance);
    return;
  }
  function boot() {
    if (typeof $3Dmol === "undefined") {
      el.innerHTML = '<p class="hint">3Dmol 加载失败</p>';
      return;
    }
    el.innerHTML = "";
    molViewerInstance = $3Dmol.createViewer("molViewer", { backgroundColor: "#fbf9f6" });
    if (typeof molLoaded !== "undefined") molLoaded = true;
    cb(molViewerInstance);
  }
  if (typeof $3Dmol !== "undefined") {
    boot();
    return;
  }
  var script = document.createElement("script");
  script.src = "https://3Dmol.org/build/3Dmol-min.js";
  script.onload = boot;
  document.head.appendChild(script);
}

async function loadProvenance(path) {
  if (!activeProject) return "";
  try {
    var data = await api("/api/lab/provenance?project_id=" + activeProject.slug + "&path=" + encodeURIComponent(path));
    if (!data.count) return '<div class="pv-empty">无溯源记录</div>';
    return data.records.map(function (r) {
      var mcp = (r.mcp_calls || []).map(function (m) { return m.tool + '("' + (m.query || "") + '")'; }).join(", ");
      return '<div class="pv-row">' +
        '<span class="pv ts">' + (r.ts || "").slice(0, 19).replace("T", " ") + "</span>" +
        '<span class="pv tag">' + (r.kind || "artifact") + "</span>" +
        '<span class="pv model">' + (r.model || "—") + "</span>" +
        (mcp ? '<span class="pv mcp">🔗 ' + escHtml(mcp) + "</span>" : "") +
        (r.content_hash ? '<span class="pv hash">#' + r.content_hash.slice(7, 15) + "</span>" : "") +
        "</div>";
    }).join("");
  } catch (_) {
    return "";
  }
}

/* ── 10. Composer ── */

var lastPrompt = "";
var lastMode = "agent";

function submitPrompt() {
  var inp = $("promptInput");
  if (!inp) return;
  var prompt = inp.value.trim();
  if (!prompt && !pendingAttachments.length) return;
  prompt = injectAttachmentsIntoPrompt(prompt || "请查看附件并分析。");
  lastPrompt = prompt;
  lastMode = ($("chatMode") && $("chatMode").value) || "agent";
  try { localStorage.setItem("lumen-last-prompt", lastPrompt); } catch (_) {}
  inp.value = "";
  inp.style.height = "auto";
  clearComposerDraft();
  var rerun = $("btnRerun");
  if (rerun) rerun.disabled = false;
  streamChat(prompt, lastMode).catch(function (err) {
    addErrorBubble($("chatScroll"), err.message);
  });
}

function rerunLastPrompt() {
  if (!lastPrompt) {
    try { lastPrompt = localStorage.getItem("lumen-last-prompt") || ""; } catch (_) {}
  }
  if (!lastPrompt) {
    alert("没有可重跑的上一条消息");
    return;
  }
  var mode = lastMode || ($("chatMode") && $("chatMode").value) || "agent";
  streamChat(lastPrompt, mode).catch(function (err) {
    addErrorBubble($("chatScroll"), err.message);
  });
}

/* ── 11. Chrome wiring ── */

// Dark mode + compact
(function () {
  var toggle = $("dmToggle");
  if (toggle) {
    if (localStorage.getItem("lumen-dark") === "1") {
      document.documentElement.classList.add("dark");
      toggle.textContent = "☀️";
    }
    toggle.addEventListener("click", function () {
      var isDark = document.documentElement.classList.toggle("dark");
      localStorage.setItem("lumen-dark", isDark ? "1" : "0");
      toggle.textContent = isDark ? "☀️" : "🌙";
    });
  }
  if (localStorage.getItem("lumen-compact") === "1") {
    document.documentElement.classList.add("compact");
    if ($("btnCompact")) $("btnCompact").textContent = "宽松";
  }
  $("btnCompact") && $("btnCompact").addEventListener("click", toggleCompactMode);
  // ask notification permission lazily once
  try {
    if (typeof Notification !== "undefined" && Notification.permission === "default") {
      // don't prompt aggressively — only after first send
    }
  } catch (_) {}
})();

// Mobile sidebar / inspector toggles + mask
(function () {
  function toggleSide() {
    var side = $("sidebarPanel");
    var mask = $("sideMask");
    if (side) side.classList.toggle("open");
    if (mask) mask.hidden = !(side && side.classList.contains("open"));
  }
  function toggleInsp() {
    var insp = $("inspectorPanel");
    if (insp) insp.classList.toggle("open");
  }
  var btnSide = $("btnSide");
  var btnInsp = $("btnInsp");
  var mask = $("sideMask");
  if (btnSide) btnSide.addEventListener("click", toggleSide);
  if (btnInsp) btnInsp.addEventListener("click", toggleInsp);
  if (mask) mask.addEventListener("click", function () {
    var side = $("sidebarPanel");
    if (side) side.classList.remove("open");
    mask.hidden = true;
  });
})();

// Refresh all
$("refreshAllBtn") && $("refreshAllBtn").addEventListener("click", function () {
  refreshHealth();
  loadProjects();
  refreshFiles();
  loadSessions();
  loadSkills();
  loadComputeJobs();
});

// New project (optional seed template)
var seedExamplesCache = [];
$("newProjectBtn") && $("newProjectBtn").addEventListener("click", async function () {
  var title = prompt("项目名称");
  if (!title) return;
  var template = "";
  try {
    if (!seedExamplesCache.length) {
      var h = await api("/api/lab/health");
      seedExamplesCache = (h.research_pack && h.research_pack.seed_examples) || [];
    }
  } catch (_) {}
  if (seedExamplesCache.length) {
    var list = seedExamplesCache.slice(0, 20).join(", ");
    var pick = prompt("可选 seed 模板（留空跳过）\n可用: " + list, "");
    if (pick != null) template = pick.trim();
  }
  try {
    activeProject = await api("/api/lab/projects", {
      method: "POST",
      body: JSON.stringify({ title: title, template: template || undefined }),
    });
    await loadProjects();
    refreshFiles();
    loadSessions();
    showLabToast("课题已创建", template ? ("模板: " + template) : activeProject.slug);
  } catch (e) { alert(e.message); }
});

// New conversation
$("newConvBtn") && $("newConvBtn").addEventListener("click", newConv);

// File panel buttons
$("fileUp") && $("fileUp").addEventListener("click", fileUp);
$("fileRefresh") && $("fileRefresh").addEventListener("click", function () { refreshFiles(); });

// Composer
$("promptInput") && $("promptInput").addEventListener("keydown", function (e) {
  if (e.key === "Enter" && !e.shiftKey) {
    // defer to @menu when open
    if (atMenuState && atMenuState.active && atMenuState.items && atMenuState.items.length) return;
    e.preventDefault();
    submitPrompt();
  }
});
var draftSaveTimer = null;
$("promptInput") && $("promptInput").addEventListener("input", function () {
  clearTimeout(draftSaveTimer);
  draftSaveTimer = setTimeout(saveComposerDraft, 400);
});
// Paste images/files from clipboard into attachments
$("promptInput") && $("promptInput").addEventListener("paste", async function (e) {
  var cd = e.clipboardData;
  if (!cd) return;
  var files = [];
  if (cd.files && cd.files.length) {
    files = Array.prototype.slice.call(cd.files);
  } else if (cd.items) {
    for (var i = 0; i < cd.items.length; i++) {
      var it = cd.items[i];
      if (it.kind === "file") {
        var f = it.getAsFile();
        if (f) files.push(f);
      }
    }
  }
  if (!files.length) return;
  e.preventDefault();
  try {
    // name clipboard images
    files = files.map(function (f, idx) {
      if (f.name && f.name !== "image.png" && f.name !== "blob") return f;
      var ext = (f.type && f.type.split("/")[1]) || "png";
      return new File([f], "paste-" + Date.now() + "-" + idx + "." + ext, { type: f.type || "image/png" });
    });
    await uploadFilesList(files);
  } catch (err) {
    alert("粘贴上传失败: " + err.message);
  }
});
$("composer") && $("composer").addEventListener("submit", function (e) {
  e.preventDefault();
  submitPrompt();
});
$("btnRerun") && $("btnRerun").addEventListener("click", function () { rerunLastPrompt(); });
// restore last prompt availability
(function () {
  try {
    lastPrompt = localStorage.getItem("lumen-last-prompt") || "";
    if (lastPrompt && $("btnRerun")) $("btnRerun").disabled = false;
  } catch (_) {}
})();

// Stop button
$("btnStop") && $("btnStop").addEventListener("click", function () {
  if (currentAbort) {
    currentAbort.abort();
    currentAbort = null;
    setRunStatus(false);
  }
});

async function uploadFilesList(files) {
  files = files ? Array.prototype.slice.call(files) : [];
  if (!files.length) return [];
  if (!activeProject) await ensureProject();
  var uploadedList = [];
  for (var i = 0; i < files.length; i++) {
    var file = files[i];
    var fd = new FormData();
    fd.append("file", file);
    var res = await fetch(labPath("/api/lab/files/upload?project_id=" + activeProject.slug), {
      method: "POST",
      body: fd,
    });
    if (!res.ok) {
      var txt = await res.text();
      throw new Error(file.name + ": " + txt);
    }
    var uploaded = file.name;
    try {
      var body = await res.json();
      if (body.uploaded) uploaded = body.uploaded;
      else if (body.path) uploaded = body.path;
    } catch (_) {}
    if (pendingAttachments.indexOf(uploaded) < 0) pendingAttachments.push(uploaded);
    uploadedList.push(uploaded);
  }
  renderAttachmentChips();
  var inp = $("promptInput");
  if (inp) {
    var tags = uploadedList.map(function (u) { return "[附件] " + u; }).join("\n") + "\n";
    inp.value = tags + (inp.value || "");
    inp.focus();
  }
  var hint = $("composerHint");
  if (hint) hint.textContent = "已上传 " + uploadedList.length + " 个附件";
  await refreshFiles();
  loadFileTree();
  return uploadedList;
}

// File upload — multi-file; inject paths into composer for agent context
$("fileUpload") && $("fileUpload").addEventListener("change", async function () {
  var files = this.files ? Array.prototype.slice.call(this.files) : [];
  if (!files.length) return;
  try {
    var uploadedList = await uploadFilesList(files);
    var filesTab = document.querySelector('.insp-tab[data-pane="files"]');
    if (filesTab) filesTab.click();
    if (uploadedList.length) previewFile(uploadedList[0], "");
  } catch (e) {
    alert("上传失败: " + e.message);
  }
  this.value = "";
});

// Drag-drop attachments onto composer
(function bindComposerDrop() {
  var wrap = $("composerDrop") || $("composer");
  var overlay = $("dropOverlay");
  if (!wrap) return;
  var dragDepth = 0;
  function hasFiles(e) {
    var dt = e.dataTransfer;
    if (!dt) return false;
    if (dt.types && (dt.types.indexOf("Files") >= 0 || (dt.types.contains && dt.types.contains("Files")))) return true;
    return dt.files && dt.files.length > 0;
  }
  wrap.addEventListener("dragenter", function (e) {
    if (!hasFiles(e)) return;
    e.preventDefault();
    dragDepth++;
    if (overlay) overlay.hidden = false;
    wrap.classList.add("is-dragover");
  });
  wrap.addEventListener("dragover", function (e) {
    if (!hasFiles(e)) return;
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
  });
  wrap.addEventListener("dragleave", function (e) {
    if (!hasFiles(e) && dragDepth === 0) return;
    dragDepth = Math.max(0, dragDepth - 1);
    if (dragDepth === 0) {
      if (overlay) overlay.hidden = true;
      wrap.classList.remove("is-dragover");
    }
  });
  wrap.addEventListener("drop", async function (e) {
    e.preventDefault();
    dragDepth = 0;
    if (overlay) overlay.hidden = true;
    wrap.classList.remove("is-dragover");
    var files = e.dataTransfer && e.dataTransfer.files;
    if (!files || !files.length) return;
    try {
      await uploadFilesList(files);
    } catch (err) {
      alert("上传失败: " + err.message);
    }
  });
})();

// Chips
function bindChipButton(btn) {
  if (!btn || btn._labChipBound) return;
  btn._labChipBound = true;
  btn.addEventListener("click", async function () {
    if (btn.dataset.brief) {
      try {
        var p = await ensureProject();
        var res = await api("/api/lab/brief", { method: "POST", body: JSON.stringify({ project_id: p.slug, topic: btn.dataset.brief }) });
        var chatScroll = $("chatScroll");
        var div = document.createElement("div");
        div.className = "chat-msg agent";
        div.textContent = "Brief 已写入 " + res.path;
        chatScroll.appendChild(div);
        div.scrollIntoView({ behavior: "smooth", block: "end" });
        setTimeout(refreshFiles, 1500);
      } catch (e) {
        var cs = $("chatScroll");
        var d2 = document.createElement("div");
        d2.className = "chat-msg agent";
        d2.textContent = "Brief 失败: " + e.message;
        cs.appendChild(d2);
      }
      return;
    }
    if (btn.dataset.prompt) streamChat(btn.dataset.prompt).catch(function (e) {
      addErrorBubble($("chatScroll"), e.message);
    });
  });
}
document.querySelectorAll(".chip").forEach(bindChipButton);

// Topbar system stats (hidden by default — OCS-quiet chrome)
(function () {
  var btn = $("btnTbOps");
  var ops = $("tbOps");
  if (!btn || !ops) return;
  try {
    if (localStorage.getItem("lumen-show-ops") === "1") ops.hidden = false;
  } catch (_) {}
  btn.addEventListener("click", function () {
    ops.hidden = !ops.hidden;
    try { localStorage.setItem("lumen-show-ops", ops.hidden ? "0" : "1"); } catch (_) {}
  });
})();

// Approval banner actions
(function () {
  var jump = $("approvalBannerJump");
  var allow = $("approvalBannerAllowAll");
  var deny = $("approvalBannerDeny");
  if (jump) {
    jump.addEventListener("click", function () {
      var pending = pendingApprovalCards();
      if (pending[0]) pending[0].scrollIntoView({ behavior: "smooth", block: "center" });
      focusActivityPane("tasks");
    });
  }
  if (allow) allow.addEventListener("click", function () { replyLatestPendingApproval(true); });
  if (deny) deny.addEventListener("click", function () { replyLatestPendingApproval(false); });
})();

// Center "⋯ more" menu for export/import/session ops
(function () {
  var btn = $("ctrMoreBtn");
  var menu = $("ctrMoreMenu");
  if (!btn || !menu) return;
  function close() {
    menu.hidden = true;
    btn.setAttribute("aria-expanded", "false");
  }
  function open() {
    menu.hidden = false;
    btn.setAttribute("aria-expanded", "true");
  }
  btn.addEventListener("click", function (e) {
    e.stopPropagation();
    if (menu.hidden) open(); else close();
  });
  menu.addEventListener("click", function () {
    // close after any action (file input opens picker first)
    setTimeout(close, 50);
  });
  document.addEventListener("click", function (e) {
    if (!menu.hidden && !menu.contains(e.target) && e.target !== btn) close();
  });
})();

function loadFavPrompts() {
  try {
    return JSON.parse(localStorage.getItem("lumen-fav-prompts") || "[]");
  } catch (_) {
    return [];
  }
}

function saveFavPrompts(list) {
  try { localStorage.setItem("lumen-fav-prompts", JSON.stringify(list.slice(0, 20))); } catch (_) {}
}

function renderFavChips() {
  var el = $("favChips");
  if (!el) return;
  var list = loadFavPrompts();
  if (!list.length) {
    el.innerHTML = "";
    return;
  }
  el.innerHTML = list.map(function (f, i) {
    var label = f.label || (f.prompt || "").slice(0, 18) || "收藏";
    return '<div class="fav-chip-row">' +
      '<button type="button" class="chip fav-run" data-i="' + i + '">' + escHtml(label) + "</button>" +
      '<button type="button" class="btn sm fav-del" data-i="' + i + '" title="删除">×</button></div>';
  }).join("");
  el.querySelectorAll(".fav-run").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var item = loadFavPrompts()[parseInt(btn.getAttribute("data-i"), 10)];
      if (item && item.prompt) streamChat(item.prompt).catch(function (e) {
        addErrorBubble($("chatScroll"), e.message);
      });
    });
  });
  el.querySelectorAll(".fav-del").forEach(function (btn) {
    btn.addEventListener("click", function (e) {
      e.stopPropagation();
      var list = loadFavPrompts();
      list.splice(parseInt(btn.getAttribute("data-i"), 10), 1);
      saveFavPrompts(list);
      renderFavChips();
    });
  });
}

$("saveFavPrompt") && $("saveFavPrompt").addEventListener("click", function () {
  var inp = $("promptInput");
  var prompt = (inp && inp.value.trim()) || lastPrompt || "";
  if (!prompt) {
    alert("请先在输入框写一段话，或先发送一条消息");
    return;
  }
  var label = prompt("快捷任务名称", prompt.slice(0, 16));
  if (label == null) return;
  var list = loadFavPrompts().filter(function (f) { return f.prompt !== prompt; });
  list.unshift({ label: (label || "收藏").slice(0, 40), prompt: prompt });
  saveFavPrompts(list);
  renderFavChips();
});
renderFavChips();

// Bridge link
$("bridgeLink") && $("bridgeLink").addEventListener("click", function (e) {
  e.preventDefault();
  window.open(API_BASE ? "/lumen-science/?embed=1&oasis=1" : "http://127.0.0.1:18990/", "_blank");
});

/* ── 12. Inspector tabs ── */

var ketcherLoaded = false, molLoaded = false;
var PANE_IDS = ["status", "tasks", "files", "artifacts", "skills", "compute", "ketcher", "molecule", "notebooks", "diff", "provenance", "office", "langgraph", "config"];
var activeNotebook = "";
var lastLangGraphResult = "";
document.querySelectorAll(".insp-tab").forEach(function (t) {
  t.addEventListener("click", function () {
    document.querySelectorAll(".insp-tab").forEach(function (b) { b.classList.remove("active"); });
    t.classList.add("active");
    var pane = t.dataset.pane;
    try { localStorage.setItem("lumen-last-pane", pane); } catch (_) {}
    PANE_IDS.forEach(function (id) {
      var el = $(id + "Pane");
      if (!el) return;
      if (pane === id) {
        el.style.display = (id === "ketcher" || id === "molecule") ? "flex" : "block";
        if (id === "ketcher") el.style.flexDirection = "column";
      } else {
        el.style.display = "none";
      }
    });
    if (pane === "skills") loadSkills();
    if (pane === "compute") { loadComputeHosts(); loadComputeJobs(); renderComputeHistory(); }
    if (pane === "status") refreshHealth();
    if (pane === "tasks") renderTasksPane();
    if (pane === "files") { refreshFiles(); loadFileRecent(); loadFileTree(); }
    if (pane === "artifacts") loadArtifacts();
    if (pane === "config") loadLabConfig();
    if (pane === "notebooks") loadNotebooks();
    if (pane === "provenance") loadProvenanceBrowser();
    if (pane === "office") loadOfficePane();
    if (pane === "langgraph") loadLangGraphPane();
    if (pane === "ketcher" && !ketcherLoaded) {
      ketcherLoaded = true;
      bootKetcherFrame();
    }
    if (pane === "molecule") {
      if (!molLoaded) {
        molLoaded = true;
        loadMoleculeViewer();
      }
      loadMoleculeBrowser();
    }
    if (pane === "ketcher") loadMoleculeBrowser();
  });
});

async function loadNotebooks() {
  var listEl = $("nbList");
  if (!listEl || !activeProject) return;
  listEl.innerHTML = '<div class="hint">加载中…</div>';
  try {
    var data = await api("/api/lab/notebooks?project_id=" + activeProject.slug);
    if ($("nbJupyterHint")) {
      $("nbJupyterHint").textContent = data.jupyter_available ? "Jupyter ✓" : "Jupyter 未安装（只读/建本可用）";
    }
    var nbs = data.notebooks || [];
    if (!nbs.length) {
      listEl.innerHTML = '<div class="hint">暂无 notebook — 点「新建」</div>';
    } else {
      listEl.innerHTML = nbs.map(function (n) {
        var name = n.name || n.Name || "";
        return '<button type="button" class="btn sm nb-pick' + (name === activeNotebook ? " primary" : "") +
          '" data-name="' + escHtml(name) + '">' + escHtml(name) +
          (n.cells != null ? " · " + n.cells + " 格" : "") + "</button>";
      }).join(" ");
      listEl.querySelectorAll(".nb-pick").forEach(function (btn) {
        btn.addEventListener("click", function () {
          openNotebook(btn.getAttribute("data-name"));
        });
      });
    }
    if (activeNotebook) openNotebook(activeNotebook);
    else if (nbs.length) openNotebook(nbs[0].name || nbs[0].Name);
  } catch (e) {
    listEl.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

async function openNotebook(name) {
  if (!activeProject || !name) return;
  activeNotebook = name;
  if ($("nbTitle")) $("nbTitle").textContent = name;
  var cellsEl = $("nbCells");
  if (!cellsEl) return;
  cellsEl.innerHTML = '<div class="hint">加载单元…</div>';
  try {
    var data = await api("/api/lab/notebooks/cells/" + encodeURIComponent(name) + "?project_id=" + activeProject.slug);
    var cells = data.cells || [];
    if (!cells.length) {
      cellsEl.innerHTML = '<div class="hint">空笔记本</div>';
      return;
    }
    cellsEl.innerHTML = cells.map(function (c, i) {
      var src = Array.isArray(c.source) ? c.source.join("") : (c.source || "");
      var outs = (c.outputs || []).map(function (o) {
        var t = o.text || o.Text || "";
        if (!t && o.data) {
          try { t = typeof o.data === "string" ? o.data : JSON.stringify(o.data); } catch (_) {}
        }
        if (Array.isArray(t)) t = t.join("");
        return escHtml(String(t || ""));
      }).filter(Boolean).join("\n");
      var st = c.execution_count != null ? " [" + c.execution_count + "]" : "";
      return '<div class="nb-cell">' +
        '<div class="nb-cell-hd">' + escHtml(c.cell_type || "code") + " #" + (i + 1) + st + "</div>" +
        '<pre class="nb-src">' + escHtml(src) + "</pre>" +
        (outs ? '<pre class="nb-out">' + outs + "</pre>" : "") +
        "</div>";
    }).join("");
    // refresh pick highlight
    document.querySelectorAll(".nb-pick").forEach(function (b) {
      b.classList.toggle("primary", b.getAttribute("data-name") === name);
    });
  } catch (e) {
    cellsEl.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

async function createNotebook() {
  if (!activeProject) {
    try { await ensureProject(); } catch (e) { alert(e.message); return; }
  }
  var name = prompt("笔记本文件名", "analysis.ipynb");
  if (!name) return;
  if (!/\.ipynb$/i.test(name)) name += ".ipynb";
  try {
    var res = await api("/api/lab/notebooks?project_id=" + activeProject.slug, {
      method: "POST",
      body: JSON.stringify({ name: name }),
    });
    activeNotebook = res.name || name;
    await loadNotebooks();
  } catch (e) {
    alert("创建失败: " + e.message);
  }
}

async function addNotebookCell() {
  if (!activeProject || !activeNotebook) {
    alert("请先选择笔记本");
    return;
  }
  var src = ($("nbCellSource") && $("nbCellSource").value) || "";
  if (!src.trim()) {
    alert("请输入代码");
    return;
  }
  try {
    await api("/api/lab/notebooks/cell/" + encodeURIComponent(activeNotebook) + "?project_id=" + activeProject.slug, {
      method: "POST",
      body: JSON.stringify({ source: src }),
    });
    if ($("nbCellSource")) $("nbCellSource").value = "";
    await openNotebook(activeNotebook);
  } catch (e) {
    alert("追加失败: " + e.message);
  }
}

async function execNotebook() {
  if (!activeProject || !activeNotebook) {
    alert("请先选择笔记本");
    return;
  }
  var hint = $("nbJupyterHint");
  if (hint) hint.textContent = "执行中…";
  try {
    var res = await api("/api/lab/notebooks/execute/" + encodeURIComponent(activeNotebook) + "?project_id=" + activeProject.slug, {
      method: "POST",
      body: JSON.stringify({}),
    });
    if (res.ok === false) {
      var err = res.error || "unknown";
      if (hint) hint.textContent = "执行失败";
      alert("执行失败: " + err);
      await openNotebook(activeNotebook);
      return;
    }
    if (hint) hint.textContent = "Jupyter ✓ 执行完成";
    showLabToast("Notebook 执行完成", activeNotebook);
    await openNotebook(activeNotebook);
  } catch (e) {
    if (hint) hint.textContent = "执行失败";
    alert("执行失败: " + e.message);
  }
}

async function injectNotebookToChat() {
  if (!activeProject || !activeNotebook) {
    alert("请先选择笔记本");
    return;
  }
  try {
    var data = await api("/api/lab/notebooks/cells/" + encodeURIComponent(activeNotebook) + "?project_id=" + activeProject.slug);
    var md = data.markdown || "";
    var path = "notebooks/" + activeNotebook;
    var prompt = "请基于笔记本 " + path + " 给出分析建议：\n\n" + md.slice(0, 6000);
    var inp = $("promptInput");
    if (inp) {
      inp.value = prompt;
      inp.focus();
    }
  } catch (e) {
    alert(e.message);
  }
}

$("nbCreateBtn") && $("nbCreateBtn").addEventListener("click", createNotebook);
$("nbRefreshBtn") && $("nbRefreshBtn").addEventListener("click", loadNotebooks);
$("nbAddCellBtn") && $("nbAddCellBtn").addEventListener("click", addNotebookCell);
$("nbExecBtn") && $("nbExecBtn").addEventListener("click", execNotebook);
$("nbInjectBtn") && $("nbInjectBtn").addEventListener("click", injectNotebookToChat);

async function loadLabConfig() {
  try {
    var d = await api("/api/lab/config");
    if ($("cfgModel")) $("cfgModel").textContent = d.model_hint || "—";
    if ($("cfgVersion")) $("cfgVersion").textContent = d.version || "—";
    if ($("cfgDefaultMode") && d.default_mode) $("cfgDefaultMode").value = d.default_mode;
    if ($("cfgToolProfile")) $("cfgToolProfile").value = d.tool_profile || "full_science";
    // apply default mode to composer if still default agent
    if (d.default_mode && $("chatMode")) {
      // only set once when loading config tab if user hasn't changed — always sync from config on open
      $("chatMode").value = d.default_mode;
    }
  } catch (e) {
    if ($("cfgHint")) $("cfgHint").textContent = e.message;
  }
}

async function saveLabConfig() {
  try {
    var body = {
      default_mode: ($("cfgDefaultMode") && $("cfgDefaultMode").value) || "agent",
      tool_profile: ($("cfgToolProfile") && $("cfgToolProfile").value) || "full_science",
    };
    await api("/api/lab/config", { method: "PUT", body: JSON.stringify(body) });
    if ($("cfgHint")) $("cfgHint").textContent = "已保存";
    if ($("chatMode")) $("chatMode").value = body.default_mode;
  } catch (e) {
    if ($("cfgHint")) $("cfgHint").textContent = e.message;
  }
}

function renderColoredDiff(text) {
  if (!text) return '<span class="hint">(empty)</span>';
  return text.split("\n").map(function (line) {
    var cls = "diff-line";
    if (line.indexOf("+++") === 0 || line.indexOf("---") === 0) cls += " diff-meta";
    else if (line.charAt(0) === "+") cls += " diff-add";
    else if (line.charAt(0) === "-") cls += " diff-del";
    else if (line.charAt(0) === " ") cls += " diff-ctx";
    return '<div class="' + cls + '">' + escHtml(line) + "</div>";
  }).join("");
}

async function runFileDiff() {
  if (!activeProject) return;
  var a = ($("diffPathA") && $("diffPathA").value.trim()) || "";
  var b = ($("diffPathB") && $("diffPathB").value.trim()) || "";
  if (!a || !b) { alert("需要两个路径"); return; }
  try {
    var d = await api("/api/lab/files/diff?project_id=" + activeProject.slug +
      "&a=" + encodeURIComponent(a) + "&b=" + encodeURIComponent(b));
    var el = $("diffBody");
    if (!el) return;
    el.className = "diff-body colored";
    var text = d.diff || "(empty)";
    if (d.identical) text = "文件内容相同\n\n" + text;
    el.innerHTML = renderColoredDiff(text);
  } catch (e) {
    if ($("diffBody")) $("diffBody").textContent = e.message;
  }
}

var onlyOfficeURL = "";

function docTypeFromExt(ext) {
  switch ((ext || "").toLowerCase()) {
    case "xlsx": return "cell";
    case "pptx": return "slide";
    default:    return "word";
  }
}

async function loadOfficePane() {
  var hint = $("officeHint");
  var host = $("officeHost");
  try {
    var h = await api("/api/lab/health");
    onlyOfficeURL = (h.onlyoffice && h.onlyoffice.url) || "";
    if (hint) {
      if (onlyOfficeURL) {
        hint.textContent = "OnlyOffice Document Server: " + onlyOfficeURL + " — 填写路径后打开。";
      } else {
        hint.textContent = "未配置 LUMEN_ONLYOFFICE_URL。将使用 Lab 文本抽取预览 + 下载。可在 VPS 安装 Document Server 后设置该环境变量。";
      }
    }
  } catch (e) {
    if (hint) hint.textContent = e.message;
  }
  if (host && !host._wired) {
    host._wired = true;
  }
}

async function openOfficePath() {
  if (!activeProject) { alert("请先选择课题"); return; }
  var path = ($("officePath") && $("officePath").value.trim()) || "";
  if (!path) { alert("填写工作区文件路径"); return; }
  var host = $("officeHost");
  if (!host) return;

  var ext = path.split(".").pop() || "docx";
  var fileName = path.split("/").pop() || path;

  // Prefer OnlyOffice if configured
  if (onlyOfficeURL) {
    var dsUrl = onlyOfficeURL.replace(/\/+$/, "");
    var mode = ($("officeMode") && $("officeMode").value) || "view";
    // Docker: replace host for container accessibility (file + callback)
    var useDockerHost = /(localhost|127\.0\.0\.1)/.test(dsUrl);
    var containerOrigin = location.origin;
    if (useDockerHost) {
      containerOrigin = location.origin.replace(/\/\/(localhost|127\.0\.0\.1)(:\d+)?/, "//host.docker.internal$2");
    }
    var fileUrlBase = containerOrigin + labPath("/api/lab/files/download?project_id=" + encodeURIComponent(activeProject.slug) + "&path=" + encodeURIComponent(path));
    var callbackUrl = "";
    var docKey = "";
    try {
      // Server mints callback URL (embeds token when LUMEN_ONLYOFFICE_CALLBACK_TOKEN set)
      var sess = await api(
        "/api/lab/onlyoffice/session?project_id=" + encodeURIComponent(activeProject.slug) +
        "&path=" + encodeURIComponent(path) +
        "&mode=" + encodeURIComponent(mode)
      );
      if (sess && sess.ok) {
        if (sess.ds_url) dsUrl = String(sess.ds_url).replace(/\/+$/, "");
        if (sess.document_key) docKey = sess.document_key;
        if (sess.file_type) ext = sess.file_type;
        if (sess.title) fileName = sess.title;
        if (mode === "edit" && sess.callback_url) {
          callbackUrl = sess.callback_url;
          // Rewrite callback host for Docker Desktop if DS is local
          if (useDockerHost) {
            callbackUrl = callbackUrl.replace(
              /\/\/(localhost|127\.0\.0\.1)(:\d+)?/,
              "//host.docker.internal$2"
            );
          }
        }
        if (sess.download_path) {
          fileUrlBase = containerOrigin + labPath(sess.download_path);
        }
      }
    } catch (e) {
      // Fall back to client-built URLs (without token) if session endpoint fails
      if (mode === "edit") {
        callbackUrl = containerOrigin + labPath(
          "/api/lab/onlyoffice/callback?project_id=" + encodeURIComponent(activeProject.slug) +
          "&path=" + encodeURIComponent(path)
        );
      }
    }
    var editorPage = labPath("/office-editor.html") +
      "?ds=" + encodeURIComponent(dsUrl) +
      "&url=" + encodeURIComponent(fileUrlBase) +
      "&title=" + encodeURIComponent(fileName) +
      "&ext=" + encodeURIComponent(ext) +
      "&mode=" + encodeURIComponent(mode) +
      "&cb=" + encodeURIComponent(callbackUrl) +
      (docKey ? "&key=" + encodeURIComponent(docKey) : "");
    host.innerHTML = '<iframe style="width:100%;height:520px;border:0;border-radius:8px" src="' + editorPage + '" allow="fullscreen"></iframe>';
    return;
  }

  // Fallback: text extraction + download
  try {
    var data = await api("/api/lab/files/content?project_id=" + activeProject.slug + "&path=" + encodeURIComponent(path));
    var kind = data.previewKind || "";
    if (kind === "office" || kind === "text" || kind === "markdown") {
      host.innerHTML = '<div class="hint">文本抽取预览（非 WYSIWYG）· ' + escHtml(kind) + "</div>" +
        '<pre class="fp-body" style="max-height:420px;overflow:auto">' + escHtml(data.content || "") + "</pre>";
    } else {
      var dl = labPath("/api/lab/files/download?project_id=" + activeProject.slug + "&path=" + encodeURIComponent(path));
      host.innerHTML = '<div class="hint">无法内联预览 — <a href="' + dl + '" target="_blank">下载</a></div>';
    }
  } catch (e) {
    host.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

$("officeOpenBtn") && $("officeOpenBtn").addEventListener("click", openOfficePath);
$("officeDownloadBtn") && $("officeDownloadBtn").addEventListener("click", function () {
  if (!activeProject) return;
  var path = ($("officePath") && $("officePath").value.trim()) || "";
  if (!path) return;
  window.open(labPath("/api/lab/files/download?project_id=" + activeProject.slug + "&path=" + encodeURIComponent(path)), "_blank");
});

/* ── LangGraph inspector pane ── */

function langGraphHistoryKey() {
  return "lumen-langgraph-history";
}

function readLangGraphHistory() {
  try {
    var list = JSON.parse(localStorage.getItem(langGraphHistoryKey()) || "[]");
    return Array.isArray(list) ? list : [];
  } catch (_) {
    return [];
  }
}

function writeLangGraphHistory(list) {
  try {
    localStorage.setItem(langGraphHistoryKey(), JSON.stringify(list || []));
  } catch (_) {}
}

function pushLangGraphHistory(entry) {
  var next = reduceLangGraphHistory(readLangGraphHistory(), entry, LG_HISTORY_MAX);
  writeLangGraphHistory(next);
  renderLangGraphHistory();
  return next;
}

function currentLangGraphProjectId() {
  return (activeProject && activeProject.slug) || "";
}

function renderLangGraphHistory() {
  var el = $("langgraphHistory");
  if (!el) return;
  // sync scope toggle UI
  var scopeBtn = $("langgraphHistScopeBtn");
  if (scopeBtn) {
    scopeBtn.textContent = langGraphHistoryScope === "all" ? "全部课题" : "本课题";
    scopeBtn.title = langGraphHistoryScope === "all"
      ? "当前显示全部课题；点击改为仅本课题"
      : "当前仅本课题；点击改为全部课题";
  }
  var all = readLangGraphHistory();
  var list = filterLangGraphHistory(all, currentLangGraphProjectId(), langGraphHistoryScope);
  if (!all.length) {
    el.className = "hint";
    el.textContent = "暂无历史（本机浏览器保存，最多 " + LG_HISTORY_MAX + " 条）";
    return;
  }
  if (!list.length) {
    el.className = "hint";
    el.textContent = langGraphHistoryScope === "all"
      ? "暂无历史"
      : "本课题暂无记录（可点「全部课题」查看其它 slug）";
    return;
  }
  el.className = "";
  el.innerHTML = list.map(function (item) {
    var when = "";
    try {
      when = new Date(item.ts || 0).toLocaleString();
    } catch (_) {
      when = String(item.ts || "");
    }
    var prompt = String(item.prompt || "").replace(/\s+/g, " ").slice(0, 48);
    var proj = item.project_id ? escHtml(item.project_id) : "—";
    var badge = item.ok ? "ok" : "err";
    var id = escHtml(item.id || "");
    return '<button type="button" class="btn sm lg-hist-item" data-id="' + id + '" style="display:block;width:100%;text-align:left;margin:0 0 4px">' +
      '<span class="' + badge + '">' + (item.ok ? "✓" : "×") + "</span> " +
      escHtml(prompt || "(空提示)") +
      ' <span class="hint">· ' + proj + " · " + escHtml(when) + "</span></button>";
  }).join("");
  el.querySelectorAll(".lg-hist-item").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var id = btn.getAttribute("data-id") || "";
      var item = null;
      readLangGraphHistory().some(function (h) {
        if (String(h.id) === id) { item = h; return true; }
        return false;
      });
      if (!item) return;
      if ($("langgraphPrompt")) $("langgraphPrompt").value = item.prompt || "";
      lastLangGraphResult = item.result || "";
      setLangGraphResultView(item.result || "(无结果)", !!item.ok);
      showLabToast("已载入历史", (item.prompt || "").slice(0, 40));
    });
  });
}

async function loadServerLangGraphHistory() {
  if (!activeProject) return;
  try {
    var data = await api(
      "/api/lab/langgraph/history?project_id=" + encodeURIComponent(activeProject.slug) + "&limit=40"
    );
    if (!data || !data.ok || !Array.isArray(data.entries)) return;
    // Merge server entries into localStorage by id (server is source of truth for this project).
    var local = readLangGraphHistory();
    var byId = {};
    local.forEach(function (e) {
      if (e && e.id) byId[String(e.id)] = e;
    });
    data.entries.forEach(function (e) {
      if (!e) return;
      var id = String(e.id || ("srv_" + (e.ts || "") + "_" + Math.random().toString(36).slice(2, 6)));
      byId[id] = {
        id: id,
        ts: e.ts || Date.now(),
        project_id: e.project_id || activeProject.slug,
        prompt: e.prompt || "",
        ok: !!e.ok,
        result: e.result || e.error || "",
        mode: e.mode || "",
      };
    });
    var merged = Object.keys(byId).map(function (k) { return byId[k]; });
    merged.sort(function (a, b) { return (b.ts || 0) - (a.ts || 0); });
    writeLangGraphHistory(merged.slice(0, LG_HISTORY_MAX));
  } catch (_) {}
}

async function loadLangGraphPane() {
  var status = $("langgraphStatus");
  var hint = $("langgraphHint");
  var runBtn = $("langgraphRunBtn");
  await loadServerLangGraphHistory();
  renderLangGraphHistory();
  try {
    var h = await api("/api/lab/health");
    var lg = (h && h.langgraph) || {};
    if (status) {
      var st = lg.available ? (lg.llm ? "可用 · LLM" : "可用 · 启发式") : "不可用";
      status.textContent = st;
      status.className = "v" + (lg.available ? " ok" : "");
    }
    if (hint) {
      hint.textContent = lg.available
        ? (lg.hint || "LangGraph 旁路可用。历史：本机浏览器 + 服务端课题持久化。")
        : (lg.hint || "未启用：设置 LUMEN_LANGGRAPH=1 并安装 venv（见 docs/lab/LANGGRAPH.md）");
    }
    if (runBtn) runBtn.disabled = !lg.available;
  } catch (e) {
    if (status) status.textContent = "检查失败";
    if (hint) hint.textContent = e.message || String(e);
    if (runBtn) runBtn.disabled = true;
  }
}

function setLangGraphResultView(text, asMarkdown) {
  var out = $("langgraphResult");
  if (!out) return;
  var t = String(text == null ? "" : text);
  if (asMarkdown && t && t !== "尚未运行" && t !== "运行中…" && t !== "请填写提示词") {
    // Markdown first (escapes raw HTML), then path linkify, then click bindings.
    out.innerHTML = linkifyLangGraphPaths(renderMarkdown(t));
    bindCodeCopy(out);
    bindWorkspaceRefs(out);
  } else {
    out.textContent = t;
  }
}

async function runLangGraphPane() {
  var promptEl = $("langgraphPrompt");
  var runBtn = $("langgraphRunBtn");
  var prompt = promptEl ? promptEl.value : "";
  var body = buildLangGraphBody(activeProject && activeProject.slug, prompt);
  if (!body.prompt) {
    lastLangGraphResult = "";
    setLangGraphResultView("请填写提示词", false);
    return;
  }
  if (runBtn) {
    runBtn.disabled = true;
    runBtn.textContent = "运行中…";
  }
  setLangGraphResultView("运行中…", false);
  try {
    var resp = await api("/api/lab/langgraph/run", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    var text = formatLangGraphResult(resp);
    lastLangGraphResult = text;
    setLangGraphResultView(text, !!(resp && resp.ok));
    pushLangGraphHistory({
      id: "lg_" + Date.now(),
      ts: Date.now(),
      project_id: body.project_id || "",
      prompt: body.prompt,
      ok: !!(resp && resp.ok),
      result: text,
    });
    if (resp && resp.ok) {
      showLabToast("LangGraph 完成", text.slice(0, 80));
    } else {
      showLabToast("LangGraph 失败", text.slice(0, 80));
    }
  } catch (e) {
    var msg = e && e.message ? e.message : String(e);
    lastLangGraphResult = msg;
    setLangGraphResultView(msg, false);
    pushLangGraphHistory({
      id: "lg_" + Date.now(),
      ts: Date.now(),
      project_id: body.project_id || "",
      prompt: body.prompt,
      ok: false,
      result: msg,
    });
    showLabToast("LangGraph 错误", msg.slice(0, 80));
  } finally {
    if (runBtn) {
      runBtn.textContent = "运行";
      // re-check availability for button state
      loadLangGraphPane();
    }
  }
}

function langGraphResultToComposer() {
  var text = String(lastLangGraphResult || "").trim();
  if (!text || text === "尚未运行" || text === "运行中…") {
    showLabToast("无结果", "先运行 LangGraph");
    return;
  }
  var ta = $("promptInput") || $("composer") || $("chatInput") ||
    document.querySelector("textarea.composer-input, #promptInput, #chatInput, .composer textarea");
  if (!ta) {
    try {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        navigator.clipboard.writeText(text);
        showLabToast("已复制", "未找到输入框，结果已复制到剪贴板");
        return;
      }
    } catch (_) {}
    alert(text);
    return;
  }
  var prefix = ta.value && ta.value.trim() ? ta.value.replace(/\s*$/, "") + "\n\n" : "";
  ta.value = prefix + "[LangGraph]\n" + text;
  ta.focus();
  try { ta.dispatchEvent(new Event("input", { bubbles: true })); } catch (_) {}
  showLabToast("已写入对话", "可编辑后发送");
}

function copyLangGraphResult() {
  var text = String(lastLangGraphResult || "").trim();
  if (!text) {
    showLabToast("无结果", "先运行 LangGraph");
    return;
  }
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(text).then(function () {
      showLabToast("已复制", "LangGraph 结果");
    }).catch(function () {
      showLabToast("复制失败", "请手动选择文本");
    });
    return;
  }
  showLabToast("复制失败", "剪贴板不可用");
}

async function saveLangGraphResultNote() {
  if (!activeProject) {
    showLabToast("请先选择课题", "");
    return;
  }
  var text = String(lastLangGraphResult || "").trim();
  if (!text || text === "尚未运行" || text === "运行中…") {
    showLabToast("无结果", "先运行 LangGraph");
    return;
  }
  var prompt = ($("langgraphPrompt") && $("langgraphPrompt").value) || "";
  var savedAt = new Date().toISOString();
  var stamp = savedAt.replace(/[:.]/g, "-").slice(0, 19);
  var path = "artifacts/langgraph-run-" + stamp + ".md";
  var md = buildLangGraphRunNote(prompt, text, {
    project_id: currentLangGraphProjectId(),
    saved_at: savedAt,
  });
  try {
    await api("/api/lab/files/write?project_id=" + encodeURIComponent(activeProject.slug), {
      method: "POST",
      body: JSON.stringify({ path: path, content: md }),
    });
    var attached = attachPathsToComposer([path], "LangGraph 运行笔记");
    showLabToast("已存笔记", path + (attached ? " · 已引用到对话" : ""));
    try {
      var filesTab = document.querySelector('.insp-tab[data-pane="files"]');
      if (filesTab) filesTab.click();
      previewFile(path, "markdown");
    } catch (_) {}
  } catch (e) {
    showLabToast("保存失败", (e && e.message) || String(e));
  }
}

$("langgraphRunBtn") && $("langgraphRunBtn").addEventListener("click", runLangGraphPane);
$("langgraphClearBtn") && $("langgraphClearBtn").addEventListener("click", function () {
  lastLangGraphResult = "";
  if ($("langgraphPrompt")) $("langgraphPrompt").value = "";
  setLangGraphResultView("尚未运行", false);
});
$("langgraphToChatBtn") && $("langgraphToChatBtn").addEventListener("click", langGraphResultToComposer);
$("langgraphCopyBtn") && $("langgraphCopyBtn").addEventListener("click", copyLangGraphResult);
$("langgraphSaveNoteBtn") && $("langgraphSaveNoteBtn").addEventListener("click", saveLangGraphResultNote);
$("langgraphHistClearBtn") && $("langgraphHistClearBtn").addEventListener("click", function () {
  var all = readLangGraphHistory();
  if (langGraphHistoryScope === "all") {
    if (!confirm("清空全部课题的 LangGraph 历史？")) return;
    writeLangGraphHistory([]);
    showLabToast("历史已清空", "全部课题");
  } else {
    var pid = currentLangGraphProjectId();
    var kept = all.filter(function (h) {
      return String(h.project_id || "").trim() !== pid;
    });
    if (kept.length === all.length) {
      showLabToast("无需清空", "本课题没有历史");
      renderLangGraphHistory();
      return;
    }
    if (!confirm("仅清空当前课题（" + (pid || "未选") + "）的历史？")) return;
    writeLangGraphHistory(kept);
    showLabToast("历史已清空", "仅本课题");
  }
  renderLangGraphHistory();
});
$("langgraphHistScopeBtn") && $("langgraphHistScopeBtn").addEventListener("click", function () {
  langGraphHistoryScope = langGraphHistoryScope === "all" ? "project" : "all";
  try { localStorage.setItem("lumen-langgraph-hist-scope", langGraphHistoryScope); } catch (_) {}
  renderLangGraphHistory();
});
try {
  var sc = localStorage.getItem("lumen-langgraph-hist-scope");
  if (sc === "all" || sc === "project") langGraphHistoryScope = sc;
} catch (_) {}

async function exportLangGraphHistory() {
  if (!activeProject) {
    showLabToast("请先选择课题", "");
    return;
  }
  var all = readLangGraphHistory();
  var list = filterLangGraphHistory(all, currentLangGraphProjectId(), langGraphHistoryScope);
  if (!list.length) {
    showLabToast("无可导出", "当前范围没有历史记录");
    return;
  }
  var exportedAt = new Date().toISOString();
  var meta = {
    project_id: currentLangGraphProjectId(),
    scope: langGraphHistoryScope,
    exported_at: exportedAt,
  };
  var doc = buildLangGraphHistoryExport(list, meta);
  var stamp = exportedAt.replace(/[:.]/g, "-").slice(0, 19);
  var jsonPath = "artifacts/langgraph-history-" + stamp + ".json";
  var mdPath = "artifacts/langgraph-history-" + stamp + ".md";
  var md = buildLangGraphHistoryMarkdown(list, {
    project_id: meta.project_id,
    scope: meta.scope,
    exported_at: exportedAt,
    json_path: jsonPath,
  });
  try {
    await api("/api/lab/files/write?project_id=" + encodeURIComponent(activeProject.slug), {
      method: "POST",
      body: JSON.stringify({ path: jsonPath, content: JSON.stringify(doc, null, 2) + "\n" }),
    });
    await api("/api/lab/files/write?project_id=" + encodeURIComponent(activeProject.slug), {
      method: "POST",
      body: JSON.stringify({ path: mdPath, content: md }),
    });
    // Soft-attach export paths into the chat composer for the main agent.
    var attached = attachPathsToComposer([mdPath, jsonPath], "LangGraph 历史导出");
    showLabToast(
      "已导出",
      list.length + " 条 · JSON + MD" + (attached ? " · 已引用到对话" : "")
    );
    try {
      var filesTab = document.querySelector('.insp-tab[data-pane="files"]');
      if (filesTab) filesTab.click();
      // Prefer readable Markdown preview after export
      previewFile(mdPath, "markdown");
    } catch (_) {}
  } catch (e) {
    showLabToast("导出失败", (e && e.message) || String(e));
  }
}

/**
 * Append @path refs (and a short label) to the main composer.
 * Returns true if the textarea was updated.
 */
function attachPathsToComposer(paths, label) {
  var ta = $("promptInput") || document.querySelector(".composer textarea");
  if (!ta) return false;
  paths = (paths || []).map(function (p) { return String(p || "").trim(); }).filter(Boolean);
  if (!paths.length) return false;
  var block = paths.map(function (p) { return "@" + p; }).join(" ");
  if (label) block = "[" + label + "] " + block;
  var cur = ta.value || "";
  ta.value = cur && cur.trim() ? cur.replace(/\s*$/, "") + "\n\n" + block : block;
  try { ta.dispatchEvent(new Event("input", { bubbles: true })); } catch (_) {}
  return true;
}

async function importLangGraphHistory() {
  if (!activeProject) {
    showLabToast("请先选择课题", "");
    return;
  }
  var path = prompt(
    "从工作区导入 LangGraph 历史 JSON\n路径（相对课题 workspace）",
    "artifacts/langgraph-history.json"
  );
  if (!path) return;
  path = String(path).replace(/^\/+/, "").replace(/\.\./g, "");
  try {
    var data = await api(
      "/api/lab/files/content?project_id=" + encodeURIComponent(activeProject.slug) +
      "&path=" + encodeURIComponent(path)
    );
    var raw = data.content || data.raw || "";
    var doc;
    try {
      doc = JSON.parse(raw);
    } catch (pe) {
      showLabToast("导入失败", "JSON 解析错误");
      return;
    }
    var merged = mergeLangGraphHistoryImport(readLangGraphHistory(), doc, LG_HISTORY_MAX);
    if (merged.error) {
      showLabToast("导入失败", merged.error);
      return;
    }
    writeLangGraphHistory(merged.list);
    renderLangGraphHistory();
    showLabToast("导入完成", "新增 " + (merged.added || 0) + " 条（总计 " + merged.list.length + "）");
  } catch (e) {
    showLabToast("导入失败", (e && e.message) || String(e));
  }
}

$("langgraphHistExportBtn") && $("langgraphHistExportBtn").addEventListener("click", exportLangGraphHistory);
$("langgraphHistImportBtn") && $("langgraphHistImportBtn").addEventListener("click", importLangGraphHistory);

async function loadProvenanceBrowser() {
  var el = $("provBody");
  if (!el || !activeProject) return;
  el.innerHTML = '<div class="hint">加载溯源…</div>';
  var q = ($("provFilter") && $("provFilter").value.trim()) || "";
  try {
    var url = "/api/lab/provenance?project_id=" + activeProject.slug + "&limit=100";
    if (q) url += "&path=" + encodeURIComponent(q);
    var data = await api(url);
    var recs = data.records || [];
    if (!recs.length) {
      el.innerHTML = '<div class="hint">暂无溯源记录' + (q ? "（路径过滤）" : "") + "</div>";
      return;
    }
    el.innerHTML = recs.map(function (r) {
      var path = r.path || r.artifact || "";
      var mcp = (r.mcp_calls || []).map(function (m) {
        return (m.tool || "") + (m.query ? '("' + m.query + '")' : "");
      }).join(", ");
      return '<div class="prov-card" data-path="' + escHtml(path) + '">' +
        '<div class="prov-hd"><span class="mono">' + escHtml(path || "(no path)") + "</span>" +
        ' <span class="hint">' + escHtml((r.ts || "").slice(0, 19).replace("T", " ")) + "</span></div>" +
        '<div class="hint">' + escHtml(r.kind || "artifact") + " · " + escHtml(r.model || "—") +
        (r.content_hash ? " · #" + escHtml(String(r.content_hash).slice(0, 12)) : "") + "</div>" +
        (mcp ? '<div class="hint">🔗 ' + escHtml(mcp) + "</div>" : "") +
        "</div>";
    }).join("");
    el.querySelectorAll(".prov-card").forEach(function (card) {
      card.addEventListener("click", function () {
        var path = card.getAttribute("data-path");
        if (!path) return;
        var tab = document.querySelector('.insp-tab[data-pane="files"]');
        if (tab) tab.click();
        previewFile(path, "");
      });
    });
  } catch (e) {
    el.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

function setMolStatus(msg, ok) {
  var el = $("molStatus");
  if (!el) return;
  el.textContent = msg || "";
  el.style.color = ok === false ? "var(--ocs-danger)" : (ok ? "var(--ocs-success)" : "var(--ocs-muted)");
}

var ketcherSameOrigin = false;
var ketcherBootPromise = null;

async function detectKetcherOrigin() {
  try {
    var h = await api("/api/lab/health");
    ketcherSameOrigin = !!(h.ketcher && h.ketcher.same_origin);
    return ketcherSameOrigin;
  } catch (_) {
    ketcherSameOrigin = false;
    return false;
  }
}

function bootKetcherFrame() {
  var frame = $("ketcherFrame");
  if (!frame) return;
  detectKetcherOrigin().then(function (same) {
    if (same) {
      frame.src = labPath("/ketcher/index.html");
      setMolStatus("同域 Ketcher 已加载 — 可用「从 Ketcher 导入 / 推送到 Ketcher」", true);
    } else {
      // fallback CDN (may be blocked) — MOL path remains reliable
      frame.src = "https://lifescience.opensource.epam.com/ketcher/standalone/index.html";
      setMolStatus("未部署同域 Ketcher，使用外链（可能失败）；请用 MOL 粘贴路径", false);
    }
  });
}

function getKetcherAPI() {
  var frame = $("ketcherFrame");
  if (!frame || !frame.contentWindow) return null;
  try {
    return frame.contentWindow.ketcher || null;
  } catch (e) {
    // cross-origin
    return null;
  }
}

async function importFromKetcher() {
  var k = getKetcherAPI();
  if (!k || typeof k.getMolfile !== "function") {
    setMolStatus("无法访问 Ketcher API（需要同域托管）。请手动 File→Save as MOL 粘贴。", false);
    alert("需要同域 Ketcher。请部署 third_party/ketcher-standalone 到服务器，或手动粘贴 MOL。");
    return;
  }
  try {
    var mol = await k.getMolfile();
    if ($("molEditor")) $("molEditor").value = mol || "";
    var v = validateMolContent(mol || "");
    setMolStatus(v.ok ? "已从 Ketcher 导入 (" + v.reason + ")" : "导入内容可能无效", v.ok);
  } catch (e) {
    setMolStatus("从 Ketcher 导入失败: " + e.message, false);
    alert(e.message);
  }
}

async function pushToKetcher() {
  var content = ($("molEditor") && $("molEditor").value) || "";
  var v = validateMolContent(content);
  if (!v.ok) {
    setMolStatus("推送失败：内容无效 (" + v.reason + ")", false);
    return;
  }
  var k = getKetcherAPI();
  if (!k || typeof k.setMolecule !== "function") {
    setMolStatus("无法访问 Ketcher API（需要同域托管）", false);
    alert("需要同域 Ketcher 才能推送结构。");
    return;
  }
  try {
    await k.setMolecule(content);
    setMolStatus("已推送到 Ketcher", true);
  } catch (e) {
    setMolStatus("推送失败: " + e.message, false);
    alert(e.message);
  }
}

async function saveMolToWorkspace() {
  if (!activeProject) { alert("请先选择课题"); return; }
  var rawPath = ($("molSavePath") && $("molSavePath").value.trim()) || "";
  var path = normalizeMolPath(rawPath);
  if ($("molSavePath")) $("molSavePath").value = path;
  var content = ($("molEditor") && $("molEditor").value) || "";
  var v = validateMolContent(content);
  if (!v.ok) {
    var msg = v.reason === "empty"
      ? "编辑器为空 — 请从 Ketcher 导出 MOL/SDF 粘贴到下方，或点「加载路径」"
      : "内容不像分子结构（需要 MOL/SDF/PDB 文本）";
    setMolStatus(msg, false);
    alert(msg);
    return;
  }
  setMolStatus("保存中…", null);
  try {
    var res = await api("/api/lab/files/write?project_id=" + activeProject.slug, {
      method: "POST",
      body: JSON.stringify({ path: path, content: content }),
    });
    var saved = res.path || path;
    setMolStatus("已保存 " + saved + " (" + v.reason + ")", true);
    if ($("composerHint")) $("composerHint").textContent = "分子已保存 " + saved;
    showLabToast("分子已保存", saved);
    refreshFiles();
    loadFileTree();
    loadArtifacts();
    loadMoleculeBrowser();
  } catch (e) {
    setMolStatus("保存失败: " + e.message, false);
    alert(e.message);
  }
}

async function loadMolFromWorkspace() {
  if (!activeProject) return;
  var path = normalizeMolPath(($("molSavePath") && $("molSavePath").value.trim()) || "");
  if ($("molSavePath")) $("molSavePath").value = path;
  setMolStatus("加载中…", null);
  try {
    var data = await api("/api/lab/files/content?project_id=" + activeProject.slug + "&path=" + encodeURIComponent(path));
    var content = data.content || "";
    var v = validateMolContent(content);
    if ($("molEditor")) $("molEditor").value = content;
    if (!v.ok) {
      setMolStatus("已加载但内容可能不是结构文件: " + path, false);
    } else {
      setMolStatus("已加载 " + path + " (" + v.reason + ")", true);
    }
  } catch (e) {
    setMolStatus("加载失败: " + e.message, false);
    alert(e.message);
  }
}

async function importSkillFile(file) {
  if (!activeProject || !file) return;
  try {
    var fd = new FormData();
    fd.append("file", file);
    var res = await fetch(labPath("/api/lab/skills/import?project_id=" + activeProject.slug), {
      method: "POST",
      body: fd,
    });
    if (!res.ok) throw new Error(await res.text());
    var body = await res.json();
    if ($("skillsHint")) $("skillsHint").textContent = "导入 " + (body.count || 0) + " 个技能";
    loadSkills();
  } catch (e) {
    alert("技能导入失败: " + e.message);
  }
}

async function loadFileRecent() {
  var el = $("fileRecent");
  if (!el || !activeProject) return;
  renderStarredFiles();
  try {
    var data = await api("/api/lab/files/recent?project_id=" + activeProject.slug + "&limit=12");
    var files = data.files || [];
    if (!files.length) {
      el.innerHTML = '<div class="hint">暂无文件</div>';
      return;
    }
    el.innerHTML = files.map(function (f) {
      return '<div class="ft-row recent" data-path="' + escHtml(f.path) + '" data-preview="' + escHtml(f.previewKind || "") + '">' +
        '<span class="ft-name">' + escHtml(f.path) + "</span>" +
        '<span class="ft-size">' + escHtml((f.mtime || "").slice(0, 16).replace("T", " ")) + "</span></div>";
    }).join("");
    el.querySelectorAll(".ft-row").forEach(function (row) {
      row.addEventListener("click", function () {
        previewFile(row.dataset.path, row.dataset.preview);
      });
    });
  } catch (e) {
    el.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

async function loadArtifacts() {
  var el = $("artifactsBody");
  if (!el || !activeProject) return;
  try {
    var data = await api("/api/lab/artifacts?project_id=" + activeProject.slug);
    var arts = data.artifacts || [];
    if (!arts.length) {
      el.innerHTML = '<div class="hint">暂无产物 — brief / 工具 / 入库后会出现在此</div>';
      return;
    }
    el.innerHTML = arts.map(function (a) {
      return '<div class="art-row" data-path="' + escHtml(a.path) + '" data-preview="' + escHtml(a.previewKind || "") + '">' +
        '<span class="art-bucket">' + escHtml(a.bucket || "") + "</span>" +
        '<span class="ft-name">' + escHtml(a.path) + "</span>" +
        '<span class="ft-size">' + fmtSize(a.size) + " · " + escHtml((a.mtime || "").slice(0, 16).replace("T", " ")) + "</span></div>";
    }).join("");
    el.querySelectorAll(".art-row").forEach(function (row) {
      row.addEventListener("click", function () {
        var filesTab = document.querySelector('.insp-tab[data-pane="files"]');
        if (filesTab) filesTab.click();
        previewFile(row.dataset.path, row.dataset.preview);
      });
    });
  } catch (e) {
    el.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

/* ── File search ── */
async function runFileSearch() {
  var qEl = $("fileSearch");
  var hitsEl = $("fileSearchHits");
  if (!qEl || !hitsEl || !activeProject) return;
  var q = qEl.value.trim();
  if (!q) {
    hitsEl.hidden = true;
    hitsEl.innerHTML = "";
    return;
  }
  try {
    var data = await api("/api/lab/files/search?project_id=" + activeProject.slug + "&q=" + encodeURIComponent(q));
    var hits = data.hits || [];
    hitsEl.hidden = false;
    if (!hits.length) {
      hitsEl.innerHTML = '<div class="hint">无匹配</div>';
      return;
    }
    hitsEl.innerHTML = hits.map(function (h) {
      return '<div class="ft-row hit" data-path="' + escHtml(h.path) + '" data-isdir="' + (h.isDir ? "1" : "0") + '" data-preview="' + escHtml(h.previewKind || "") + '">' +
        '<span class="ft-name">' + escHtml(h.path) + '</span>' +
        '<span class="ft-size">' + escHtml(h.match) + "</span>" +
        (h.snippet ? '<div class="hit-snip">' + escHtml(h.snippet) + "</div>" : "") +
        "</div>";
    }).join("");
    hitsEl.querySelectorAll(".ft-row").forEach(function (row) {
      row.addEventListener("click", function () {
        if (row.dataset.isdir === "1") {
          fileCwd = row.dataset.path;
          refreshFiles();
        } else {
          previewFile(row.dataset.path, row.dataset.preview);
        }
      });
    });
  } catch (e) {
    hitsEl.hidden = false;
    hitsEl.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

/* ── Compute ── */
async function loadComputeHosts() {
  var sel = $("computeHost");
  if (!sel) return;
  try {
    var data = await api("/api/lab/compute/ssh-hosts");
    var hosts = data.hosts || [];
    var hasLocal = hosts.some(function (h) {
      var a = typeof h === "string" ? h : (h.alias || h.Alias || "");
      return a === "local" || a === "localhost";
    });
    if (!hasLocal) hosts = [{ alias: "local", source: "builtin" }].concat(hosts);
    sel.innerHTML = hosts.map(function (h) {
      var name = typeof h === "string" ? h : (h.alias || h.Alias || h.Host || h.host || h.name || "local");
      var src = (h && h.source) || "";
      var label = name === "local" ? "local（本机 shell）" : name + (src ? " [" + src + "]" : "");
      return '<option value="' + escHtml(name) + '">' + escHtml(label) + "</option>";
    }).join("");
    renderHostRegistry(hosts);
  } catch (e) {
    sel.innerHTML = '<option value="local">local（本机 shell）</option>';
  }
}

function renderHostRegistry(hosts) {
  var el = $("hostRegistry");
  if (!el) return;
  // show local + registry + first few ssh_config for ping
  var list = (hosts || []).filter(function (h) {
    if (!h) return false;
    if (h.source === "registry" || h.alias === "local") return true;
    return h.source === "ssh_config";
  }).slice(0, 12);
  if (!list.length) {
    el.innerHTML = '<div class="hint">尚无主机 — 下方可添加</div>';
    return;
  }
  el.innerHTML = list.map(function (h) {
    var alias = h.alias || h.Alias || "";
    var canDel = h.source === "registry";
    return '<div class="host-row">' +
      '<span class="mono">' + escHtml(alias) + "</span> " +
      '<span class="hint">' + escHtml(h.user || "") + (h.hostname ? "@" + escHtml(h.hostname) : "") +
      (h.source ? " · " + escHtml(h.source) : "") + "</span>" +
      (h.notes ? ' <span class="hint">(' + escHtml(h.notes) + ")</span>" : "") +
      ' <button type="button" class="btn sm host-ping" data-alias="' + escHtml(alias) + '">探测</button>' +
      (canDel ? ' <button type="button" class="btn sm host-del" data-alias="' + escHtml(alias) + '">删除</button>' : "") +
      ' <span class="hint host-ping-res" data-alias="' + escHtml(alias) + '"></span></div>';
  }).join("");
  el.querySelectorAll(".host-del").forEach(function (btn) {
    btn.addEventListener("click", async function () {
      try {
        await api("/api/lab/compute/ssh-hosts?alias=" + encodeURIComponent(btn.getAttribute("data-alias")), { method: "DELETE" });
        loadComputeHosts();
      } catch (e) { alert(e.message); }
    });
  });
  el.querySelectorAll(".host-ping").forEach(function (btn) {
    btn.addEventListener("click", async function () {
      var alias = btn.getAttribute("data-alias");
      var resEl = el.querySelector('.host-ping-res[data-alias="' + alias + '"]');
      if (resEl) resEl.textContent = "探测中…";
      try {
        var r = await api("/api/lab/compute/ssh-hosts/ping", {
          method: "POST",
          body: JSON.stringify({ alias: alias }),
        });
        if (resEl) {
          resEl.textContent = (r.ok ? "✓ " : "✗ ") + (r.message || "") + " (" + r.latency_ms + "ms)";
          resEl.style.color = r.ok ? "var(--ocs-success)" : "var(--ocs-danger)";
        }
      } catch (e) {
        if (resEl) resEl.textContent = "✗ " + e.message;
      }
    });
  });
}

async function loadMoleculeBrowser() {
  var els = [$("molBrowser"), $("molBrowser2")].filter(Boolean);
  if (!els.length || !activeProject) return;
  try {
    var data = await api("/api/lab/files/recent?project_id=" + activeProject.slug + "&limit=50");
    var mols = (data.files || []).filter(function (f) {
      return f.previewKind === "molecule" || /\.(pdb|sdf|mol|cif)$/i.test(f.path || "");
    });
    var html = !mols.length
      ? '<div class="hint">工作区暂无 .pdb/.sdf/.mol 文件</div>'
      : mols.map(function (f) {
          return '<button type="button" class="btn sm mol-pick" data-path="' + escHtml(f.path) + '">' + escHtml(f.path) + "</button>";
        }).join(" ");
    els.forEach(function (el) {
      el.innerHTML = html;
      el.querySelectorAll(".mol-pick").forEach(function (btn) {
        btn.addEventListener("click", async function () {
          var path = btn.getAttribute("data-path");
          try {
            var content = await api("/api/lab/files/content?project_id=" + activeProject.slug + "&path=" + encodeURIComponent(path));
            if ($("molEditor")) $("molEditor").value = content.content || "";
            if ($("molSavePath")) $("molSavePath").value = path;
            openMoleculeFromContent(path, content.content || "", "molecule");
          } catch (e) { alert(e.message); }
        });
      });
    });
  } catch (e) {
    els.forEach(function (el) { el.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>"; });
  }
}

async function registerHost() {
  var alias = ($("hostAlias") && $("hostAlias").value.trim()) || "";
  var hostname = ($("hostHostname") && $("hostHostname").value.trim()) || "";
  var user = ($("hostUser") && $("hostUser").value.trim()) || "";
  var notes = ($("hostNotes") && $("hostNotes").value.trim()) || "";
  if (!alias) { alert("需要别名 alias"); return; }
  try {
    await api("/api/lab/compute/ssh-hosts", {
      method: "POST",
      body: JSON.stringify({ alias: alias, hostname: hostname, user: user, notes: notes }),
    });
    if ($("hostAlias")) $("hostAlias").value = "";
    loadComputeHosts();
  } catch (e) {
    alert(e.message);
  }
}

async function loadFileTree() {
  var el = $("fileTreeView");
  if (!el || !activeProject) return;
  try {
    var data = await api("/api/lab/files/tree?project_id=" + activeProject.slug + "&path=.&depth=4");
    el.innerHTML = renderTreeNodes(data.tree ? (data.tree.children || [data.tree]) : [], 0);
    el.querySelectorAll("[data-path]").forEach(function (n) {
      n.addEventListener("click", function (e) {
        e.stopPropagation();
        var p = n.getAttribute("data-path");
        var isDir = n.getAttribute("data-isdir") === "1";
        if (isDir) {
          fileCwd = p;
          refreshFiles();
        } else {
          previewFile(p, n.getAttribute("data-preview") || "");
        }
      });
    });
  } catch (e) {
    el.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

function renderTreeNodes(nodes, depth) {
  if (!nodes || !nodes.length) return "";
  return nodes.map(function (n) {
    var pad = depth * 12;
    var icon = n.isDir ? "📁" : fileIcon(n.name || n.path);
    var kids = n.isDir && n.children && n.children.length
      ? '<div class="tree-kids">' + renderTreeNodes(n.children, depth + 1) + "</div>"
      : "";
    return '<div class="tree-node" style="padding-left:' + pad + 'px" data-path="' + escHtml(n.path) +
      '" data-isdir="' + (n.isDir ? "1" : "0") + '" data-preview="' + escHtml(n.previewKind || "") + '">' +
      '<span class="tree-label">' + icon + " " + escHtml(n.name || n.path) + "</span></div>" + kids;
  }).join("");
}

// Attachments state
var pendingAttachments = [];

function renderAttachmentChips() {
  var el = $("attachChips");
  if (!el) return;
  if (!pendingAttachments.length) {
    el.innerHTML = "";
    el.hidden = true;
    return;
  }
  el.hidden = false;
  el.innerHTML = pendingAttachments.map(function (p, i) {
    return '<span class="attach-chip">' + escHtml(p) +
      ' <button type="button" data-i="' + i + '" class="attach-x">×</button></span>';
  }).join("");
  el.querySelectorAll(".attach-x").forEach(function (btn) {
    btn.addEventListener("click", function () {
      pendingAttachments.splice(parseInt(btn.getAttribute("data-i"), 10), 1);
      renderAttachmentChips();
    });
  });
}

function injectAttachmentsIntoPrompt(prompt) {
  if (!pendingAttachments.length) return prompt;
  var block = pendingAttachments.map(function (p) { return "[附件] " + p; }).join("\n");
  pendingAttachments = [];
  renderAttachmentChips();
  return block + "\n" + prompt;
}

async function loadComputeJobs() {
  var el = $("computeJobs");
  if (!el || !activeProject) return;
  try {
    var data = await api("/api/lab/compute/jobs?project_id=" + activeProject.slug);
    var jobs = data.jobs || [];
    if (!jobs.length) {
      el.innerHTML = '<div class="hint">暂无任务 — 用 local 主机试：echo hi &gt; result.txt</div>';
      return;
    }
    el.innerHTML = jobs.map(function (j) {
      var outs = j.outputs || [];
      var outsHtml = outs.map(function (o) {
        return '<div class="job-out-row">' +
          '<span class="mono">' + escHtml(o.path) + (o.error ? " ⚠ " + escHtml(o.error) : "") + "</span>" +
          '<span class="job-out-actions">' +
          '<button type="button" class="btn sm job-import" data-job="' + escHtml(j.id) + '" data-path="' + escHtml(o.path) + '">入库</button>' +
          "</span></div>";
      }).join("");
      var batchBtn = outs.length > 1
        ? '<button type="button" class="btn sm job-import-all" data-job="' + escHtml(j.id) + '">全部入库</button>'
        : "";
      var live = j.status === "running" || j.status === "pending";
      var cancelBtn = live
        ? '<button type="button" class="btn sm job-cancel" data-job="' + escHtml(j.id) + '">取消</button>'
        : "";
      var logBtn = '<button type="button" class="btn sm job-log" data-job="' + escHtml(j.id) + '">SSE 日志</button>';
      var rerunBtn = !live
        ? '<button type="button" class="btn sm job-rerun" data-job="' + escHtml(j.id) + '">重跑</button>'
        : "";
      var copyBtn = '<button type="button" class="btn sm job-copy" data-job="' + escHtml(j.id) + '">复制命令</button>';
      var globsList = j.output_globs || j.globs || [];
      var globsStr = Array.isArray(globsList) ? globsList.join(", ") : String(globsList || "");
      var remoteHint = (j.host && j.host !== "local" && j.host !== "localhost") ? " · SSH" : " · local";
      var meta = "cwd=" + (j.work_dir || "workspace") +
        (j.timeout_sec ? " · timeout=" + j.timeout_sec + "s" : "") +
        (globsStr ? " · globs=" + globsStr : "") +
        (outs.length ? " · harvested=" + outs.length : "");
      return '<div class="job-card status-' + escHtml(j.status || "") + '" data-jid="' + escHtml(j.id) +
        '" data-host="' + escHtml(j.host || "") + '" data-cmd="' + escHtml(j.command || "") +
        '" data-workdir="' + escHtml(j.work_dir || "") +
        '" data-timeout="' + escHtml(String(j.timeout_sec || 600)) +
        '" data-globs="' + escHtml(globsStr) + '">' +
        '<div class="job-hd"><strong>' + escHtml(j.id) + "</strong> · " + escHtml(j.status) + remoteHint +
        (live ? ' <span class="hint">日志实时刷新…</span>' : "") +
        " " + logBtn +
        (cancelBtn ? " " + cancelBtn : "") +
        (rerunBtn ? " " + rerunBtn : "") +
        " " + copyBtn +
        (batchBtn ? " " + batchBtn : "") + "</div>" +
        '<div class="hint mono">' + escHtml(j.host) + " · " + escHtml(j.command) + "</div>" +
        '<div class="hint">' + escHtml(meta) + "</div>" +
        (j.error ? '<div class="ft-err">' + escHtml(j.error) + "</div>" : "") +
        (j.output ? '<pre class="job-out">' + escHtml((j.output || "").slice(-4000)) + "</pre>" : '<pre class="job-out hint">(尚无输出)</pre>') +
        (outsHtml ? '<div class="job-outs">' + outsHtml + "</div>" : "") +
        "</div>";
    }).join("");
    el.querySelectorAll(".job-import").forEach(function (btn) {
      btn.addEventListener("click", function () {
        importComputeOutput(btn.getAttribute("data-job"), btn.getAttribute("data-path"), false);
      });
    });
    el.querySelectorAll(".job-import-all").forEach(function (btn) {
      btn.addEventListener("click", function () {
        importComputeOutput(btn.getAttribute("data-job"), "", true);
      });
    });
    el.querySelectorAll(".job-cancel").forEach(function (btn) {
      btn.addEventListener("click", function () {
        cancelComputeJob(btn.getAttribute("data-job"));
      });
    });
    el.querySelectorAll(".job-log").forEach(function (btn) {
      btn.addEventListener("click", function () {
        watchJobLog(btn.getAttribute("data-job"));
      });
    });
    el.querySelectorAll(".job-copy").forEach(function (btn) {
      btn.addEventListener("click", function () {
        var card = btn.closest(".job-card");
        var cmd = card && card.getAttribute("data-cmd");
        if (cmd) copyTextToClipboard(cmd, btn);
      });
    });
    el.querySelectorAll(".job-rerun").forEach(function (btn) {
      btn.addEventListener("click", function () {
        var card = btn.closest(".job-card");
        if (!card) return;
        if ($("computeHost")) $("computeHost").value = card.getAttribute("data-host") || "local";
        if ($("computeCmd")) $("computeCmd").value = card.getAttribute("data-cmd") || "";
        if ($("computeWorkDir")) $("computeWorkDir").value = card.getAttribute("data-workdir") || "";
        if ($("computeTimeout")) $("computeTimeout").value = card.getAttribute("data-timeout") || "600";
        var g = card.getAttribute("data-globs") || "";
        if ($("computeGlobs")) $("computeGlobs").value = g;
        submitComputeJob();
      });
    });
  } catch (e) {
    el.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

async function cancelComputeJob(jobId) {
  if (!activeProject || !jobId) return;
  try {
    await api("/api/lab/compute/jobs/" + encodeURIComponent(jobId) + "/cancel?project_id=" + activeProject.slug, {
      method: "POST",
      body: "{}",
    });
    loadComputeJobs();
  } catch (e) {
    alert("取消失败: " + e.message);
  }
}

async function importWorkspaceZip(file) {
  if (!activeProject || !file) return;
  try {
    var fd = new FormData();
    fd.append("file", file);
    var res = await fetch(labPath("/api/lab/files/import?project_id=" + activeProject.slug), {
      method: "POST",
      body: fd,
    });
    if (!res.ok) throw new Error(await res.text());
    var body = await res.json();
    var hint = $("composerHint");
    if (hint) hint.textContent = "ZIP 导入 " + (body.count || 0) + " 个文件 → " + (body.dest || "");
    await refreshFiles();
    loadFileTree();
    loadArtifacts();
  } catch (e) {
    alert("导入失败: " + e.message);
  }
}

async function importComputeOutput(jobId, path, all) {
  if (!activeProject || !jobId) return;
  try {
    var body = all ? { all: true } : { path: path };
    var res = await api("/api/lab/compute/jobs/" + encodeURIComponent(jobId) + "/import?project_id=" + activeProject.slug, {
      method: "POST",
      body: JSON.stringify(body),
    });
    var paths = [];
    if (res.workspace_path) paths.push(res.workspace_path);
    (res.imported || []).forEach(function (it) {
      if (it.workspace_path) paths.push(it.workspace_path);
    });
    var inp = $("promptInput");
    if (inp && paths.length) {
      paths.forEach(function (wp) {
        var tag = "\n[产物] " + wp;
        if (inp.value.indexOf(wp) < 0) inp.value = (inp.value || "") + tag;
      });
      if (paths.length) inp.value += "\n";
    }
    await refreshFiles();
    var filesTab = document.querySelector('.insp-tab[data-pane="files"]');
    if (filesTab) filesTab.click();
    if (paths[0]) previewFile(paths[0], res.previewKind || "");
    var hint = $("composerHint");
    if (hint) hint.textContent = "已入库 " + paths.length + " 个产物";
  } catch (e) {
    alert("入库失败: " + e.message);
  }
}

function exportActiveSession(format) {
  if (!activeProject || !activeThread) {
    alert("请先选择会话");
    return;
  }
  format = format || "md";
  var url = labPath("/api/lab/projects/" + activeProject.slug + "/sessions/" + encodeURIComponent(activeThread) + "/export?format=" + format);
  window.open(url, "_blank");
}

async function runSessionSearch() {
  var qEl = $("sessionSearch");
  var hitsEl = $("sessionSearchHits");
  if (!qEl || !hitsEl || !activeProject) return;
  var q = qEl.value.trim();
  if (!q) {
    hitsEl.hidden = true;
    hitsEl.innerHTML = "";
    return;
  }
  try {
    var data = await api("/api/lab/projects/" + activeProject.slug + "/sessions?q=" + encodeURIComponent(q));
    var hits = data.hits || [];
    hitsEl.hidden = false;
    if (!hits.length) {
      hitsEl.innerHTML = '<div class="hint">无匹配会话</div>';
      return;
    }
    hitsEl.innerHTML = hits.map(function (h) {
      return '<div class="ft-row hit sess-hit" data-sid="' + escHtml(h.session_id) + '">' +
        '<span class="ft-name">' + escHtml(h.session_title || h.session_id) +
        (h.role && h.role !== "meta" ? " · " + escHtml(h.role) : "") + "</span>" +
        '<div class="hit-snip">' + escHtml(h.snippet || "") + "</div></div>";
    }).join("");
    hitsEl.querySelectorAll(".sess-hit").forEach(function (row) {
      row.addEventListener("click", function () {
        openSession(row.getAttribute("data-sid"));
      });
    });
  } catch (e) {
    hitsEl.hidden = false;
    hitsEl.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
  }
}

function loadComputeHistory() {
  try {
    return JSON.parse(localStorage.getItem("lumen-compute-history") || "[]");
  } catch (_) {
    return [];
  }
}

function pushComputeHistory(cmd, host) {
  if (!cmd) return;
  var list = loadComputeHistory().filter(function (x) { return x.cmd !== cmd; });
  list.unshift({ cmd: cmd, host: host || "", at: Date.now() });
  list = list.slice(0, 12);
  try { localStorage.setItem("lumen-compute-history", JSON.stringify(list)); } catch (_) {}
  renderComputeHistory();
}

function renderComputeHistory() {
  var el = $("computeHistory");
  if (!el) return;
  var list = loadComputeHistory();
  if (!list.length) {
    el.innerHTML = '<div class="hint">暂无历史命令</div>';
  } else {
    el.innerHTML = list.map(function (h, i) {
      return '<button type="button" class="btn sm hist-cmd" data-i="' + i + '" title="' + escHtml(h.cmd) + '">' +
        escHtml((h.cmd || "").slice(0, 48)) + (h.cmd.length > 48 ? "…" : "") + "</button>";
    }).join(" ");
    el.querySelectorAll(".hist-cmd").forEach(function (btn) {
      btn.addEventListener("click", function () {
        var item = list[parseInt(btn.getAttribute("data-i"), 10)];
        if (!item) return;
        if ($("computeCmd")) $("computeCmd").value = item.cmd;
        if (item.host && $("computeHost")) $("computeHost").value = item.host;
      });
    });
  }
  renderComputeTemplates();
}

function loadComputeTemplates() {
  try {
    return JSON.parse(localStorage.getItem("lumen-compute-templates") || "[]");
  } catch (_) {
    return [];
  }
}
function saveComputeTemplates(list) {
  try { localStorage.setItem("lumen-compute-templates", JSON.stringify(list.slice(0, 20))); } catch (_) {}
}
function renderComputeTemplates() {
  var el = $("computeTemplates");
  if (!el) return;
  var list = loadComputeTemplates();
  if (!list.length) {
    el.innerHTML = '<div class="hint">暂无模板 — 下方可保存当前命令</div>';
    return;
  }
  el.innerHTML = list.map(function (t, i) {
    return '<div class="fav-chip-row">' +
      '<button type="button" class="btn sm tmpl-run" data-i="' + i + '" title="' + escHtml(t.cmd || "") + '">' +
      escHtml(t.name || "模板") + "</button>" +
      '<button type="button" class="btn sm tmpl-del" data-i="' + i + '">×</button></div>';
  }).join("");
  el.querySelectorAll(".tmpl-run").forEach(function (btn) {
    btn.addEventListener("click", function () {
      var item = loadComputeTemplates()[parseInt(btn.getAttribute("data-i"), 10)];
      if (!item) return;
      if ($("computeCmd")) $("computeCmd").value = item.cmd || "";
      if (item.host && $("computeHost")) $("computeHost").value = item.host;
      if (item.globs != null && $("computeGlobs")) $("computeGlobs").value = item.globs;
      if ($("computeWorkDir")) $("computeWorkDir").value = item.work_dir || "";
      if ($("computeTimeout") && item.timeout_sec) $("computeTimeout").value = String(item.timeout_sec);
    });
  });
  el.querySelectorAll(".tmpl-del").forEach(function (btn) {
    btn.addEventListener("click", function (e) {
      e.stopPropagation();
      var list = loadComputeTemplates();
      list.splice(parseInt(btn.getAttribute("data-i"), 10), 1);
      saveComputeTemplates(list);
      renderComputeTemplates();
    });
  });
}
function saveCurrentComputeTemplate() {
  var cmd = ($("computeCmd") && $("computeCmd").value.trim()) || "";
  if (!cmd) { alert("请先填写命令"); return; }
  var name = prompt("模板名称", cmd.slice(0, 24));
  if (name == null) return;
  var host = ($("computeHost") && $("computeHost").value) || "local";
  var globs = ($("computeGlobs") && $("computeGlobs").value) || "";
  var workDir = ($("computeWorkDir") && $("computeWorkDir").value) || "";
  var timeoutSec = ($("computeTimeout") && parseInt($("computeTimeout").value, 10)) || 600;
  var list = loadComputeTemplates().filter(function (t) { return t.name !== name; });
  list.unshift({
    name: (name || "模板").slice(0, 40),
    cmd: cmd, host: host, globs: globs,
    work_dir: workDir, timeout_sec: timeoutSec,
  });
  saveComputeTemplates(list);
  renderComputeTemplates();
  showLabToast("已保存模板", name);
}

async function submitComputeJob() {
  if (!activeProject) return;
  var host = $("computeHost") && $("computeHost").value;
  var cmd = $("computeCmd") && $("computeCmd").value.trim();
  var globsRaw = ($("computeGlobs") && $("computeGlobs").value.trim()) || "";
  var workDir = ($("computeWorkDir") && $("computeWorkDir").value.trim()) || "";
  var timeoutSec = ($("computeTimeout") && parseInt($("computeTimeout").value, 10)) || 600;
  var body = buildComputeJobBody({
    host: host,
    command: cmd,
    work_dir: workDir,
    timeout_sec: timeoutSec,
    output_globs: globsRaw,
  });
  if (!body.host || !body.command) {
    alert("需要主机和命令");
    return;
  }
  try {
    var j = await api("/api/lab/compute/jobs?project_id=" + activeProject.slug, {
      method: "POST",
      body: JSON.stringify(body),
    });
    pushComputeHistory(cmd, host);
    showLabToast("任务已提交", (j && j.id) || host);
    loadComputeJobs();
    if (j && j.id) watchJobLog(j.id);
    // Poll a few times
    var n = 0;
    var timer = setInterval(function () {
      loadComputeJobs();
      if (++n > 20) clearInterval(timer);
    }, 2000);
  } catch (e) {
    alert(e.message);
  }
}

async function loadMoleculeViewer() {
  var el = $("molViewer");
  if (!el) return;
  ensureMoleculeViewer(function (viewer) {
    // default demo if empty
    try {
      if (!viewer) return;
      // keep existing model if user already opened a file
    } catch (_) {}
  });
  // Prefer last molecule path from workspace if any
  if (activeProject) {
    try {
      var data = await api("/api/lab/files/recent?project_id=" + activeProject.slug + "&limit=30");
      var mol = (data.files || []).find(function (f) {
        return f.previewKind === "molecule" || /\.(pdb|sdf|mol|cif)$/i.test(f.path || "");
      });
      if (mol) {
        var content = await api("/api/lab/files/content?project_id=" + activeProject.slug + "&path=" + encodeURIComponent(mol.path));
        if (content.content) openMoleculeFromContent(mol.path, content.content, "molecule");
      } else {
        ensureMoleculeViewer(function (viewer) {
          fetch("https://files.rcsb.org/download/4HHB.pdb").then(function (r) { return r.text(); }).then(function (pdb) {
            viewer.clear();
            viewer.addModel(pdb, "pdb");
            viewer.setStyle({}, { cartoon: { color: "#c28b4b" } });
            viewer.zoomTo();
            viewer.render();
            var label = $("molLabel");
            if (label) label.textContent = "demo: 4HHB.pdb";
          }).catch(function () {});
        });
      }
    } catch (_) {}
  }
}

// Live job log via SSE
var jobLogAbort = null;
function watchJobLog(jobId) {
  if (!activeProject || !jobId) return;
  if (jobLogAbort) try { jobLogAbort.abort(); } catch (_) {}
  jobLogAbort = new AbortController();
  var pre = $("jobLogLive");
  if (pre) {
    pre.hidden = false;
    pre.textContent = "连接日志流 " + jobId + "…\n";
  }
  fetch(labPath("/api/lab/compute/jobs/" + encodeURIComponent(jobId) + "/log?project_id=" + activeProject.slug), {
    signal: jobLogAbort.signal,
    headers: { Accept: "text/event-stream" },
  }).then(function (res) {
    if (!res.ok || !res.body) throw new Error("log stream HTTP " + res.status);
    var reader = res.body.getReader();
    var dec = new TextDecoder();
    var buf = "";
    function pump() {
      return reader.read().then(function (r) {
        if (r.done) return;
        buf += dec.decode(r.value, { stream: true });
        var lines = buf.split("\n");
        buf = lines.pop();
        for (var i = 0; i < lines.length; i++) {
          var line = lines[i];
          if (line.indexOf("data:") !== 0) continue;
          var json = line.slice(5).trim();
          if (!json.startsWith("{")) continue;
          try {
            var ev = JSON.parse(json);
            if (pre && ev.output != null) {
              pre.textContent = ev.output || "";
              pre.scrollTop = pre.scrollHeight;
            }
            if (ev.status && ev.status !== "running" && ev.status !== "pending") {
              loadComputeJobs();
            }
          } catch (_) {}
        }
        return pump();
      });
    }
    return pump();
  }).catch(function (e) {
    if (e.name === "AbortError") return;
    if (pre) pre.textContent += "\n日志流结束: " + e.message;
  });
}

/* ── 13. Command palette (⌘K / Ctrl+K) ── */

var paletteCmds = [
  { label: "新建课题", action: function () { $("newProjectBtn") && $("newProjectBtn").click(); }, hotkey: "⌘N" },
  { label: "重命名当前课题", action: function () { if (activeProject) renameProject(activeProject.slug, activeProject.title); } },
  { label: "一键 Brief: aspirin", action: function () { ensureProject().then(function (p) { return api("/api/lab/brief", { method: "POST", body: JSON.stringify({ project_id: p.slug, topic: "aspirin" }) }); }).then(function (r) { var d = document.createElement("div"); d.className = "chat-msg agent"; d.textContent = "Brief 已写入 " + r.path; $("chatScroll").appendChild(d); }).catch(function (e) { addErrorBubble($("chatScroll"), e.message); }); } },
  { label: "文献检索: PubMed", action: function () { streamChat("用 pubmed 域检索最新文献").catch(function (e) { addErrorBubble($("chatScroll"), e.message); }); } },
  { label: "打开 Bridge", action: function () { window.open(API_BASE ? "/lumen-science/?embed=1&oasis=1" : "http://127.0.0.1:18990/", "_blank"); } },
  { label: "刷新状态", action: function () { refreshHealth(); } },
  { label: "复制当前对话 Markdown", action: function () { copyConversationMarkdown(); } },
  { label: "导出当前会话 Markdown", action: function () { exportActiveSession("md"); } },
  { label: "重命名当前会话", action: function () { if (activeThread) renameSession(activeThread); } },
  { label: "分支当前会话", action: function () { if (activeThread) forkSession(activeThread); } },
  { label: "导出全部会话 Markdown", action: function () {
    if (!activeProject) return;
    window.open(labPath("/api/lab/projects/" + activeProject.slug + "/sessions/export-all?format=md"), "_blank");
  } },
  { label: "快捷键帮助", action: function () { openShortcuts(); }, hotkey: "⌘/" },
  { label: "切换紧凑布局", action: function () { toggleCompactMode(); } },
  { label: "置顶/取消当前会话", action: function () { if (activeThread) togglePin(activeThread); } },
  { label: "删除当前会话", action: function () { if (activeThread) deleteSession(activeThread); } },
  { label: "打开产物面板", action: function () { var t = document.querySelector('.insp-tab[data-pane="artifacts"]'); if (t) t.click(); } },
  { label: "打开 Notebook 面板", action: function () { var t = document.querySelector('.insp-tab[data-pane="notebooks"]'); if (t) t.click(); } },
  { label: "打开 LangGraph 面板", action: function () { var t = document.querySelector('.insp-tab[data-pane="langgraph"]'); if (t) t.click(); } },
  { label: "重跑上一条消息", action: function () { rerunLastPrompt(); } },
  { label: "新建工作区文件", action: function () { var t = document.querySelector('.insp-tab[data-pane="files"]'); if (t) t.click(); fileNewPrompt(); } },
  { label: "在对话中查找", action: function () { openChatFind(); }, hotkey: "⌘F" },
  { label: "引用所选文件到对话", action: function () {
    var t = document.querySelector('.insp-tab[data-pane="files"]');
    if (t) t.click();
    attachSelectedToComposer();
  } },
];

var paletteDynamic = []; // {label, kind, action, hotkey}
var paletteTimer = null;

function openPalette() {
  var el = $("cmdPalette");
  if (!el) return;
  el.style.display = "flex";
  var inp = $("paletteInput");
  inp.value = "";
  inp.placeholder = "命令 · 文件 · 会话…";
  inp.focus();
  paletteDynamic = [];
  renderPaletteItems("");
}
function closePalette() {
  var el = $("cmdPalette");
  if (el) el.style.display = "none";
  paletteDynamic = [];
}
function allPaletteItems(filter) {
  var q = (filter || "").toLowerCase();
  var cmds = paletteCmds.filter(function (c) {
    return !q || c.label.toLowerCase().indexOf(q) !== -1;
  }).map(function (c) {
    return { label: c.label, hotkey: c.hotkey || "", kind: "cmd", action: c.action };
  });
  var dyn = paletteDynamic.filter(function (c) {
    return !q || (c.label || "").toLowerCase().indexOf(q) !== -1;
  });
  return cmds.concat(dyn).slice(0, 40);
}
function renderPaletteItems(filter) {
  var res = $("paletteResults");
  if (!res) return;
  var items = allPaletteItems(filter);
  if (!items.length) {
    res.innerHTML = '<div class="item hint">无匹配 — 试试文件名或会话关键词</div>';
    return;
  }
  res.innerHTML = items.map(function (c, i) {
    var badge = c.kind === "file" ? "📄" : (c.kind === "sess" ? "💬" : "⚡");
    return '<div class="item" data-idx="' + i + '"><span>' + badge + " " + escHtml(c.label) +
      '</span><span class="hotkey">' + escHtml(c.hotkey || (c.kind === "file" ? "file" : c.kind === "sess" ? "sess" : "")) + "</span></div>";
  }).join("");
  res.querySelectorAll(".item").forEach(function (el) {
    el.addEventListener("click", function () {
      var q = ($("paletteInput") && $("paletteInput").value) || filter || "";
      var items2 = allPaletteItems(q);
      var cmd = items2[parseInt(el.dataset.idx)];
      if (cmd && cmd.action) { closePalette(); cmd.action(); }
    });
  });
}

async function enrichPalette(q) {
  paletteDynamic = [];
  if (!q || q.length < 1 || !activeProject) {
    renderPaletteItems(q);
    return;
  }
  try {
    // files
    var files = await api("/api/lab/files/search?project_id=" + activeProject.slug +
      "&q=" + encodeURIComponent(q) + "&limit=8");
    (files.hits || []).forEach(function (h) {
      if (h.isDir) return;
      var path = h.path || h.name;
      paletteDynamic.push({
        label: path,
        kind: "file",
        action: function () {
          var tab = document.querySelector('.insp-tab[data-pane="files"]');
          if (tab) tab.click();
          previewFile(path, h.previewKind || "");
        },
      });
    });
  } catch (_) {}
  try {
    var sess = await api("/api/lab/projects/" + activeProject.slug + "/sessions?q=" + encodeURIComponent(q));
    var seen = {};
    (sess.hits || []).forEach(function (h) {
      if (!h.session_id || seen[h.session_id]) return;
      seen[h.session_id] = true;
      paletteDynamic.push({
        label: (h.session_title || h.session_id) + " · " + (h.snippet || "").slice(0, 40),
        kind: "sess",
        action: function () { openSession(h.session_id); },
      });
    });
  } catch (_) {}
  // also match thread titles locally
  threads.forEach(function (t) {
    if ((t.title || "").toLowerCase().indexOf(q.toLowerCase()) >= 0) {
      paletteDynamic.push({
        label: t.title || t.id,
        kind: "sess",
        action: function () { openSession(t.id); },
      });
    }
  });
  renderPaletteItems(q);
}

$("paletteInput") && $("paletteInput").addEventListener("input", function (e) {
  var q = e.target.value || "";
  renderPaletteItems(q);
  clearTimeout(paletteTimer);
  paletteTimer = setTimeout(function () { enrichPalette(q); }, 180);
});
$("paletteInput") && $("paletteInput").addEventListener("keydown", function (e) {
  if (e.key === "Escape") closePalette();
  if (e.key === "Enter") {
    var sel = document.querySelector("#paletteResults .item");
    if (sel) sel.click();
  }
});
$("cmdPalette") && $("cmdPalette").addEventListener("click", function (e) { if (e.target === $("cmdPalette")) closePalette(); });
document.addEventListener("keydown", function (e) {
  if ((e.metaKey || e.ctrlKey) && e.key === "k") { e.preventDefault(); openPalette(); }
  if ((e.metaKey || e.ctrlKey) && e.key === "/") { e.preventDefault(); openShortcuts(); }
  if ((e.metaKey || e.ctrlKey) && (e.key === "f" || e.key === "F")) {
    // find in chat when focus is in center
    var t = e.target;
    var tag = (t && t.tagName) || "";
    if (tag === "INPUT" || tag === "TEXTAREA") return;
    e.preventDefault();
    openChatFind();
  }
  if (e.key === "Escape") {
    closeShortcuts();
    closePalette();
    hideAtMenu();
    closeChatFind();
  }
});

/* ── 14. Embed + Resize ── */

var params = new URLSearchParams(location.search);
if (params.get("embed") || params.get("oasis")) document.body.classList.add("embed-oasis");

(function () {
  function restorePanelWidth(panel, key) {
    if (!panel) return;
    try {
      var w = parseInt(localStorage.getItem(key) || "", 10);
      if (w >= 180 && w <= 520) panel.style.width = w + "px";
    } catch (_) {}
  }
  function makeResizable(handle, panel, isRight, key) {
    if (!handle || !panel) return;
    restorePanelWidth(panel, key);
    var startX, startW;
    handle.addEventListener("mousedown", function (e) {
      startX = e.clientX; startW = panel.getBoundingClientRect().width;
      document.body.style.cursor = "col-resize"; document.body.style.userSelect = "none";
      handle.classList.add("active");
      function onMove(e) {
        var delta = isRight ? startX - e.clientX : e.clientX - startX;
        var w = Math.max(180, Math.min(520, startW + delta));
        panel.style.width = w + "px";
      }
      function onUp() {
        document.body.style.cursor = "";
        document.body.style.userSelect = "";
        handle.classList.remove("active");
        document.removeEventListener("mousemove", onMove);
        document.removeEventListener("mouseup", onUp);
        try {
          localStorage.setItem(key, String(Math.round(panel.getBoundingClientRect().width)));
        } catch (_) {}
      }
      document.addEventListener("mousemove", onMove);
      document.addEventListener("mouseup", onUp);
    });
  }
  makeResizable($("resizeLeft"), $("sidebarPanel"), false, "lumen-side-w");
  makeResizable($("resizeRight"), $("inspectorPanel"), true, "lumen-insp-w");
})();

// Productivity wiring
$("skillsSaveBtn") && $("skillsSaveBtn").addEventListener("click", saveSkillsEnabled);
$("fileSearchBtn") && $("fileSearchBtn").addEventListener("click", runFileSearch);
$("fileSearch") && $("fileSearch").addEventListener("keydown", function (e) {
  if (e.key === "Enter") { e.preventDefault(); runFileSearch(); }
});
$("computeSubmit") && $("computeSubmit").addEventListener("click", submitComputeJob);
$("computeRefresh") && $("computeRefresh").addEventListener("click", loadComputeJobs);
$("computeSaveTmpl") && $("computeSaveTmpl").addEventListener("click", saveCurrentComputeTemplate);
$("chatFindBtn") && $("chatFindBtn").addEventListener("click", openChatFind);
$("sessionSearchBtn") && $("sessionSearchBtn").addEventListener("click", runSessionSearch);
$("sessionSearch") && $("sessionSearch").addEventListener("keydown", function (e) {
  if (e.key === "Enter") { e.preventDefault(); runSessionSearch(); }
});
$("copyConvBtn") && $("copyConvBtn").addEventListener("click", copyConversationMarkdown);
$("exportMdBtn") && $("exportMdBtn").addEventListener("click", function () { exportActiveSession("md"); });
$("exportJsonBtn") && $("exportJsonBtn").addEventListener("click", function () { exportActiveSession("json"); });
$("exportAllSessBtn") && $("exportAllSessBtn").addEventListener("click", function () {
  if (!activeProject) { alert("请先选择课题"); return; }
  var fmt = confirm("导出 JSON？\n确定=JSON，取消=Markdown") ? "json" : "md";
  window.open(labPath("/api/lab/projects/" + activeProject.slug + "/sessions/export-all?format=" + fmt), "_blank");
});
$("importSessFile") && $("importSessFile").addEventListener("change", async function () {
  var f = this.files && this.files[0];
  this.value = "";
  if (!f || !activeProject) {
    if (!activeProject) alert("请先选择课题");
    return;
  }
  try {
    var text = await f.text();
    var data = JSON.parse(text);
    var payload = {};
    if (data.turns) {
      payload = { title: data.title || f.name, turns: data.turns };
    } else if (data.sessions && data.sessions[0]) {
      // export-all pack — import first session only, or all
      var list = data.sessions;
      if (list.length > 1 && !confirm("检测到 " + list.length + " 个会话，全部导入？\n取消=只导入第一个")) {
        list = [list[0]];
      }
      for (var i = 0; i < list.length; i++) {
        await api("/api/lab/projects/" + activeProject.slug + "/sessions/import", {
          method: "POST",
          body: JSON.stringify({ title: list[i].title, turns: list[i].turns || [], session: list[i] }),
        });
      }
      await loadSessions();
      if (threads.length) openSession(threads[0].id);
      return;
    } else if (data.role || Array.isArray(data)) {
      payload = { title: "导入 " + f.name, turns: Array.isArray(data) ? data : [data] };
    } else {
      payload = { title: data.title || f.name, turns: data.turns || [], session: data };
    }
    var sess = await api("/api/lab/projects/" + activeProject.slug + "/sessions/import", {
      method: "POST",
      body: JSON.stringify(payload),
    });
    await loadSessions();
    if (sess.id) openSession(sess.id);
  } catch (e) {
    alert("导入失败: " + e.message);
  }
});
$("forkSessBtn") && $("forkSessBtn").addEventListener("click", function () {
  if (activeThread) forkSession(activeThread);
  else alert("请先选择会话");
});
$("deleteSessBtn") && $("deleteSessBtn").addEventListener("click", function () {
  if (activeThread) deleteSession(activeThread);
  else alert("请先选择会话");
});

function openShortcuts() {
  var m = $("shortcutsModal");
  if (m) m.hidden = false;
}
function closeShortcuts() {
  var m = $("shortcutsModal");
  if (m) m.hidden = true;
}
$("btnShortcuts") && $("btnShortcuts").addEventListener("click", openShortcuts);
$("shortcutsClose") && $("shortcutsClose").addEventListener("click", closeShortcuts);
$("shortcutsModal") && $("shortcutsModal").addEventListener("click", function (e) {
  if (e.target === $("shortcutsModal")) closeShortcuts();
});
$("artifactsRefresh") && $("artifactsRefresh").addEventListener("click", loadArtifacts);
$("workspaceExportBtn") && $("workspaceExportBtn").addEventListener("click", function () {
  if (!activeProject) { alert("请先选择课题"); return; }
  window.open(labPath("/api/lab/files/export?project_id=" + encodeURIComponent(activeProject.slug)), "_blank");
});
$("workspaceImport") && $("workspaceImport").addEventListener("change", function () {
  var f = this.files && this.files[0];
  if (f) importWorkspaceZip(f);
  this.value = "";
});
$("cfgSaveBtn") && $("cfgSaveBtn").addEventListener("click", saveLabConfig);
$("diffRunBtn") && $("diffRunBtn").addEventListener("click", runFileDiff);
$("provRefreshBtn") && $("provRefreshBtn").addEventListener("click", loadProvenanceBrowser);
$("provFilter") && $("provFilter").addEventListener("keydown", function (e) {
  if (e.key === "Enter") { e.preventDefault(); loadProvenanceBrowser(); }
});
$("molSaveBtn") && $("molSaveBtn").addEventListener("click", saveMolToWorkspace);
$("molLoadBtn") && $("molLoadBtn").addEventListener("click", loadMolFromWorkspace);
$("molFromKetcher") && $("molFromKetcher").addEventListener("click", importFromKetcher);
$("molToKetcher") && $("molToKetcher").addEventListener("click", pushToKetcher);
$("molTo3dBtn") && $("molTo3dBtn").addEventListener("click", function () {
  var path = normalizeMolPath(($("molSavePath") && $("molSavePath").value) || "structure.mol");
  var content = ($("molEditor") && $("molEditor").value) || "";
  var v = validateMolContent(content);
  if (!v.ok) {
    setMolStatus("无法打开 3D：内容无效 (" + v.reason + ")", false);
    alert("编辑器为空或不是分子结构文本");
    return;
  }
  openMoleculeFromContent(path, content, "molecule");
  setMolStatus("已送入 3Dmol (" + v.reason + ")", true);
});
$("skillsImport") && $("skillsImport").addEventListener("change", function () {
  var f = this.files && this.files[0];
  if (f) importSkillFile(f);
  this.value = "";
});
$("hostRegisterBtn") && $("hostRegisterBtn").addEventListener("click", registerHost);
$("fileTreeRefresh") && $("fileTreeRefresh").addEventListener("click", loadFileTree);
$("previewModalClose") && $("previewModalClose").addEventListener("click", closePreviewModal);
$("previewModal") && $("previewModal").addEventListener("click", function (e) {
  if (e.target === $("previewModal")) closePreviewModal();
});
function bindChatScrollJump() {
  var scroll = $("chatScroll");
  if (!scroll) return;
  var btn = $("jumpBottomBtn");
  if (!btn) {
    btn = document.createElement("button");
    btn.type = "button";
    btn.id = "jumpBottomBtn";
    btn.className = "jump-bottom";
    btn.hidden = true;
    btn.textContent = "↓ 最新";
    btn.title = "滚到最新";
    var ctr = scroll.parentElement || document.body;
    if (getComputedStyle(ctr).position === "static") ctr.style.position = "relative";
    ctr.appendChild(btn);
    btn.addEventListener("click", function () {
      scroll.scrollTop = scroll.scrollHeight;
      btn.hidden = true;
    });
  }
  scroll.addEventListener("scroll", function () {
    var dist = scroll.scrollHeight - scroll.scrollTop - scroll.clientHeight;
    btn.hidden = dist < 120;
  });
}

/* ── Find in chat ── */
var chatFindMatches = [];
var chatFindIdx = -1;

function openChatFind() {
  var bar = $("chatFindBar");
  if (!bar) {
    bar = document.createElement("div");
    bar.id = "chatFindBar";
    bar.className = "chat-find-bar";
    bar.innerHTML =
      '<input type="search" id="chatFindInput" class="file-search" placeholder="在当前对话中查找…" />' +
      '<span class="hint" id="chatFindCount">0</span>' +
      '<button type="button" class="btn sm" id="chatFindPrev">↑</button>' +
      '<button type="button" class="btn sm" id="chatFindNext">↓</button>' +
      '<button type="button" class="btn sm" id="chatFindClose">×</button>';
    var ctr = ($("chatScroll") && $("chatScroll").parentElement) || document.body;
    ctr.appendChild(bar);
    $("chatFindClose").addEventListener("click", closeChatFind);
    $("chatFindPrev").addEventListener("click", function () { stepChatFind(-1); });
    $("chatFindNext").addEventListener("click", function () { stepChatFind(1); });
    $("chatFindInput").addEventListener("input", function () { runChatFind(this.value); });
    $("chatFindInput").addEventListener("keydown", function (e) {
      if (e.key === "Enter") { e.preventDefault(); stepChatFind(e.shiftKey ? -1 : 1); }
      if (e.key === "Escape") closeChatFind();
    });
  }
  bar.hidden = false;
  var inp = $("chatFindInput");
  if (inp) { inp.focus(); inp.select(); runChatFind(inp.value); }
}

function closeChatFind() {
  var bar = $("chatFindBar");
  if (bar) bar.hidden = true;
  clearChatFindMarks();
  chatFindMatches = [];
  chatFindIdx = -1;
}

function clearChatFindMarks() {
  document.querySelectorAll(".chat-find-hit").forEach(function (el) {
    el.classList.remove("chat-find-hit", "chat-find-cur");
  });
}

function runChatFind(q) {
  clearChatFindMarks();
  chatFindMatches = [];
  chatFindIdx = -1;
  var countEl = $("chatFindCount");
  q = (q || "").trim().toLowerCase();
  if (!q) {
    if (countEl) countEl.textContent = "0";
    return;
  }
  var scroll = $("chatScroll");
  if (!scroll) return;
  var nodes = scroll.querySelectorAll(".msg-user-text, .agent-text, .tool-card-name, .tool-card-output, .tool-card-args");
  nodes.forEach(function (n) {
    var text = (n.innerText || n.textContent || "").toLowerCase();
    if (text.indexOf(q) >= 0) {
      n.classList.add("chat-find-hit");
      chatFindMatches.push(n);
    }
  });
  if (countEl) countEl.textContent = String(chatFindMatches.length);
  if (chatFindMatches.length) stepChatFind(1);
}

function stepChatFind(dir) {
  if (!chatFindMatches.length) return;
  chatFindMatches.forEach(function (n) { n.classList.remove("chat-find-cur"); });
  chatFindIdx = (chatFindIdx + dir + chatFindMatches.length) % chatFindMatches.length;
  var el = chatFindMatches[chatFindIdx];
  if (el) {
    el.classList.add("chat-find-cur");
    el.scrollIntoView({ behavior: "smooth", block: "center" });
  }
  var countEl = $("chatFindCount");
  if (countEl) countEl.textContent = (chatFindIdx + 1) + "/" + chatFindMatches.length;
}

/* ── @path autocomplete ── */
var atMenuTimer = null;

function ensureAtMenu() {
  var m = $("atMenu");
  if (m) return m;
  m = document.createElement("div");
  m.id = "atMenu";
  m.className = "at-menu";
  m.hidden = true;
  var wrap = $("composerDrop") || $("composer") || document.body;
  wrap.appendChild(m);
  return m;
}

function hideAtMenu() {
  atMenuState.active = false;
  atMenuState.items = [];
  atMenuState.idx = 0;
  var m = $("atMenu");
  if (m) { m.hidden = true; m.innerHTML = ""; }
}

function renderAtMenu() {
  var m = ensureAtMenu();
  var items = atMenuState.items;
  if (!items.length) {
    m.innerHTML = '<div class="at-empty hint">无匹配文件</div>';
    m.hidden = false;
    atMenuState.active = true;
    return;
  }
  m.innerHTML = items.map(function (it, i) {
    return '<button type="button" class="at-item' + (i === atMenuState.idx ? " active" : "") +
      '" data-i="' + i + '">' + escHtml(it.path || it) + "</button>";
  }).join("");
  m.hidden = false;
  atMenuState.active = true;
  m.querySelectorAll(".at-item").forEach(function (btn) {
    btn.addEventListener("mousedown", function (e) {
      e.preventDefault();
      pickAtItem(parseInt(btn.getAttribute("data-i"), 10));
    });
  });
}

function pickAtItem(i) {
  var inp = $("promptInput");
  if (!inp || !atMenuState.items[i]) return;
  var path = atMenuState.items[i].path || atMenuState.items[i];
  var val = inp.value || "";
  var start = atMenuState.start;
  if (start < 0) start = val.lastIndexOf("@");
  var caret = typeof inp.selectionStart === "number" ? inp.selectionStart : val.length;
  // replace from @start to caret
  var before = val.slice(0, start);
  var after = val.slice(caret);
  var insert = "@" + path + " ";
  inp.value = before + insert + after;
  var pos = (before + insert).length;
  try { inp.setSelectionRange(pos, pos); } catch (_) {}
  hideAtMenu();
  inp.focus();
}

async function queryAtPaths(q) {
  if (!activeProject) return [];
  try {
    if (q) {
      var data = await api("/api/lab/files/search?project_id=" + activeProject.slug +
        "&q=" + encodeURIComponent(q) + "&limit=12");
      return (data.hits || []).filter(function (h) { return !h.isDir; }).slice(0, 12);
    }
    var recent = await api("/api/lab/files/recent?project_id=" + activeProject.slug + "&limit=12");
    return (recent.files || []).slice(0, 12).map(function (f) {
      return { path: f.path || f.name };
    });
  } catch (_) {
    return [];
  }
}

function onPromptAtInput() {
  var inp = $("promptInput");
  if (!inp) return;
  var val = inp.value || "";
  var caret = typeof inp.selectionStart === "number" ? inp.selectionStart : val.length;
  var before = val.slice(0, caret);
  var m = before.match(/(?:^|[\s\n])@([\w./\-]*)$/);
  if (!m) {
    hideAtMenu();
    return;
  }
  atMenuState.start = before.length - m[1].length - 1; // position of @
  var q = m[1] || "";
  clearTimeout(atMenuTimer);
  atMenuTimer = setTimeout(async function () {
    atMenuState.items = await queryAtPaths(q);
    atMenuState.idx = 0;
    renderAtMenu();
  }, 120);
}

$("promptInput") && $("promptInput").addEventListener("input", onPromptAtInput);
$("promptInput") && $("promptInput").addEventListener("keydown", function (e) {
  if (!atMenuState.active) return;
  if (e.key === "ArrowDown") {
    e.preventDefault();
    atMenuState.idx = Math.min(atMenuState.items.length - 1, atMenuState.idx + 1);
    renderAtMenu();
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    atMenuState.idx = Math.max(0, atMenuState.idx - 1);
    renderAtMenu();
  } else if (e.key === "Enter" || e.key === "Tab") {
    if (atMenuState.items.length) {
      e.preventDefault();
      pickAtItem(atMenuState.idx);
    }
  } else if (e.key === "Escape") {
    hideAtMenu();
  }
});
// hide when clicking outside
document.addEventListener("click", function (e) {
  var m = $("atMenu");
  if (!m || m.hidden) return;
  if (e.target === $("promptInput") || (m.contains && m.contains(e.target))) return;
  hideAtMenu();
});

/* ── 15. Init ── */

(async function init() {
  try {
    await refreshHealth();
    await loadProjects();
    if (activeProject) {
      await loadSessions();
      if (activeThread) await openSession(activeThread);
      else loadComposerDraft();
      loadSkills();
      loadComputeHosts();
    } else {
      renderThreadTabs();
    }
  } catch (e) {
    var ib = $("inspectorBody");
    if (ib) ib.innerHTML = '<div class="ft-err">' + escHtml(e.message) + "</div>";
    try { showLabToast("初始化失败", e.message || String(e)); } catch (_) {}
  } finally {
    // Always dismiss splash so users are never stuck on "正在加载"
    try {
      var s0 = $("splash");
      if (s0) s0.classList.add("hide");
    } catch (_) {}
  }
  // periodic health refresh
  setInterval(function () { refreshHealth().catch(function () {}); }, 45000);
  bindChatScrollJump();
  renderComputeHistory();
  renderFavChips();
  // Default right pane = files (OCS-style workspace). Restore last pane if user chose one.
  try {
    var lastPane = localStorage.getItem("lumen-last-pane") || "files";
    var tab = document.querySelector('.insp-tab[data-pane="' + lastPane + '"]');
    if (tab) tab.click();
    else {
      var filesTab = document.querySelector('.insp-tab[data-pane="files"]');
      if (filesTab) filesTab.click();
    }
  } catch (_) {
    try {
      var ft = document.querySelector('.insp-tab[data-pane="files"]');
      if (ft) ft.click();
    } catch (__) {}
  }
  bindWelcomeChips(document);
  setTimeout(function () {
    var s = $("splash");
    if (s) s.classList.add("hide");
  }, 400);
})();
