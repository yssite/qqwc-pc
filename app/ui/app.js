/* 全球网测 桌面版 前端逻辑 */
const TAURI = window.__TAURI__ || {};
const invoke = TAURI.core ? TAURI.core.invoke : async () => { throw new Error("Tauri API 不可用"); };
const listen = TAURI.event ? TAURI.event.listen : async () => () => {};

const $ = (id) => document.getElementById(id);

const state = {
  settings: null,
  nodes: [],
  history: [],
  selectedNode: null,
  running: false,
  samples: { down: [], up: [] },
  ping: null,
  cur: { down: 0, up: 0 },
  lat: {},
  sortBy: "default",
  probing: false,
  probeGen: 0,
  localIp: "",
  publicIp: "",
  ipInfo: null,
  dataDir: "",
};

/* ---------------- 通用 ---------------- */
function switchView(name) {
  document.querySelectorAll(".tab").forEach((t) => t.classList.toggle("active", t.dataset.view === name));
  document.querySelectorAll(".view").forEach((v) => v.classList.toggle("active", v.id === "view-" + name));
  if (name === "history") renderHistory();
  if (name === "nodes") renderNodes();
}

document.querySelectorAll(".tab").forEach((t) => t.addEventListener("click", () => switchView(t.dataset.view)));

function fmt(v, digits = 2) {
  if (v === null || v === undefined || Number.isNaN(v)) return "--";
  return Number(v).toFixed(digits);
}
function fmtSpeed(v) {
  if (!v || v < 0) return "--";
  if (v >= 1000) return (v / 1000).toFixed(2) + " Gbps";
  return v.toFixed(2) + " Mbps";
}

/* ---------------- 仪表盘 ---------------- */
const GAUGE_MARKS = [0, 1, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000];
const gCanvas = $("gauge");
const gctx = gCanvas.getContext("2d");

const live = { down: 0, up: 0 };          // 目标值（每 500ms 更新）
let finalValues = null;                    // 测速结束后的最终平均值（顶部卡片保留显示）
const disp = { down: 0, up: 0 };          // 显示值（每帧插值）
let activeDir = "down";
let needleValue = 0;                       // 仪表盘当前显示值
let needleTarget = 0;
let lastFrame = 0;
let domAcc = 0;

function markAngle(i) {
  const start = (150 * Math.PI) / 180;
  const sweep = (240 * Math.PI) / 180;
  return start + (sweep * i) / (GAUGE_MARKS.length - 1);
}

function valueToAngle(v) {
  v = Math.max(0, v);
  for (let i = 0; i < GAUGE_MARKS.length - 1; i++) {
    const a = GAUGE_MARKS[i];
    const b = GAUGE_MARKS[i + 1];
    if (v <= b) {
      const t = (v - a) / (b - a);
      return markAngle(i) + (markAngle(i + 1) - markAngle(i)) * Math.max(0, Math.min(1, t));
    }
  }
  return markAngle(GAUGE_MARKS.length - 1);
}

function drawGauge(value) {
  const W = gCanvas.width;
  const H = gCanvas.height;
  const cx = W / 2;
  const cy = H * 0.66;
  const R = Math.min(W * 0.40, H * 0.50);   // 半径按画布实际尺寸取，保证两端不被裁切
  const scale = R / 150;
  gctx.clearRect(0, 0, W, H);

  const lw = Math.max(10, 22 * scale);
  gctx.lineCap = "round";
  gctx.lineWidth = lw;
  gctx.strokeStyle = "rgba(28,48,76,.95)";
  gctx.beginPath();
  gctx.arc(cx, cy, R, markAngle(0), markAngle(GAUGE_MARKS.length - 1));
  gctx.stroke();

  const angle = valueToAngle(value);
  if (value > 0.01) {
    const grad = gctx.createLinearGradient(cx - R, cy - R, cx + R, cy + R);
    grad.addColorStop(0, "#3ac9e8");
    grad.addColorStop(1, "#7ef0c8");
    gctx.strokeStyle = grad;
    gctx.shadowColor = "rgba(53,200,232,.55)";
    gctx.shadowBlur = 14 * scale;
    gctx.beginPath();
    gctx.arc(cx, cy, R, markAngle(0), angle);
    gctx.stroke();
    gctx.shadowBlur = 0;
  }

  for (let i = 0; i < GAUGE_MARKS.length; i++) {
    const a = markAngle(i);
    gctx.strokeStyle = "rgba(150,180,215,.45)";
    gctx.lineWidth = Math.max(1, 2 * scale);
    gctx.beginPath();
    gctx.moveTo(cx + Math.cos(a) * (R - 10 * scale), cy + Math.sin(a) * (R - 10 * scale));
    gctx.lineTo(cx + Math.cos(a) * (R + 12 * scale), cy + Math.sin(a) * (R + 12 * scale));
    gctx.stroke();

    const lx = cx + Math.cos(a) * (R - 34 * scale);
    const ly = cy + Math.sin(a) * (R - 34 * scale);
    gctx.fillStyle = "rgba(190,212,238,.85)";
    gctx.font = `${Math.round(12 * scale)}px 'Microsoft YaHei UI', sans-serif`;
    gctx.textAlign = "center";
    gctx.textBaseline = "middle";
    const label = GAUGE_MARKS[i] >= 1000 ? GAUGE_MARKS[i] / 1000 + "G" : String(GAUGE_MARKS[i]);
    gctx.fillText(label, lx, ly);
  }

  const na = valueToAngle(value);
  gctx.strokeStyle = "rgba(232,240,250,.92)";
  gctx.lineWidth = Math.max(2, 4.5 * scale);
  gctx.lineCap = "round";
  gctx.beginPath();
  gctx.moveTo(cx, cy);
  gctx.lineTo(cx + Math.cos(na) * (R - 54 * scale), cy + Math.sin(na) * (R - 54 * scale));
  gctx.stroke();
  gctx.fillStyle = "rgba(232,240,250,.95)";
  gctx.beginPath();
  gctx.arc(cx, cy, Math.max(4, 8 * scale), 0, Math.PI * 2);
  gctx.fill();

  gctx.textAlign = "center";
  gctx.fillStyle = "#eaf3ff";
  gctx.font = `600 ${Math.round(32 * scale)}px 'Microsoft YaHei UI', sans-serif`;
  const main = value < 0.01 ? "--" : value >= 1000 ? (value / 1000).toFixed(2) : value.toFixed(2);
  gctx.fillText(main, cx, cy - 84 * scale);
  gctx.fillStyle = "rgba(150,178,210,.95)";
  gctx.font = `${Math.round(12 * scale)}px 'Microsoft YaHei UI', sans-serif`;
  gctx.fillText(value >= 1000 ? "Gbps" : "Mbps", cx, cy - 62 * scale);
}

function setLive(dir, speed, switchDir = true) {
  live[dir] = Math.max(0, speed || 0);
  if (switchDir) activeDir = dir;
}

function refreshLiveDom() {
  const dv = finalValues ? finalValues.down : disp.down;
  const uv = finalValues ? finalValues.up : disp.up;
  $("mDown").innerHTML = `${dv.toFixed(2)}<i>Mbps</i>`;
  $("mUp").innerHTML = `${uv.toFixed(2)}<i>Mbps</i>`;
  $("liveDown").textContent = fmtSpeed(dv);
  $("liveUp").textContent = fmtSpeed(uv);
}

function animate(ts) {
  const dt = Math.min(0.06, lastFrame ? (ts - lastFrame) / 1000 : 0.016);
  lastFrame = ts;
  // 指数平滑：时间常数 0.22s，指针/数字连续过渡，不再半秒一跳
  const k = 1 - Math.exp(-dt / 0.22);
  disp.down += (live.down - disp.down) * k;
  disp.up += (live.up - disp.up) * k;
  if (Math.abs(live.down - disp.down) < 0.01) disp.down = live.down;
  if (Math.abs(live.up - disp.up) < 0.01) disp.up = live.up;

  const target = activeDir === "up" ? disp.up : disp.down;
  needleTarget = target;
  needleValue += (needleTarget - needleValue) * Math.min(1, k * 1.15);
  if (Math.abs(needleTarget - needleValue) < 0.01) needleValue = needleTarget;

  drawGauge(needleValue);

  domAcc += dt;
  if (domAcc > 0.09) {
    domAcc = 0;
    refreshLiveDom();
  }
  requestAnimationFrame(animate);
}

/* ---------------- 曲线 ---------------- */
function drawChart(canvas, data, color) {
  const ctx = canvas.getContext("2d");
  const W = canvas.width;
  const H = canvas.height;
  ctx.clearRect(0, 0, W, H);

  // 网格
  ctx.strokeStyle = "rgba(35,52,80,.85)";
  ctx.lineWidth = 1;
  for (let i = 0; i <= 4; i++) {
    const y = (H - 10) * (i / 4) + 5;
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(W, y);
    ctx.stroke();
  }

  if (!data.length) return;
  const maxV = Math.max(1, ...data) * 1.12;
  const stepX = data.length > 1 ? W / (data.length - 1) : W;
  const yOf = (v) => H - 8 - (v / maxV) * (H - 20);
  const xOf = (i) => Math.max(0, Math.min(W, i * stepX));

  // 面积
  const grad = ctx.createLinearGradient(0, 0, 0, H);
  grad.addColorStop(0, color + "55");
  grad.addColorStop(1, color + "00");
  ctx.beginPath();
  ctx.moveTo(0, H);
  data.forEach((v, i) => ctx.lineTo(xOf(i), yOf(v)));
  ctx.lineTo(xOf(data.length - 1), H);
  ctx.closePath();
  ctx.fillStyle = grad;
  ctx.fill();

  // 折线
  ctx.beginPath();
  data.forEach((v, i) => (i === 0 ? ctx.moveTo(xOf(i), yOf(v)) : ctx.lineTo(xOf(i), yOf(v))));
  ctx.strokeStyle = color;
  ctx.lineWidth = 2;
  ctx.lineJoin = "round";
  ctx.stroke();

  // 末端点
  const lastX = xOf(data.length - 1);
  const lastY = yOf(data[data.length - 1]);
  ctx.beginPath();
  ctx.arc(lastX, lastY, 3.5, 0, Math.PI * 2);
  ctx.fillStyle = color;
  ctx.fill();
}

/* ---------------- 节点库 ---------------- */
function nodeKey(n) {
  return n.hostip + "|" + n.port;
}

function renderNodes() {
  const q = ($("nodeSearch").value || "").trim().toLowerCase();
  const prov = $("provinceFilter").value;
  const oper = $("operFilter").value;

  const list = state.nodes.filter((n) => {
    if (prov && n.pname !== prov) return false;
    if (oper && n.oper !== oper) return false;
    if (q) {
      const hay = (n.hostname + n.pname + n.location + n.oper + n.hostip + n.port).toLowerCase();
      if (!hay.includes(q)) return false;
    }
    return true;
  });

  if (state.sortBy !== "default") {
    const dir = state.sortBy === "asc" ? 1 : -1;
    list.sort((a, b) => {
      const va = state.lat[nodeKey(a)];
      const vb = state.lat[nodeKey(b)];
      const na = va === undefined ? null : va;
      const nb = vb === undefined ? null : vb;
      if (na === null && nb === null) return 0;
      if (na === null) return 1;              // 未测的排最后
      if (nb === null) return -1;
      const sa = na <= 0 ? 99999 : na;        // 超时排最后
      const sb = nb <= 0 ? 99999 : nb;
      return (sa - sb) * dir;
    });
  }

  $("nodesHint").textContent = `共 ${list.length} / ${state.nodes.length} 个节点 · 点击任意行立即以其测速`;

  const box = $("nodeList");
  box.innerHTML = "";
  const frag = document.createDocumentFragment();
  list.slice(0, 600).forEach((n) => {
    const row = document.createElement("div");
    row.className = "node-row" + (state.selectedNode && nodeKey(state.selectedNode) === nodeKey(n) ? " selected" : "");
    row.innerHTML = `
      <span class="name">${escapeHtml(n.hostname || "未知节点")}</span>
      <span class="dim">${escapeHtml(n.hostip)}:${escapeHtml(n.port)}</span>
      <span><i class="badge ${escapeHtml(n.oper)}">${escapeHtml(n.oper || "未知")}</i></span>
      <span class="dim">${escapeHtml(n.location || n.pname)}</span>
      <span class="lat" data-k="${nodeKey(n)}">${latText(state.lat[nodeKey(n)])}</span>`;
    row.addEventListener("click", () => {
      state.selectedNode = n;
      $("serverName").textContent = n.hostname;
      $("nodeInfo").textContent = `${n.hostname} · ${n.hostip}:${n.port}`;
      switchView("test");
      startTest();
    });
    frag.appendChild(row);
  });
  box.appendChild(frag);
}

function latText(v) {
  if (v === undefined) return "—";
  if (v <= 0) return "超时";
  return v.toFixed(1) + " ms";
}

function latClass(v) {
  if (v === undefined) return "lat";
  if (v <= 0) return "lat dead";
  return v > 120 ? "lat bad" : "lat";
}

function escapeHtml(s) {
  return String(s == null ? "" : s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
}

function fillProvinceFilter() {
  const set = new Set(state.nodes.map((n) => n.pname).filter(Boolean));
  const sel = $("provinceFilter");
  const cur = sel.value;
  sel.innerHTML = `<option value="">全部省份</option>` + [...set].sort().map((p) => `<option>${escapeHtml(p)}</option>`).join("");
  sel.value = cur;
}

/* ---------------- 历史 ---------------- */
function renderHistory() {
  const box = $("histList");
  box.innerHTML = "";
  $("histHint").textContent = `共 ${state.history.length} 条记录`;
  if (!state.history.length) {
    box.innerHTML = `<div style="padding:30px;color:var(--muted)">暂无记录，先跑一次测速吧。</div>`;
    return;
  }
  const frag = document.createDocumentFragment();
  state.history.forEach((r) => {
    const row = document.createElement("div");
    row.className = "hist-row";
    row.innerHTML = `
      <span class="dim">${escapeHtml(r.time_text)}</span>
      <span class="name">${escapeHtml(r.node.hostname)}</span>
      <span class="lat">↓ ${fmt(r.down.avg)}</span>
      <span class="lat" style="color:var(--accent2)">↑ ${fmt(r.up.avg)}</span>
      <span>${fmt(r.ping.avg, 1)} ms</span>
      <span>${r.score}</span>
      <span><button class="btn-danger" style="padding:4px 10px;font-size:12px">删除</button></span>`;
    row.querySelector("button").addEventListener("click", async (e) => {
      e.stopPropagation();
      state.history = await invoke("history_delete", { id: r.id });
      renderHistory();
    });
    row.addEventListener("click", () => showDetail(r));
    frag.appendChild(row);
  });
  box.appendChild(frag);
}

function showDetail(r) {
  const d = r.down.samples || [];
  const u = r.up.samples || [];
  state.samples.down = d;
  state.samples.up = u;
  drawChart($("chartDown"), d, "#35c8e8");
  drawChart($("chartUp"), u, "#57d9a3");
  $("rMaxDown").textContent = fmt(r.down.max) + " Mbps";
  $("rAvgDown").textContent = fmt(r.down.avg) + " Mbps";
  $("rMaxUp").textContent = fmt(r.up.max) + " Mbps";
  $("rAvgUp").textContent = fmt(r.up.avg) + " Mbps";
  $("rJitter").textContent = fmt(r.ping.jitter, 1) + " ms";
  $("rLoss").textContent = fmt(r.ping.loss, 0) + " %";
  $("rScore").textContent = `${r.score} · ${r.grade}`;
  $("rUuid").textContent = r.uuid;
  $("mPing").innerHTML = `${fmt(r.ping.avg, 1)}<i>ms</i>`;
  finalValues = { down: r.down.avg, up: r.up.avg };   // 顶部卡片显示该条记录的平均值
  live.down = 0; live.up = 0; needleTarget = 0;        // 指针归零（被遮罩挡住）
  state.lastRecord = r;
  $("histStamp").textContent = r.time_text;
  $("gaugeMask").classList.add("show");
  $("serverName").textContent = r.node.hostname;
  $("nodeInfo").textContent = `${r.node.hostname} · ${r.node.hostip}:${r.node.port}`;
  switchView("test");
}

/* ---------------- 测速流程 ---------------- */
function resetLive() {
  state.samples.down = [];
  state.samples.up = [];
  live.down = 0; live.up = 0; disp.down = 0; disp.up = 0;
  needleValue = 0; needleTarget = 0; activeDir = "down";
  finalValues = null;
  state.cur = { down: 0, up: 0 };
  needleTarget = 0;
  ["mPing", "mDown", "mUp"].forEach((id) => ($(id).innerHTML = `--<i>${id === "mPing" ? "ms" : "Mbps"}</i>`));
  ["rMaxDown", "rAvgDown", "rMaxUp", "rAvgUp", "rJitter", "rLoss", "rScore", "rUuid"].forEach((id) => ($(id).textContent = "--"));
  $("liveDown").textContent = "0.00 Mbps";
  $("liveUp").textContent = "0.00 Mbps";
  drawChart($("chartDown"), [], "#35c8e8");
  drawChart($("chartUp"), [], "#57d9a3");
}

async function startTest() {
  if (state.running) return;
  $("gaugeMask").classList.remove("show");
  resetLive();
  state.running = true;
  $("btnStart").textContent = "停止";
  $("btnStart").classList.add("stop");
  $("phaseText").textContent = "正在连接…";
  $("statusText").textContent = "测速中";

  try {
    await invoke("start_test", {
      req: {
        node: state.selectedNode,
        ip: state.publicIp || state.localIp || "",
        skip_upload: false,
        skip_ping: false,
      },
    });
  } catch (e) {
    state.running = false;
    $("btnStart").textContent = "开始测试";
    $("btnStart").classList.remove("stop");
    $("phaseText").textContent = "启动失败：" + e;
  }
}

async function stopTest() {
  await invoke("stop_test");
  $("phaseText").textContent = "正在停止…";
}

$("btnStart").addEventListener("click", () => {
  if (state.running) stopTest();
  else startTest();
});

/* ---------------- 事件 ---------------- */
(async function wireEvents() {
  await listen("phase", (ev) => {
    const p = ev.payload || {};
    const map = {
      match: "正在匹配测速节点…",
      ping: "正在测量时延…",
      download: "正在测试下行速率…",
      upload: "正在测试上行速率…",
      canceled: "已取消",
    };
    let text = map[p.phase] || p.phase;
    if (p.note) text += "  " + p.note;
    $("phaseText").textContent = text;
  });

  await listen("node", (ev) => {
    const n = ev.payload.node;
    if (!n) return;
    $("serverName").textContent = n.hostname;
    $("nodeInfo").textContent = `${n.hostname} · ${n.hostip}:${n.port}`;
  });

  await listen("ping", (ev) => {
    const { ping, dns_ms } = ev.payload;
    state.ping = ping;
    $("mPing").innerHTML = `${fmt(ping.avg, 1)}<i>ms</i>`;
    $("rJitter").textContent = fmt(ping.jitter, 1) + " ms";
    $("rLoss").textContent = fmt(ping.loss, 0) + " %";
    if (dns_ms > 0) $("phaseText").textContent = `解析 ${fmt(dns_ms, 1)} ms · 建连 ${fmt(ping.avg, 1)} ms`;
  });

  await listen("sample", (ev) => {
    const { dir, data } = ev.payload;
    if (dir === "down") {
      state.samples.down.push(data.speed);
      if (state.samples.down.length > 80) state.samples.down.shift();
      state.cur.down = data.speed;
      setLive("down", data.speed);
      drawChart($("chartDown"), state.samples.down, "#35c8e8");
    } else {
      state.samples.up.push(data.speed);
      if (state.samples.up.length > 80) state.samples.up.shift();
      state.cur.up = data.speed;
      setLive("up", data.speed);
      drawChart($("chartUp"), state.samples.up, "#57d9a3");
    }
  });

  await listen("phase-result", (ev) => {
    const p = ev.payload;
    if (p.dir === "down") {
      $("rAvgDown").textContent = fmtSpeed(p.avg);
      $("rMaxDown").textContent = fmtSpeed(p.max);
      setLive("down", p.avg);           // 阶段结束改为显示加权平均
      activeDir = "down";
    } else {
      $("rAvgUp").textContent = fmtSpeed(p.avg);
      $("rMaxUp").textContent = fmtSpeed(p.max);
      setLive("up", p.avg);
      activeDir = "up";
    }
  });

  await listen("done", async (ev) => {
    const p = ev.payload;
    state.running = false;
    $("btnStart").textContent = "开始测试";
    $("btnStart").classList.remove("stop");
    $("phaseText").textContent = `完成 · ${p.grade} · 评分 ${p.score}`;
    $("statusText").textContent = "完成";
    $("rScore").textContent = `${p.score} · ${p.grade}`;
    $("rUuid").textContent = p.uuid;
    $("mPing").innerHTML = `${fmt(p.ping.avg, 1)}<i>ms</i>`;
    finalValues = { down: p.down.avg, up: p.up.avg };
    live.down = 0; live.up = 0;       // 指针归零
    needleTarget = 0;
    state.lastRecord = p.record;
    state.history = await invoke("history_list");
    scheduleAuto();
  });

  await listen("test-error", (ev) => {
    state.running = false;
    $("btnStart").textContent = "开始测试";
    $("btnStart").classList.remove("stop");
    $("phaseText").textContent = "失败：" + (ev.payload.message || "未知错误");
    $("statusText").textContent = "失败";
  });
})();

/* ---------------- 结果长图 ---------------- */
function renderResultCard(r) {
  const W = 900, H = 1240;
  const c = document.createElement("canvas");
  c.width = W; c.height = H;
  const x = c.getContext("2d");
  const bg = x.createLinearGradient(0, 0, 0, H);
  bg.addColorStop(0, "#0c1626");
  bg.addColorStop(1, "#111f36");
  x.fillStyle = bg;
  x.fillRect(0, 0, W, H);
  x.fillStyle = "#3ac9e8";
  x.fillRect(0, 0, W, 6);

  x.textAlign = "left";
  x.fillStyle = "#8fb3d9";
  x.font = "600 24px 'Microsoft YaHei UI', sans-serif";
  x.fillText("全球网测 · 测速结果", 64, 96);
  x.fillStyle = "#5f7fa6";
  x.font = "18px 'Microsoft YaHei UI', sans-serif";
  x.fillText(r.time_text, 64, 132);

  x.fillStyle = "#e8eef8";
  x.font = "600 34px 'Microsoft YaHei UI', sans-serif";
  x.fillText(r.node.hostname, 64, 206);
  x.fillStyle = "#7f9cbf";
  x.font = "18px 'Microsoft YaHei UI', sans-serif";
  x.fillText(`${r.node.hostip}:${r.node.port}  ·  ${r.node.pname}${r.node.oper}`, 64, 242);

  const big = (label, value, unit, color, cx) => {
    x.textAlign = "center";
    x.fillStyle = "#6f8db3";
    x.font = "18px 'Microsoft YaHei UI', sans-serif";
    x.fillText(label, cx, 340);
    x.fillStyle = color;
    x.font = "700 62px 'Microsoft YaHei UI', sans-serif";
    x.fillText(value, cx, 412);
    x.fillStyle = "#6f8db3";
    x.font = "18px 'Microsoft YaHei UI', sans-serif";
    x.fillText(unit, cx, 446);
    x.textAlign = "left";
  };
  big("下行速率", fmt(r.down.avg), "Mbps", "#3ac9e8", 220);
  big("Ping 时延", fmt(r.ping.avg, 1), "ms", "#e8eef8", 450);
  big("上行速率", fmt(r.up.avg), "Mbps", "#57d9a3", 680);

  x.strokeStyle = "rgba(120,160,210,.18)";
  x.beginPath();
  x.moveTo(64, 500);
  x.lineTo(W - 64, 500);
  x.stroke();

  const uuid = String(r.uuid || "");
  const uuidShort = uuid.length > 20 ? uuid.slice(0, 8) + "…" + uuid.slice(-6) : uuid;
  const rows = [
    ["峰值下行", fmt(r.down.max) + " Mbps"],
    ["峰值上行", fmt(r.up.max) + " Mbps"],
    ["抖动", fmt(r.ping.jitter, 1) + " ms"],
    ["丢包", fmt(r.ping.loss, 0) + " %"],
    ["评分", `${r.score} · ${r.grade}`],
    ["会话", uuidShort],
  ];
  const top = 560, rowH = 82;
  rows.forEach(([k, v], i) => {
    const bx = i % 2 === 0 ? 64 : W / 2 + 10;
    const y = top + Math.floor(i / 2) * rowH;
    x.fillStyle = "#6f8db3";
    x.font = "17px 'Microsoft YaHei UI', sans-serif";
    x.fillText(k, bx, y);
    x.fillStyle = "#dce9fb";
    x.font = "600 26px 'Microsoft YaHei UI', sans-serif";
    x.fillText(v, bx, y + 34);
  });

  const drawSpark = (data, top_, color) => {
    if (!data || data.length < 2) return;
    const max = Math.max(...data) * 1.12 || 1;
    const w = W - 128, h = 96;
    x.beginPath();
    data.forEach((v, i) => {
      const px = 64 + (w * i) / (data.length - 1);
      const py = top_ + h - (v / max) * h;
      i === 0 ? x.moveTo(px, py) : x.lineTo(px, py);
    });
    x.strokeStyle = color;
    x.lineWidth = 3;
    x.lineJoin = "round";
    x.stroke();
    x.lineWidth = 1;
  };
  x.fillStyle = "#4f6a8d";
  x.font = "15px 'Microsoft YaHei UI', sans-serif";
  x.fillText("下行速率采样", 64, 866);
  drawSpark(r.down.samples, 882, "#3ac9e8");
  x.fillText("上行速率采样", 64, 1030);
  drawSpark(r.up.samples, 1046, "#57d9a3");

  x.fillStyle = "#44608a";
  x.font = "15px 'Microsoft YaHei UI', sans-serif";
  x.fillText("全球网测 桌面版 · CAICT 公共测速节点", 64, 1200);
  x.textAlign = "right";
  x.fillText(r.time_text, W - 64, 1200);
  x.textAlign = "left";
  return c;
}

$("btnSaveImg").addEventListener("click", async () => {
  const r = state.lastRecord;
  if (!r) { alert("先跑一次测速，或点开一条历史记录"); return; }
  try {
    const dataUrl = renderResultCard(r).toDataURL("image/png");
    const name = "speedtest_" + String(r.time_text).replace(/[^0-9]/g, "");
    const p = await invoke("save_result_image", { dataUrl, name });
    $("statusText").textContent = "结果图已保存";
    $("phaseText").textContent = "已保存并打开：" + p;
    await invoke("open_file", { path: p });   // 保存后自动用默认看图程序打开
  } catch (e) {
    alert("保存失败：" + e);
  }
});

/* ---------------- 诊断 ---------------- */
$("btnPing").addEventListener("click", async () => {
  const host = $("pingHost").value.trim() || "www.baidu.com";
  const port = parseInt($("pingPort").value) || 80;
  const count = parseInt($("pingCount").value) || 10;
  $("pingOut").textContent = "正在测试…";
  const st = await invoke("diag_ping", { host, port, count });
  const lines = [`目标: ${host}:${port}`];
  (st.samples || []).forEach((v, i) => lines.push(`第 ${i + 1} 次: ${v.toFixed(2)} ms`));
  lines.push(`平均 ${fmt(st.avg, 2)} ms  最小 ${fmt(st.min, 2)}  最大 ${fmt(st.max, 2)}`);
  lines.push(`抖动 ${fmt(st.jitter, 2)} ms  丢包 ${fmt(st.loss, 0)} %`);
  $("pingOut").textContent = lines.join("\n");
});

$("btnDns").addEventListener("click", async () => {
  const host = $("dnsHost").value.trim();
  $("dnsOut").textContent = "解析中…";
  const r = await invoke("diag_dns", { host });
  $("dnsOut").textContent = `域名: ${r.host}\n耗时: ${r.ms} ms\n结果:\n` + (r.addrs || []).join("\n");
});

$("btnTrace").addEventListener("click", async () => {
  const host = $("trHost").value.trim();
  $("trOut").textContent = "追踪中（最长约 30 秒）…";
  const out = await invoke("diag_traceroute", { host });
  $("trOut").textContent = out;
});

/* ---------------- 设置 ---------------- */
function provinces() {
  return [...new Set(state.nodes.map((n) => n.pname).filter(Boolean))]
    .sort((a, b) => a.localeCompare(b, "zh"));
}

function citiesOf(prov) {
  const set = new Set();
  state.nodes
    .filter((n) => n.pname === prov)
    .forEach((n) => {
      const c = n.location || n.pname;
      if (c) set.add(c);
    });
  return [...set].sort((a, b) => a.localeCompare(b, "zh"));
}

function fillProvinceSelect(cur) {
  const sel = $("setProvince");
  sel.innerHTML =
    '<option value="">请选择省份</option>' +
    provinces().map((p) => `<option>${escapeHtml(p)}</option>`).join("");
  if (cur && provinces().includes(cur)) sel.value = cur;
}

function fillCitySelect(prov, cur) {
  const sel = $("setCity");
  if (!prov) {
    sel.innerHTML = '<option value="">请先选择省份</option>';
    sel.disabled = true;
    return;
  }
  sel.disabled = false;
  const cs = citiesOf(prov);
  sel.innerHTML =
    '<option value="">全部城市</option>' + cs.map((c) => `<option>${escapeHtml(c)}</option>`).join("");
  sel.value = cs.includes(cur) ? cur : "";
}

function fillSettings() {
  const s = state.settings;
  $("setDown").value = s.down_threads;
  $("setUp").value = s.up_threads;
  $("setDur").value = s.duration_sec;
  $("setBw").value = s.bandwidth;
  $("setAuto").checked = s.auto_match;
  $("setAutoTest").checked = !!s.auto_enabled;
  $("setAutoMin").value = s.auto_interval_min || 30;
  fillProvinceSelect(s.province);
  fillCitySelect(s.province, s.city);
  $("setOper").value = s.oper;
  $("setDeviceId").textContent = s.device_id;
  $("setDataDir").textContent = state.dataDir;
  $("setNodeCount").textContent = state.nodes.length + " 个";
}

$("btnSaveSettings").addEventListener("click", async () => {
  const s = {
    ...state.settings,
    down_threads: parseInt($("setDown").value) || 8,
    up_threads: parseInt($("setUp").value) || 4,
    duration_sec: parseInt($("setDur").value) || 8,
    bandwidth: 200,
    auto_match: $("setAuto").checked,
    auto_enabled: $("setAutoTest").checked,
    auto_interval_min: parseInt($("setAutoMin").value) || 30,
    province: $("setProvince").value,
    city: $("setCity").value,
    oper: $("setOper").value,
  };
  state.settings = await invoke("save_settings", { settings: s });
  $("saveHint").textContent = "已保存 " + new Date().toLocaleTimeString();
  scheduleAuto();
});

$("latSort").addEventListener("change", () => {
  state.sortBy = $("latSort").value;
  renderNodes();
});

$("setProvince").addEventListener("change", () => {
  fillCitySelect($("setProvince").value, "");
});

$("nodeSearch").addEventListener("input", renderNodes);
$("provinceFilter").addEventListener("change", renderNodes);
$("operFilter").addEventListener("change", renderNodes);

$("btnRefreshNodes").addEventListener("click", async () => {
  $("nodesHint").textContent = "正在从 CAICT 拉取最新节点表…";
  try {
    await invoke("refresh_nodes");
    const boot = await invoke("bootstrap");
    state.nodes = boot.nodes;
    fillProvinceFilter();
    renderNodes();
    $("nodeCount").textContent = state.nodes.length + " 个";
  } catch (e) {
    $("nodesHint").textContent = "刷新失败：" + e;
  }
});

$("btnProbeAll").addEventListener("click", async () => {
  if (state.probing) return;
  const keys = [...document.querySelectorAll("#nodeList .node-row")]
    .map((r) => r.querySelector(".lat").dataset.k)
    .slice(0, 160);
  if (!keys.length) return;

  const items = keys.map((k) => {
    const [ip, port] = k.split("|");
    return [ip, parseInt(port)];
  });

  state.probing = true;
  const gen = ++state.probeGen;
  const btn = $("btnProbeAll");
  const stopBtn = $("btnProbeStop");
  btn.disabled = true;
  stopBtn.style.display = "";
  $("nodesHint").textContent = `正在并发测量 ${items.length} 个节点的 TCP 延迟…`;

  try {
    const res = await invoke("probe_nodes_batch", { items });
    if (gen !== state.probeGen) return;           // 已点过“停止”
    for (const [i, rtt] of res) {
      const k = keys[i];
      if (k !== undefined) state.lat[k] = rtt <= 0 ? -1 : rtt;
    }
    state.sortBy = "asc";
    $("latSort").value = "asc";
    // 原地刷新，不重建列表（避免打断滚动/选中）
    document.querySelectorAll("#nodeList .lat").forEach((el) => {
      const k = el.dataset.k;
      if (state.lat[k] !== undefined) {
        el.textContent = latText(state.lat[k]);
        el.className = latClass(state.lat[k]);
      }
    });
    const alive = res.filter(([, r]) => r > 0).length;
    $("nodesHint").textContent = `延迟测量完成：${alive} 个可用 / ${keys.length} 个已测 · 已按延迟升序排列`;
  } catch (e) {
    $("nodesHint").textContent = "测延迟失败：" + e;
  } finally {
    if (gen === state.probeGen) {
      state.probing = false;
      btn.disabled = false;
      stopBtn.style.display = "none";
    }
  }
});

$("btnProbeStop").addEventListener("click", () => {
  state.probeGen++;
  state.probing = false;
  $("btnProbeAll").disabled = false;
  $("btnProbeStop").style.display = "none";
  $("nodesHint").textContent = "已停止测延迟";
});

$("btnExport").addEventListener("click", async () => {
  const p = await invoke("history_export");
  alert("已导出到：\n" + p);
});

$("btnOpenDir").addEventListener("click", () => invoke("open_folder", { path: state.dataDir }));

$("btnClearHist").addEventListener("click", async () => {
  if (!confirm("确定清空所有历史记录？")) return;
  state.history = await invoke("history_clear");
  renderHistory();
});

$("serverChip").addEventListener("click", () => switchView("nodes"));

/* ---------------- 定时自动测速 ---------------- */
let autoTimer = null;
let autoAt = 0;

function scheduleAuto() {
  if (autoTimer) { clearTimeout(autoTimer); autoTimer = null; }
  const s = state.settings;
  if (!s || !s.auto_enabled) {
    autoAt = 0;
    const el = $("autoNext");
    if (el) el.textContent = "—";
    return;
  }
  const ms = Math.max(1, s.auto_interval_min || 30) * 60000;
  autoAt = Date.now() + ms;
  autoTimer = setTimeout(() => {
    autoAt = 0;
    if (!state.running) startTest();
    scheduleAuto();
  }, ms);
}

setInterval(() => {
  const el = $("autoNext");
  if (!el) return;
  if (!autoAt) { el.textContent = "—"; return; }
  const left = Math.max(0, Math.round((autoAt - Date.now()) / 1000));
  el.textContent = `${Math.floor(left / 60)}:${String(left % 60).padStart(2, "0")}`;
}, 1000);

/* ---------------- 公网出口信息 ---------------- */
async function detectPublicIp() {
  try {
    const info = await invoke("detect_ip");
    if (!info || !info.ip) return;
    state.ipInfo = info;
    state.publicIp = info.ip;
    $("localIp").textContent = info.ip;
    const geo = [info.province, info.city, info.oper].filter(Boolean).join(" · ");
    $("ipGeo").textContent = geo || info.isp || "--";
    $("serverName").textContent = state.selectedNode ? state.selectedNode.hostname : "未选择节点";

    // 用出口归属地/运营商做「就近 + 同运营商」匹配
    const s = { ...state.settings };
    if (info.province) s.province = info.province;
    if (info.city) s.city = info.city;
    if (info.oper) s.oper = info.oper;
    state.settings = await invoke("save_settings", { settings: s });
    fillSettings();
  } catch (e) {
    console.warn("detect_ip failed", e);
  }
}

/* ---------------- 启动 ---------------- */
function drawLogo() {
  const c = $("logoCanvas");
  const x = c.getContext("2d");
  const cx = 20, cy = 24, r = 13;
  x.lineCap = "round";
  x.lineWidth = 4;
  x.strokeStyle = "rgba(40,70,110,1)";
  x.beginPath();
  x.arc(cx, cy, r, (140 * Math.PI) / 180, (400 * Math.PI) / 180);
  x.stroke();
  const g = x.createLinearGradient(0, 0, 40, 40);
  g.addColorStop(0, "#35c8e8");
  g.addColorStop(1, "#57d9a3");
  x.strokeStyle = g;
  x.beginPath();
  x.arc(cx, cy, r, (140 * Math.PI) / 180, (330 * Math.PI) / 180);
  x.stroke();
}

(async function init() {
  drawLogo();
  drawGauge(0);
  requestAnimationFrame(animate);
  try {
    const boot = await invoke("bootstrap");
    state.settings = boot.settings;
    state.nodes = boot.nodes;
    state.history = boot.history;
    state.localIp = boot.local_ip;
    state.dataDir = boot.data_dir;
    $("localIp").textContent = boot.local_ip;
    state.localIp = boot.local_ip;
    setTimeout(detectPublicIp, 120);
    scheduleAuto();
    $("nodeCount").textContent = boot.node_count + " 个";
    $("setNodeCount").textContent = boot.node_count + " 个";
    $("setDataDir").textContent = boot.data_dir;
    $("setDeviceId").textContent = boot.settings.device_id;
    $("pingHost").value = boot.nodes[0] ? boot.nodes[0].hostip : "www.baidu.com";
    fillProvinceFilter();
    fillSettings();
    renderNodes();
    renderHistory();
    setTimeout(fitAll, 80);
    setTimeout(fitAll, 400);
    if (boot.settings.last_server_id) {
      const n = boot.nodes.find((x) => x.hostid === boot.settings.last_server_id);
      if (n) {
        state.selectedNode = n;
        $("serverName").textContent = n.hostname;
        $("nodeInfo").textContent = `${n.hostname} · ${n.hostip}:${n.port}`;
      }
    }
  } catch (e) {
    $("phaseText").textContent = "初始化失败：" + e;
  }
})();

/* ---------------- 画布自适应 ---------------- */
function fitCanvas(cv) {
  const dpr = window.devicePixelRatio || 1;
  const r = cv.getBoundingClientRect();
  const w = Math.max(1, Math.floor((r.width || cv.width) * dpr));
  const h = Math.max(1, Math.floor((r.height || cv.height) * dpr));
  if (cv.width !== w || cv.height !== h) {
    cv.width = w;
    cv.height = h;
  }
}

function fitAll() {
  [gCanvas, $("chartDown"), $("chartUp")].forEach(fitCanvas);
  drawGauge(needleValue);
  drawChart($("chartDown"), state.samples.down, "#35c8e8");
  drawChart($("chartUp"), state.samples.up, "#57d9a3");
}

window.addEventListener("resize", () => {
  fitAll();
});
window.addEventListener("DOMContentLoaded", () => setTimeout(fitAll, 60));

