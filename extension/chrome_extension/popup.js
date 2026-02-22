// KillZBot Control Panel — Popup Controller
// Communicates with background.js via chrome.runtime.sendMessage

const $ = (id) => document.getElementById(id);

// ─── Elements ────────────────────────────────────────────────
const agentDot     = $("agent-dot");
const agentStatus  = $("agent-status");
const mapDot       = $("map-dot");
const mapStatus    = $("map-status");
const btnPause     = $("btn-pause");
const btnResume    = $("btn-resume");
const configSelect = $("config-select");
const statUptime   = $("stat-uptime");
const statFrames   = $("stat-frames");
const statDecisions = $("stat-decisions");
const statPotions  = $("stat-potions");
const statLoots    = $("stat-loots");
const statChickens = $("stat-chickens");
const btnMapActivate = $("btn-map-activate");
const btnMapDeactivate = $("btn-map-deactivate");
const btnKill      = $("btn-kill");
const opacitySlider = $("opacity-slider");
const opacityValue = $("opacity-value");
const versionEl    = $("version");

// ─── Init ────────────────────────────────────────────────────
versionEl.textContent = "v" + chrome.runtime.getManifest().version;

let pollTimer = null;

// ─── Helpers ─────────────────────────────────────────────────
function send(cmd, extra) {
  return new Promise((resolve) => {
    chrome.runtime.sendMessage({ cmd, ...extra }, (resp) => {
      resolve(resp || {});
    });
  });
}

function formatUptime(ms) {
  if (!ms || ms <= 0) return "—";
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${sec}s`;
  return `${sec}s`;
}

function formatNum(n) {
  if (n === undefined || n === null) return "0";
  return n.toLocaleString();
}

// ─── Connection Status ───────────────────────────────────────
function setDot(dot, statusEl, connected, label) {
  dot.className = "dot " + (connected ? "dot-ok" : "dot-off");
  statusEl.textContent = label || (connected ? "Connected" : "Disconnected");
}

// ─── Poll Loop ───────────────────────────────────────────────
async function refresh() {
  // Agent stats
  const stats = await send("get_stats");
  if (stats && !stats.error) {
    setDot(agentDot, agentStatus, true, stats.paused ? "Paused" : "Running");
    statUptime.textContent = formatUptime(stats.uptime_ms);
    statFrames.textContent = formatNum(stats.frames);
    statDecisions.textContent = formatNum(stats.decisions);
    statPotions.textContent = formatNum(stats.potions);
    statLoots.textContent = formatNum(stats.loots);
    statChickens.textContent = formatNum(stats.chickens);

    btnPause.disabled = !!stats.paused;
    btnResume.disabled = !stats.paused;
  } else {
    setDot(agentDot, agentStatus, false);
    btnPause.disabled = true;
    btnResume.disabled = true;
  }

  // Map status
  const mapInfo = await send("getMapStatus");
  if (mapInfo && mapInfo.connected !== undefined) {
    setDot(mapDot, mapStatus, mapInfo.connected);
    mapLabel.textContent = mapInfo.enabled ? "ON" : "OFF";
    opacitySlider.value = mapInfo.opacity || 180;
    opacityValue.textContent = mapInfo.opacity || 180;
  } else {
    setDot(mapDot, mapStatus, false);
  }
}

// ─── Event Handlers ──────────────────────────────────────────
btnPause.addEventListener("click", async () => {
  await send("pause", { reason: "popup" });
  btnPause.disabled = true;
  btnResume.disabled = false;
  setDot(agentDot, agentStatus, true, "Paused");
});

btnResume.addEventListener("click", async () => {
  await send("resume");
  btnPause.disabled = false;
  btnResume.disabled = true;
  setDot(agentDot, agentStatus, true, "Running");
});

configSelect.addEventListener("change", async () => {
  const val = configSelect.value;
  if (val) {
    await send("update_config", { data: { config_name: val } });
  }
});

btnMapActivate.addEventListener("click", async () => {
  await send("activateMap", { durationMs: 5000 });
  btnMapActivate.disabled = true;
  btnMapDeactivate.disabled = false;
});

btnMapDeactivate.addEventListener("click", async () => {
  await send("deactivateMap");
  btnMapActivate.disabled = false;
  btnMapDeactivate.disabled = true;
});

btnKill.addEventListener("click", async () => {
  if (confirm("Kill all KillZBot processes? This cannot be undone!")) {
    await send("killMap");
  }
});

opacitySlider.addEventListener("input", () => {
  opacityValue.textContent = opacitySlider.value;
});

opacitySlider.addEventListener("change", async () => {
  await send("setOpacity", { value: parseInt(opacitySlider.value, 10) });
});

// ─── Start ───────────────────────────────────────────────────
// Load persisted config selection
chrome.storage.local.get(["selectedConfig"], (result) => {
  if (result.selectedConfig) {
    configSelect.value = result.selectedConfig;
  }
});

configSelect.addEventListener("change", () => {
  chrome.storage.local.set({ selectedConfig: configSelect.value });
});

refresh();
pollTimer = setInterval(refresh, 2000);

// Cleanup when popup closes
window.addEventListener("unload", () => {
  if (pollTimer) clearInterval(pollTimer);
});
