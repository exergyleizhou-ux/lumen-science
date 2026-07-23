// Science GUI — (5-ship native MCP fleet + token ops)
const $ = (id) => document.getElementById(id);
// When proxied under /lumen-science on the Oasis site, prefix API + SSE paths.
const API = (typeof location !== "undefined" && location.pathname.startsWith("/lumen-science"))
  ? "/lumen-science"
  : "";
const FETCH_TIMEOUT_MS = 30000;

/** True when served on public Oasis (no local Claude Science.app). */
function isOasisPublicHost() {
  if (typeof location === "undefined") return false;
  if (location.pathname.startsWith("/lumen-science")) return true;
  return /oasisdata2026\.xyz$/i.test(location.hostname || "");
}

/** Lab workbench base URL — same-origin on Oasis, loopback on desktop. */
function labBaseURL() {
  if (isOasisPublicHost()) {
    return (location.origin || "") + "/lumen-lab/";
  }
  if (typeof location !== "undefined" && location.protocol === "https:") {
    return "https://127.0.0.1:18995/";
  }
  return "http://127.0.0.1:18992/";
}

function labHealthURL() {
  if (isOasisPublicHost()) return "/lumen-lab/api/lab/health";
  return "http://127.0.0.1:18992/api/lab/health";
}

function openLabWorkbench(extraQuery) {
  let u = labBaseURL();
  // Only allow ascii query keys to avoid Safari data-detectors inventing paths.
  if (extraQuery && /^[a-zA-Z0-9_=&%-]+$/.test(String(extraQuery))) {
    u += (u.indexOf("?") >= 0 ? "&" : "?") + String(extraQuery).replace(/^\?/, "");
  }
  // Same-tab navigation is more reliable on mobile Safari than window.open.
  if (isOasisPublicHost()) {
    location.href = u;
  } else {
    window.open(u, "_blank");
  }
  return u;
}
let mode = "proxy";
let busy = false;
let keys = {};
let profiles = [];
let activeProfileId = "";
let templates = [];
let evtSrc = null;
let sseBackoff = 2000;
let modalLogWhich = "proxy";
let panelStartedAt = Date.now();

const KEY_LABELS = {
  deepseek: "DeepSeek API Key",
  qwen: "DashScope API Key",
  moonshot: "Moonshot API Key",
  zhipu: "智谱 API Key",
};

const PROVIDER_LABELS = {
  deepseek: "DeepSeek",
  qwen: "通义千问",
  moonshot: "Moonshot",
  zhipu: "智谱",
};

const STATE_LABELS = { green: "正常", amber: "待机", red: "异常" };
const BADGE_CLASS = { green: "ok", amber: "warn", red: "err" };

function fmtUptime(ms) {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = String(Math.floor(s / 3600)).padStart(2, "0");
  const m = String(Math.floor((s % 3600) / 60)).padStart(2, "0");
  const sec = String(s % 60).padStart(2, "0");
  return `${h}:${m}:${sec}`;
}

function setFlow(rowId, badgeId, state) {
  const badge = $(badgeId);
  if (!badge) return;
  const cls = BADGE_CLASS[state] || "warn";
  badge.className = "badge " + cls;
  badge.textContent = STATE_LABELS[state] || STATE_LABELS.amber;
  if (rowId === "stSandbox") {
    const box = $("stSandbox");
    if (box) box.className = "flow-sandbox" + (cls === "ok" ? "" : " idle");
    const inner = $("stSandboxInner");
    if (inner) inner.className = "flow-row inner " + cls;
    return;
  }
  const row = $(rowId);
  if (row) row.className = "flow-row " + cls;
}

const OASIS_MSG_AUTH = "lumen-science:oasis-auth";
const OASIS_MSG_REQUEST = "lumen-science:request-oauth";

function setBusy(on) {
  busy = on;
  ["heroBtn", "stopBtn", "saveKeyBtn", "verifyKeyBtn", "oasisPublishBtn", "oasisResearchBtn", "oasisLoginBtn", "oasisNativeVerifyBtn", "oasisBriefBtn", "oasisProbeBtn", "oasisC2DAlgoBtn"].forEach((id) => {
    const el = $(id);
    if (el) el.disabled = on;
  });
}

function setMsg(text, kind) {
  const el = $("msg");
  if (!el) return;
  el.textContent = text;
  el.className = "msg" + (kind ? " " + kind : "");
}

function setConn(live) {
  const el = $("connDot");
  if (!el) return;
  el.textContent = live ? "实时" : "重连中";
  el.className = "live-pill" + (live ? " live" : " dead");
  el.title = live ? "实时连接正常" : "连接断开，自动重连中…";
}

function updateTelemetry(extra = {}) {
  const p = extra.provider || $("provider")?.value || "—";
  const proxyPort = extra.proxy_port ?? ($("proxyPort")?.value || "—");
  const sandboxPort = extra.sandbox_port ?? ($("sandboxPort")?.value || "—");
  const m = extra.mode || mode;
  if ($("telProvider")) $("telProvider").textContent = PROVIDER_LABELS[p] || String(p);
  if ($("telProxy")) $("telProxy").textContent = ":" + proxyPort;
  if ($("telSandbox")) $("telSandbox").textContent = ":" + sandboxPort;
  if ($("telMode")) $("telMode").textContent = m === "official" ? "官方" : "第三方";
  if ($("telParity")) {
    const parity = extra.csswitch_parity || "0.4.1";
    const gw = extra.gateway || "go";
    $("telParity").textContent = "CS " + parity + " · " + gw;
    $("telParity").title =
      "行为对标 CSSwitch " + parity + "（官方数据面为 Rust gateway；Lumen 使用 Go 代理）";
  }
  if ($("flowProxy")) $("flowProxy").textContent = `PROXY · :${proxyPort}`;
  if ($("flowSandbox")) $("flowSandbox").textContent = `SANDBOX · :${sandboxPort}`;
}

async function api(path, opts = {}) {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), FETCH_TIMEOUT_MS);
  try {
    const res = await fetch(API + path, {
      headers: { "Content-Type": "application/json", ...(opts.headers || {}) },
      signal: ctrl.signal,
      ...opts,
    });
    const data = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(data.error || res.statusText);
    return data;
  } catch (e) {
    if (e.name === "AbortError") throw new Error("请求超时，请稍后重试");
    throw e;
  } finally {
    clearTimeout(timer);
  }
}

function applyStatus(st) {
  setFlow("stProxy", "badgeProxy", st.proxy);
  setFlow("stSandbox", "badgeSandbox", st.sandbox);
  setFlow("stUpstream", "badgeUpstream", st.upstream);
  const orb = $("brandDot");
  if (orb) orb.classList.toggle("live", st.proxy === "green" || st.sandbox === "green");
  updateTelemetry({
    provider: st.provider,
    mode: st.mode,
    proxy_port: st.proxy_port,
    sandbox_port: st.sandbox_port,
  });
  const pct = Number(st.cache_session_hit_pct || 0);
  const cacheBar = $("cacheBar");
  if (cacheBar && (pct > 0 || Number(st.cache_hit_tokens) > 0)) {
    cacheBar.hidden = false;
    if ($("cacheFill")) $("cacheFill").style.width = Math.min(100, pct) + "%";
    if ($("cacheTxt")) {
      $("cacheTxt").textContent =
        `缓存 ${pct}% · 上次 ${st.cache_last_hit_pct || 0}% · ${st.cache_hit_tokens || 0} tok`;
    }
  }
  const url = st.url || "";
  const bar = $("urlBar");
  if (bar) {
    if (url) {
      bar.hidden = false;
      bar.href = url;
      const textEl = bar.querySelector(".url-text");
      if (textEl) textEl.textContent = url;
    } else {
      bar.hidden = true;
    }
  }
}

function applyMode(m) {
  mode = m;
  const shell = $("shell");
  if (shell) shell.classList.toggle("mode-official", m === "official");

  // Lab mode: show iframe, hide bridge
  const bridgeEl = $("bridgeContent");
  const labEl = $("labContainer");
  const isLab = m === "lab";

  if (bridgeEl) bridgeEl.style.display = isLab ? "none" : "";
  if (labEl) {
    labEl.style.display = isLab ? "block" : "none";
    if (isLab) {
      const frame = $("labFrame");
      // Always refresh to public /lumen-lab/ when on Oasis (never 127.0.0.1).
      if (frame) {
        const want = labBaseURL() + "?embed=1";
        if (frame.src === "about:blank" || frame.src.indexOf("127.0.0.1") >= 0 || isOasisPublicHost()) {
          frame.src = want;
        }
      }
    }
  }

  // Official hint
  const alertEl = document.querySelector(".alert.info.official-only");
  if (alertEl) alertEl.style.display = (m === "official" || m === "lab") ? "none" : "";

  document.querySelectorAll(".tabs .tab").forEach((b) => {
    b.classList.toggle("active", b.dataset.mode === m);
  });
  const heroLabel = $("heroBtn")?.querySelector(".hero-label");
  if (heroLabel) {
    if (m === "official") heroLabel.textContent = "打开官方 Claude Science";
    else if (m === "lab") heroLabel.textContent = "打开实验室";
    else if (isOasisPublicHost()) heroLabel.textContent = "打开实验室（网页）";
    else heroLabel.textContent = "一键开始";
  }
  updateTelemetry({ mode: m });
}

async function loadTemplates() {
  try {
    const r = await api("/api/templates");
    templates = r.templates || [];
  } catch (_) {
    templates = [];
  }
  renderTemplateSelect();
}

function renderTemplateSelect() {
  const sel = $("templateSelect");
  if (!sel) return;
  if (!templates.length) {
    sel.innerHTML = `<option value="deepseek">DeepSeek</option>`;
    return;
  }
  sel.innerHTML = templates
    .map((t) => `<option value="${t.id}">${t.name}</option>`)
    .join("");
}

function selectedProfile() {
  const id = $("profileSelect")?.value;
  if (!id) return null;
  return profiles.find((p) => p.id === id) || null;
}

function templateForContext() {
  const p = selectedProfile();
  if (p) return templates.find((t) => t.id === p.template_id) || null;
  const tid = $("templateSelect")?.value;
  if (tid) return templates.find((t) => t.id === tid) || null;
  return currentTemplate();
}

function effectiveBaseURL(tpl) {
  if (!tpl) return "";
  const input = $("baseUrlInput");
  if (tpl.base_url_editable && input) {
    const v = input.value.trim();
    if (v) return v;
  }
  const p = selectedProfile();
  if (p?.base_url) return p.base_url;
  return tpl.base_url || "";
}

function updateBaseURLUI() {
  const row = $("baseUrlRow");
  const input = $("baseUrlInput");
  const hint = $("baseUrlHint");
  if (!row || !input) return;
  const tpl = templateForContext();
  if (!tpl || tpl.adapter !== "relay") {
    row.hidden = true;
    return;
  }
  row.hidden = false;
  const p = selectedProfile();
  const preset = tpl.base_url || "";
  const current = p?.base_url || preset;
  if (tpl.base_url_editable) {
    input.readOnly = false;
    input.value = current;
    input.placeholder = preset || "https://…";
    if (hint) {
      hint.textContent = preset
        ? "官方地址已预填，可改到 token 套餐 / 区域镜像 / 自建反代"
        : "自定义端点：填写地址与 key";
    }
  } else {
    input.readOnly = true;
    input.value = preset;
    if (hint) hint.textContent = "原生线路地址固定，不可编辑";
  }
}

const CAT_LABELS = { official: "官方", cn_official: "国内", custom: "自定义" };

function renderProfiles() {
  const sel = $("profileSelect");
  const list = $("profileList");
  if (!profiles.length) {
    if (sel) sel.innerHTML = `<option value="">（无配置）</option>`;
    if (list) list.innerHTML = `<p class="field-hint">暂无配置档。填写 key 后点「新建配置」，或从 CSSwitch 导入 ~/.csswitch/config.json。</p>`;
    return;
  }
  if (sel) {
    sel.innerHTML = profiles
      .map((p) => `<option value="${p.id}">${p.name}</option>`)
      .join("");
    sel.value = activeProfileId || profiles[0]?.id || "";
  }
  if (list) {
    list.innerHTML = profiles
      .map((p) => {
        const active = p.id === activeProfileId || p.active;
        const cat = CAT_LABELS[p.category] || p.category || "自定义";
        const model = p.model ? `<span class="pf-model">${p.model}</span>` : "";
        const key = p.key_masked || p.masked_key || (p.has_key ? "••••" : "未设置 key");
        return `<article class="pf-card${active ? " active" : ""}" data-id="${p.id}" role="listitem">
          <div class="pf-head">
            <strong class="pf-name">${p.name}</strong>
            ${active ? '<span class="pf-badge">当前</span>' : ""}
          </div>
          <div class="pf-meta"><span class="pf-cat">${cat}</span> · ${p.template_id || "custom"} ${model}</div>
          <div class="pf-url">${p.base_url || "默认端点"}</div>
          <div class="pf-key">${key}</div>
          <div class="pf-actions">
            <button type="button" class="btn secondary compact pf-act" data-act="switch">设为当前</button>
            <button type="button" class="btn ghost compact pf-act" data-act="probe">探测</button>
            <button type="button" class="btn ghost compact pf-act" data-act="delete">删除</button>
          </div>
        </article>`;
      })
      .join("");
    list.querySelectorAll(".pf-card").forEach((card) => {
      const id = card.dataset.id;
      card.querySelectorAll(".pf-act").forEach((btn) => {
        btn.addEventListener("click", async (e) => {
          e.stopPropagation();
          if (sel) sel.value = id;
          const act = btn.dataset.act;
          if (act === "switch") await switchProfile();
          else if (act === "probe") await probeProfile();
          else if (act === "delete") await deleteProfile(id);
        });
      });
      card.addEventListener("click", () => {
        if (sel) sel.value = id;
        updateProfileHint();
      });
    });
  }
  updateProfileHint();
}

async function deleteProfile(id) {
  if (!id || !confirm("删除此配置？")) return;
  setBusy(true);
  try {
    await api("/api/profiles?id=" + encodeURIComponent(id), { method: "DELETE" });
    await loadConfig();
    setMsg("已删除配置", "ok");
  } catch (e) {
    setMsg(String(e.message || e), "err");
  } finally {
    setBusy(false);
  }
}

function updateProfileHint() {
  const hint = $("profileHint");
  const sel = $("profileSelect");
  if (!hint || !sel) return;
  const p = profiles.find((x) => x.id === sel.value);
  if (!p) {
    hint.textContent = "命名配置档，切换前上游探活";
    return;
  }
  const parts = [p.name];
  if (p.verified) parts.push("key 已验证");
  else if (p.verified_hint) parts.push(p.verified_hint);
  if (p.model) parts.push(`model: ${p.model}`);
  if (p.base_url) parts.push(p.base_url);
  hint.textContent = parts.join(" · ");
  updateBaseURLUI();
  updateRelayModelUI();
}

async function loadConfig() {
  await loadTemplates();
  const cfg = await api("/api/config");
  $("provider").innerHTML = (cfg.providers || [])
    .map((p) => `<option value="${p.id}">${p.label}</option>`)
    .join("");
  $("provider").value = cfg.provider || "deepseek";
  $("proxyPort").value = cfg.proxy_port ?? 18991;
  $("sandboxPort").value = cfg.sandbox_port ?? 8990;
  $("cacheBoost").checked = cfg.cache_boost !== false;
  if ($("tooluseShim")) $("tooluseShim").value = cfg.tooluse_shim || "off";
  keys = cfg.keys || {};
  profiles = cfg.profiles || [];
  activeProfileId = cfg.active_profile_id || "";
  renderProfiles();
  applyMode(cfg.mode === "official" ? "official" : "proxy");
  reflectProvider();
  updateBaseURLUI();
  updateRelayModelUI();
  updateTelemetry({
    provider: cfg.provider,
    proxy_port: cfg.proxy_port,
    sandbox_port: cfg.sandbox_port,
    mode: cfg.mode,
  });
}

async function switchProfile() {
  const id = $("profileSelect")?.value;
  if (!id) { setMsg("请先新建或选择配置", "err"); return; }
  setBusy(true);
  try {
    const r = await api("/api/profiles/switch", { method: "POST", body: JSON.stringify({ id }) });
    activeProfileId = id;
    await loadConfig();
    setMsg(r.message || "已切换配置", "ok");
  } catch (e) {
    setMsg(String(e.message || e), "err");
  } finally {
    setBusy(false);
  }
}

async function probeProfile() {
  const id = $("profileSelect")?.value;
  if (!id) { setMsg("请选择配置", "err"); return; }
  setBusy(true);
  try {
    const r = await api("/api/profiles/probe", { method: "POST", body: JSON.stringify({ id }) });
    setMsg(r.ok ? `✓ ${r.hint}` : `✗ ${r.hint}`, r.ok ? "ok" : "err");
  } catch (e) {
    setMsg(String(e.message || e), "err");
  } finally {
    setBusy(false);
  }
}

async function addProfile() {
  const key = $("keyInput")?.value?.trim();
  if (!key) { setMsg("请先填写 API key", "err"); return; }
  const tpl = templateForContext() || templates[0];
  const name = prompt("配置名称", tpl?.name || "我的配置");
  if (!name) return;
  const model = $("relayModelSelect")?.value || tpl?.builtin_models?.[0] || "";
  setBusy(true);
  try {
    await api("/api/profiles", {
      method: "POST",
      body: JSON.stringify({
        name,
        template_id: tpl?.id || "deepseek",
        api_key: key,
        base_url: effectiveBaseURL(tpl),
        model,
      }),
    });
    await loadConfig();
    setMsg(`已创建配置「${name}」`, "ok");
  } catch (e) {
    setMsg(String(e.message || e), "err");
  } finally {
    setBusy(false);
  }
}

async function checkUpdate() {
  setBusy(true);
  try {
    const r = await api("/api/check-update");
    if (!r.ok) {
      setMsg(r.error || "检查更新失败", "err");
      return;
    }
    if (r.update_available) {
      setMsg(`有新版本 ${r.latest}（当前 ${r.current}）`, "ok");
      if (r.url) window.open(r.url, "_blank");
    } else {
      setMsg(`已是最新（${r.current}）`, "ok");
    }
  } catch (e) {
    if (window._releaseURL) window.open(window._releaseURL, "_blank");
    else setMsg(String(e.message || e), "err");
  } finally {
    setBusy(false);
  }
}

function reflectProvider() {
  const p = $("provider").value;
  $("keyLabel").textContent = KEY_LABELS[p] || "API Key";
  const masked = keys[p] || "";
  $("keyInput").value = "";
  $("keyInput").placeholder = masked ? `已存：${masked}` : "粘贴 key（仅存本地）";
  updateRelayModelUI();
}

async function persistSettings() {
  const proxyPort = parseInt($("proxyPort").value, 10) || 18991;
  const sandboxPort = parseInt($("sandboxPort").value, 10) || 8990;
  if (proxyPort === sandboxPort) throw new Error("代理与沙箱端口不能相同");
  if (proxyPort === 8765 || sandboxPort === 8765) throw new Error("端口 8765 保留给真实 Science 实例");
  const body = {
    provider: $("provider").value,
    proxy_port: proxyPort,
    sandbox_port: sandboxPort,
    cache_boost: $("cacheBoost").checked,
  };
  if ($("tooluseShim")) body.tooluse_shim = $("tooluseShim").value || "off";
  await api("/api/config", { method: "PUT", body: JSON.stringify(body) });
  updateTelemetry({ proxy_port: proxyPort, sandbox_port: sandboxPort });
}

function currentTemplate() {
  const p = $("provider")?.value;
  return templates.find((t) => t.adapter === p) || templates.find((t) => t.id === p) || null;
}

function isRelayTemplate(tpl) {
  return tpl && (tpl.adapter === "relay" || tpl.requires_model_override);
}

function updateRelayModelUI() {
  const tpl = templateForContext();
  const row = $("relayModelRow");
  if (!row) return;
  const show = isRelayTemplate(tpl);
  row.hidden = !show;
  const sel = $("relayModelSelect");
  if (sel && tpl?.builtin_models?.length) {
    const cur = sel.value;
    sel.innerHTML = tpl.builtin_models.map((m) => `<option value="${m}">${m}</option>`).join("");
    if (cur && [...sel.options].some((o) => o.value === cur)) sel.value = cur;
    const p = selectedProfile();
    if (p?.model && [...sel.options].some((o) => o.value === p.model)) sel.value = p.model;
  }
}

async function fetchRelayModels() {
  const key = $("keyInput")?.value?.trim();
  if (!key) { setMsg("请先填写 API key", "err"); return; }
  const tpl = templateForContext();
  const base = effectiveBaseURL(tpl);
  if (!base) { setMsg("请先填写 base_url", "err"); return; }
  setBusy(true);
  try {
    const r = await api("/api/relay/models", {
      method: "POST",
      body: JSON.stringify({ base_url: base, api_key: key }),
    });
    const sel = $("relayModelSelect");
    if (!sel) return;
    const models = r.models || [];
    if (!models.length) { setMsg("未返回模型列表", "err"); return; }
    sel.innerHTML = models.map((m) => `<option value="${m.id}">${m.display_name || m.id}</option>`).join("");
    setMsg(`已加载 ${models.length} 个模型`, "ok");
  } catch (e) {
    setMsg(String(e.message || e), "err");
  } finally {
    setBusy(false);
  }
}

async function switchMode(m) {
  if (m === mode) return;
  // Lab mode: just switch the view, no API call needed
  if (m === "lab") {
    applyMode(m);
    return;
  }
  setBusy(true);
  try {
    await api("/api/mode", { method: "PUT", body: JSON.stringify({ mode: m }) });
    applyMode(m);
    setMsg(m === "official" ? "已切换到官方模式" : "已切换到第三方模式", "ok");
  } catch (e) {
    setMsg(String(e.message || e), "err");
  } finally {
    setBusy(false);
  }
}

async function verifyKey() {
  setBusy(true);
  try {
    const v = await api("/api/verify", { method: "POST", body: "{}" });
    setMsg(v.ok ? `✓ ${v.hint}` : `✗ ${v.hint}`, v.ok ? "ok" : "err");
  } catch (e) {
    setMsg(String(e.message || e), "err");
  } finally {
    setBusy(false);
  }
}

async function saveKey() {
  const key = $("keyInput").value.trim();
  if (!key) { setMsg("请输入 API key", "err"); return; }
  setBusy(true);
  try {
    await persistSettings();
    const r = await api("/api/key", {
      method: "POST",
      body: JSON.stringify({ provider: $("provider").value, key }),
    });
    keys[$("provider").value] = r.masked;
    reflectProvider();
    await verifyKey();
  } catch (e) {
    setMsg(String(e.message || e), "err");
    setBusy(false);
  }
}

async function oneClick() {
  // Oasis public demo: no Claude Science.app on the VPS — open Lab instead.
  if (isOasisPublicHost()) {
    setMsg("正在打开实验室…", "ok");
    openLabWorkbench("embed=1"); // same-tab navigate
    return;
  }
  setBusy(true);
  setMsg("正在启动代理与沙箱…");
  try {
    if ($("keyInput").value.trim()) {
      await persistSettings();
      const key = $("keyInput").value.trim();
      const r = await api("/api/key", {
        method: "POST",
        body: JSON.stringify({ provider: $("provider").value, key }),
      });
      keys[$("provider").value] = r.masked;
      reflectProvider();
    } else {
      await persistSettings();
    }
    const r = await api("/api/start", { method: "POST", body: "{}" });
    setMsg((r.msg || "已启动") + (r.url ? `\n${r.url}` : ""), "ok");
    if (r.url) {
      applyStatus({ proxy: "green", sandbox: "green", upstream: "green", url: r.url });
      // Show prominent sandbox link (works without popup permission)
      var bar = $("urlBar");
      if (bar) {
        bar.href = r.url;
        bar.hidden = false;
        const cert = bar.querySelector(".cert-id") || bar.querySelector(".url-text");
        if (cert) cert.textContent = r.url;
      }
      // 一键跳转到沙箱 Science 会话
      window.open(r.url, "_blank");
    }
  } catch (e) {
    const msg = String(e.message || e);
    if (/Science binary not found|claude-science|未找到 Claude Science/i.test(msg)) {
      setMsg(
        "未找到 Claude Science 桌面程序，无法启动沙箱。Mac 请安装 Claude Science.app；绿洲网页请点顶栏「实验室」。",
        "err"
      );
      try {
        openLabWorkbench("embed=1");
      } catch (_) {}
    } else {
      setMsg(msg, "err");
    }
  } finally {
    setBusy(false);
  }
}

async function heroClick() {
  if (mode === "lab") {
    openLabWorkbench("embed=1");
    setMsg("已打开实验室：" + labBaseURL(), "ok");
    return;
  }
  if (mode === "official") {
    if (isOasisPublicHost()) {
      setMsg(
        "官方 Claude Science 只能在本机桌面启动。网页端请用「实验室」，或在 Mac 安装 Claude Science 后用 Lumen Science.app。",
        "err"
      );
      return;
    }
    setBusy(true);
    try {
      await api("/api/official", { method: "POST", body: "{}" });
      setMsg("已打开官方 Claude Science", "ok");
    } catch (e) {
      setMsg(String(e.message || e), "err");
    } finally {
      setBusy(false);
    }
  } else {
    await oneClick();
  }
}

async function stopAll() {
  setBusy(true);
  try {
    await api("/api/stop", { method: "POST", body: "{}" });
    setMsg("已停止沙箱与代理", "ok");
    applyStatus({ proxy: "amber", sandbox: "amber", upstream: "amber", url: "" });
    $("brandDot")?.classList.remove("live");
  } catch (e) {
    setMsg(String(e.message || e), "err");
  } finally {
    setBusy(false);
  }
}

async function runDoctor() {
  setBusy(true);
  try {
    const r = await api("/api/doctor");
    const lines = (r.results || []).map((x) => {
      const lvl = x.level || x.Level || "";
      const msg = x.message || x.Message || "";
      const icon = lvl === "pass" ? "✓" : lvl === "fail" ? "✗" : "⚠";
      return `${icon} ${msg}`;
    });
    showModal("自检", lines.join("\n") + `\n\n警告 ${r.warnings} · 失败 ${r.failures}`);
  } catch (e) {
    setMsg(String(e.message || e), "err");
  } finally {
    setBusy(false);
  }
}

async function fetchLogs(which) {
  const r = await api(`/api/logs?which=${which}`);
  return r.text || r.error || "(空)";
}

async function showLogs() {
  setBusy(true);
  modalLogWhich = "proxy";
  try {
    $("modalTabs").hidden = false;
    document.querySelectorAll("#modalTabs .tab").forEach((b) => {
      b.classList.toggle("active", b.dataset.log === "proxy");
    });
    showModal("运行日志", await fetchLogs("proxy"));
  } catch (e) {
    setMsg(String(e.message || e), "err");
  } finally {
    setBusy(false);
  }
}

async function showResearch() {
  setBusy(true);
  try {
    const v = await api("/api/research?verify=1");
    const summary = [
      v.healthy ? "✓ 资源包就绪" : "✗ 资源不完整",
      `runtime ${v.runtime_version || "—"}`,
      `db clients ${v.bio_lib_packages || 0}`,
      `domains ${v.domains || 0} · tools ${v.domain_tools || 0}`,
      `skills ${v.skills || 0}`,
      v.org_pack_seeded ? `org pack (${v.workspaces} workspaces)` : "org pack 未播种",
    ].join("\n");
    const detail = await api("/api/research");
    showModal("科研资源", summary + "\n\n" + JSON.stringify(detail, null, 2).slice(0, 6000));
  } catch (e) {
    setMsg(String(e.message || e), "err");
  } finally {
    setBusy(false);
  }
}

function showModal(title, body) {
  $("modalTitle").textContent = title;
  $("modalBody").textContent = body;
  $("modal").showModal();
}

function connectSSE() {
  if (evtSrc) evtSrc.close();
  evtSrc = new EventSource(API + "/api/events");
  evtSrc.onopen = () => { sseBackoff = 2000; setConn(true); };
  evtSrc.onmessage = (ev) => {
    try { applyStatus(JSON.parse(ev.data)); } catch (_) {}
  };
  evtSrc.onerror = () => {
    setConn(false);
    evtSrc.close();
    setTimeout(connectSSE, sseBackoff);
    sseBackoff = Math.min(sseBackoff * 1.5, 30000);
  };
}

function tickUptime() {
  if ($("telUptime")) $("telUptime").textContent = fmtUptime(Date.now() - panelStartedAt);
}

function applyOasisAuth(st) {
  const badge = $("oasisAuthBadge");
  const masked = $("oasisTokenMasked");
  const loginBtn = $("oasisLoginBtn");
  const discBtn = $("oasisDisconnectBtn");
  if (!badge) return;
  if (st?.token_set) {
    badge.className = "badge ok";
    badge.textContent = "已连接";
    if (masked) masked.textContent = st.token_masked || "token ••••";
    if (loginBtn) loginBtn.hidden = true;
    if (discBtn) discBtn.hidden = false;
  } else {
    badge.className = "badge warn";
    badge.textContent = "未连接";
    if (masked) masked.textContent = "匿名可搜数据集 · 预览需登录";
    if (loginBtn) loginBtn.hidden = false;
    if (discBtn) discBtn.hidden = true;
  }
}

async function loadOasisAuth() {
  try {
    const st = await api("/api/oasis");
    applyOasisAuth(st);
    return st;
  } catch (_) {
    return null;
  }
}

function renderNativeFleet(fleet) {
  const el = $("nativeFleetList");
  if (!el) return;
  if (!fleet?.length) {
    el.textContent = "舰队未连接";
    return;
  }
  el.innerHTML = fleet.map((m) => {
    const ok = m.connected ? "✓" : "✗";
    const tools = (m.tools || []).length;
    return `<li>${ok} <strong>${m.label || m.id}</strong> · ${tools} tools</li>`;
  }).join("");
}

async function loadNativeFleet() {
  try {
    const st = await api("/api/native/status");
    renderNativeFleet(st.fleet);
    return st;
  } catch (_) {
    renderNativeFleet([]);
    return null;
  }
}

async function saveOasisToken(accessToken) {
  if (!accessToken) return;
  const res = await api("/api/oasis/token", {
    method: "POST",
    body: JSON.stringify({ access_token: accessToken }),
  });
  applyOasisAuth({ token_set: true, token_masked: res.token_masked });
  setMsg("绿洲登录态已同步 — 可预览数据集样本", "info");
}

async function clearOasisToken() {
  await api("/api/oasis/token", { method: "DELETE" });
  applyOasisAuth({ token_set: false });
  setMsg("已断开绿洲登录", "info");
}

function requestOasisLogin() {
  if (window.parent !== window) {
    window.parent.postMessage({ type: OASIS_MSG_REQUEST }, location.origin);
    setMsg("已向绿洲请求登录态…", "info");
    return;
  }
  window.open("https://demo.oasisdata2026.xyz/login?next=/workspace/lumen-science", "_blank");
}

function wireOasisAuthBridge() {
  window.addEventListener("message", async (e) => {
    if (e.origin !== location.origin) return;
    if (e.data?.type === OASIS_MSG_AUTH) {
      if (e.data.access_token) {
        try {
          await saveOasisToken(e.data.access_token);
        } catch (err) {
          setMsg(String(err.message || err), "err");
        }
      } else {
        applyOasisAuth({ token_set: false });
      }
    }
  });
  $("oasisLoginBtn")?.addEventListener("click", requestOasisLogin);
  $("oasisDisconnectBtn")?.addEventListener("click", () =>
    clearOasisToken().catch((e) => setMsg(e.message, "err"))
  );
  $("oasisBriefBtn")?.addEventListener("click", async () => {
    const topic = ($("oasisBriefTopic")?.value || "").trim();
    if (!topic) {
      setMsg("请输入课题关键词", "err");
      return;
    }
    setBusy(true);
    setMsg("正在生成 Research Brief…", "info");
    try {
      const res = await api("/api/native/brief", {
        method: "POST",
        body: JSON.stringify({ topic, max_articles: 5, max_datasets: 5 }),
      });
      showModal("Research Brief · " + topic, res.markdown || JSON.stringify(res, null, 2));
      setMsg(`简报已生成 · ${(res.claims || []).length} 条溯源`, "info");
    } catch (e) {
      setMsg(String(e.message || e), "err");
    } finally {
      setBusy(false);
    }
  });
  $("oasisNativeVerifyBtn")?.addEventListener("click", async () => {
    setBusy(true);
    try {
      const res = await api("/api/native/verify", { method: "POST", body: "{}" });
      const lines = (res.results || []).map((r) =>
        (r.pass ? "✓ " : "✗ ") + r.fleet_id + "." + r.tool + (r.error ? " — " + r.error : "")
      );
      showModal("Native MCP", (res.ok ? "✓ PASS\n" : "✗ FAIL\n") + lines.join("\n"));
      await loadNativeFleet();
    } catch (e) {
      setMsg(String(e.message || e), "err");
    } finally {
      setBusy(false);
    }
  });
  $("oasisProbeBtn")?.addEventListener("click", async () => {
    const ds = ($("oasisDatasetId")?.value || "").trim();
    if (!ds) {
      setMsg("请输入数据集 ID", "err");
      return;
    }
    setBusy(true);
    try {
      const res = await api("/api/oasis/probe", {
        method: "POST",
        body: JSON.stringify({ dataset_id: ds }),
      });
      if (res.need_login) {
        setMsg(res.error || "需要登录绿洲", "err");
        return;
      }
      showModal("数据集预览 · " + ds, JSON.stringify(res.preview || res, null, 2));
    } catch (e) {
      setMsg(String(e.message || e), "err");
    } finally {
      setBusy(false);
    }
  });
  $("oasisC2DAlgoBtn")?.addEventListener("click", async () => {
    const ds = ($("oasisDatasetId")?.value || "").trim();
    setBusy(true);
    try {
      const res = await api("/api/native/c2d/algorithms", {
        method: "POST",
        body: JSON.stringify({ dataset_id: ds }),
      });
      if (res.need_login) {
        setMsg(res.error || "需要登录绿洲", "err");
        return;
      }
      showModal("C2D 算法" + (ds ? " · " + ds : ""), JSON.stringify(res.data || res, null, 2));
    } catch (e) {
      setMsg(String(e.message || e), "err");
    } finally {
      setBusy(false);
    }
  });
}

async function init() {
  wireOasisAuthBridge();
  try {
    const [v, health] = await Promise.all([api("/api/version"), api("/api/health")]);
    if (null) null.textContent = "v" + (v.version || "dev");
    window._releaseURL = v.release;
    window._issuesURL = v.issues;
    if (typeof health.uptime_ms === "number") panelStartedAt = Date.now() - health.uptime_ms;
    updateTelemetry({
      provider: health.provider,
      mode: health.mode,
      proxy_port: health.proxy_port,
      sandbox_port: health.sandbox_port,
      csswitch_parity: health.csswitch_parity,
      gateway: health.gateway,
    });
    if (health.proxy || health.sandbox) $("brandDot")?.classList.add("live");
  } catch (_) {}
  await loadConfig();
  await loadOasisAuth();
  await loadNativeFleet();
  await probeLabHealth();
  if (document.body.classList.contains("embed-oasis") && window.parent !== window) {
    window.parent.postMessage({ type: OASIS_MSG_REQUEST }, location.origin);
  }
  connectSSE();
  tickUptime();
  setInterval(tickUptime, 1000);
}

document.querySelectorAll(".tabs .tab").forEach((b) => {
  b.addEventListener("click", () => switchMode(b.dataset.mode));
});
$("provider")?.addEventListener("change", () => {
  reflectProvider();
  updateBaseURLUI();
  updateRelayModelUI();
  persistSettings().catch((e) => setMsg(e.message, "err"));
});
$("templateSelect")?.addEventListener("change", () => {
  updateBaseURLUI();
  updateRelayModelUI();
});
$("baseUrlInput")?.addEventListener("change", async () => {
  const p = selectedProfile();
  const tpl = templateForContext();
  if (!p || !tpl?.base_url_editable) return;
  const base = $("baseUrlInput")?.value?.trim();
  if (!base) return;
  try {
    await api("/api/profiles", {
      method: "PUT",
      body: JSON.stringify({ id: p.id, base_url: base }),
    });
    await loadConfig();
    setMsg("已保存 base_url", "ok");
  } catch (e) {
    setMsg(String(e.message || e), "err");
  }
});
$("proxyPort")?.addEventListener("change", () => persistSettings().catch((e) => setMsg(e.message, "err")));
$("sandboxPort")?.addEventListener("change", () => persistSettings().catch((e) => setMsg(e.message, "err")));
$("cacheBoost")?.addEventListener("change", () => persistSettings().catch((e) => setMsg(e.message, "err")));
$("tooluseShim")?.addEventListener("change", () => persistSettings().catch((e) => setMsg(e.message, "err")));
$("fetchRelayModelsBtn")?.addEventListener("click", fetchRelayModels);
$("saveKeyBtn")?.addEventListener("click", saveKey);
$("verifyKeyBtn")?.addEventListener("click", verifyKey);
$("heroBtn")?.addEventListener("click", heroClick);
$("stopBtn")?.addEventListener("click", stopAll);
$("quitBtn")?.addEventListener("click", async () => {
  try { await api("/api/quit-proxy", { method: "POST", body: "{}" }); } catch (_) {}
  window.close();
});
$("openBrowserBtn")?.addEventListener("click", async () => {
  if (isOasisPublicHost()) {
    const u = openLabWorkbench("embed=1");
    setMsg("网页端已打开实验室（Claude Science 桌面仅本机可用）：" + u, "ok");
    applyMode("lab");
    return;
  }
  try {
    const r = await api("/api/open-browser", { method: "POST", body: "{}" });
    setMsg("已在浏览器打开 " + (r.url || ""), "ok");
  } catch (e) {
    setMsg(String(e.message || e), "err");
  }
});
$("doctorBtn")?.addEventListener("click", runDoctor);
$("logsBtn")?.addEventListener("click", showLogs);
$("researchBtn")?.addEventListener("click", showResearch);
$("switchProfileBtn")?.addEventListener("click", switchProfile);
$("probeProfileBtn")?.addEventListener("click", probeProfile);
$("addProfileBtn")?.addEventListener("click", addProfile);
$("profileSelect")?.addEventListener("change", updateProfileHint);
$("updateBtn")?.addEventListener("click", checkUpdate);
$("reportBtn")?.addEventListener("click", () => { if (window._issuesURL) window.open(window._issuesURL, "_blank"); });
$("modalClose")?.addEventListener("click", () => $("modal").close());

// Oasis integration (added to embed into https://demo.oasisdata2026.xyz)
function wireOasisPublish(btn) {
  if (!btn) return;
  btn.addEventListener("click", async () => {
    setBusy(true);
    try {
      await api("/api/science/config").catch(() => ({}));
      setMsg("准备发布到绿洲：打开 C2D 货架。终端可运行 lumen oasis publish --name lumen-science-bridge", "info");
      window.open("https://demo.oasisdata2026.xyz/c2d", "_blank");
    } catch (e) {
      setMsg("请在终端运行: lumen oasis publish --name lumen-science-bridge", "info");
    } finally {
      setBusy(false);
    }
  });
}
wireOasisPublish($("oasisPublishBtn"));
wireOasisPublish($("oasisPublishFootBtn"));

function wireOasisResearch(btn) {
  if (!btn) return;
  btn.addEventListener("click", () => {
    window.open("https://demo.oasisdata2026.xyz/c2d", "_blank");
  });
}
wireOasisResearch($("oasisResearchBtn"));
wireOasisResearch($("oasisResearchFootBtn"));

async function probeLabHealth() {
  const dot = $("labDot");
  const link = $("labLink");
  const tab = $("labTab");
  if (link) {
    link.href = labBaseURL();
    if (isOasisPublicHost()) link.removeAttribute("target");
  }
  try {
    const r = await fetch(labHealthURL(), { signal: AbortSignal.timeout(2500) });
    if (!r.ok) throw new Error("lab down");
    const h = await r.json();
    const healthy = h.research_pack?.healthy;
    if (dot) {
      dot.textContent = healthy ? "✓" : "○";
      dot.title = healthy ? "Lab + research pack ready" : "Lab up, pack incomplete";
    }
    if (tab && healthy) {
      const f = h.fleet || {};
      tab.textContent = "实验室 (" + (f.connected_total || 0) + ")";
      tab.title = (f.cs_connected || 0) + " CS + " + (f.lumen_native || 0) + " 原生 · " + (h.research_pack?.domain_tools || 0) + " tools";
    }
    const smode = h.science_mode || "hybrid";
    if (link && smode === "bridge") link.style.opacity = "0.55";
  } catch (_) {
    if (dot) {
      dot.textContent = "—";
      dot.title = isOasisPublicHost()
        ? "实验室探测失败（可点「实验室」标签直接打开）"
        : "实验室未启动 (lumen science lab)";
    }
    if (tab) {
      tab.textContent = "实验室";
      tab.title = isOasisPublicHost() ? "打开网页实验室" : "实验室未启动";
    }
  }
}

// Embed mode for Oasis site (add ?embed=1 or ?oasis=1 to URL)
const p = new URLSearchParams(location.search);
if (p.get("embed") || p.get("oasis") || p.get("theme") === "oasis") {
  document.body.classList.add("embed-oasis");
  const oasisLink = document.querySelector(".btn-oasis");
  if (oasisLink) {
    oasisLink.href = "https://demo.oasisdata2026.xyz/";
    oasisLink.removeAttribute("target");
  }
  const oasisCard = document.querySelector(".oasis-card");
  if (oasisCard) oasisCard.style.display = "none";
  // Publish/research remain reachable via footer buttons in embed mode.
}
document.querySelectorAll("#modalTabs .tab").forEach((btn) => {
  btn.addEventListener("click", async () => {
    modalLogWhich = btn.dataset.log;
    document.querySelectorAll("#modalTabs .tab").forEach((b) => {
      b.classList.toggle("active", b.dataset.log === modalLogWhich);
    });
    $("modalBody").textContent = "加载中…";
    try {
      $("modalBody").textContent = await fetchLogs(modalLogWhich);
    } catch (e) {
      $("modalBody").textContent = String(e.message || e);
    }
  });
});

init().catch((e) => setMsg(String(e.message || e), "err"));