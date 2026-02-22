// KZB Control Panel — Enhanced Popup Controller with full 400+ settings support
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
const invGrid      = $("inv-grid");

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

// ─── Tab Switching ───────────────────────────────────────────
document.querySelectorAll(".tab").forEach((tab) => {
  tab.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((t) => t.classList.remove("active"));
    document.querySelectorAll(".tab-panel").forEach((p) => p.classList.remove("active"));
    tab.classList.add("active");
    $(tab.dataset.tab).classList.add("active");
  });
});

// ─── Connection Status ───────────────────────────────────────
function setDot(dot, statusEl, connected, label) {
  dot.className = "dot " + (connected ? "dot-ok" : "dot-off");
  statusEl.textContent = label || (connected ? "Connected" : "Disconnected");
}

// ─── Class Section Visibility ────────────────────────────────
function updateClassSections() {
  const val = configSelect.value.toLowerCase();
  document.querySelectorAll(".class-section").forEach((el) => {
    const cls = el.dataset.class;
    el.classList.toggle("visible", val.startsWith(cls));
  });
  // If no match, show all
  const any = document.querySelector(".class-section.visible");
  if (!any) {
    document.querySelectorAll(".class-section").forEach((el) => el.classList.add("visible"));
  }
}

configSelect.addEventListener("change", updateClassSections);
updateClassSections();

// ─── Inventory Grid ─────────────────────────────────────────
function initInventoryGrid() {
  if (!invGrid) return;
  invGrid.innerHTML = "";

  // Create 4x10 grid (40 cells)
  for (let row = 0; row < 4; row++) {
    for (let col = 0; col < 10; col++) {
      const idx = row * 10 + col;
      const cell = document.createElement("div");
      cell.className = "inv-cell free"; // Default: free (1)
      cell.dataset.row = row;
      cell.dataset.col = col;
      cell.dataset.cfgIndex = idx;
      cell.title = `Cell ${row},${col}`;

      cell.addEventListener("click", () => {
        cell.classList.toggle("free");
        debouncedSave();
      });

      invGrid.appendChild(cell);
    }
  }
}

// ─── Config Persistence ──────────────────────────────────────
// Save all data-cfg values to chrome.storage.local
function saveAllSettings() {
  const settings = {};

  // Handle regular inputs
  document.querySelectorAll("[data-cfg]").forEach((el) => {
    const key = el.dataset.cfg;
    if (el.type === "checkbox") {
      settings[key] = el.checked;
    } else if (el.type === "number" || el.type === "range") {
      settings[key] = parseFloat(el.value) || 0;
    } else if (el.tagName === "TEXTAREA") {
      settings[key] = el.value; // Store as-is; agent will parse
    } else {
      settings[key] = el.value;
    }
  });

  // Handle inventory grid
  if (invGrid) {
    const invArray = [];
    for (let row = 0; row < 4; row++) {
      invArray[row] = [];
      for (let col = 0; col < 10; col++) {
        const cell = invGrid.querySelector(`[data-row="${row}"][data-col="${col}"]`);
        // 0 = locked (red), 1 = free (green)
        invArray[row][col] = cell && cell.classList.contains("free") ? 1 : 0;
      }
    }
    settings["Inventory"] = invArray;
  }

  chrome.storage.local.set({ kzbConfig: settings });
}

function loadAllSettings() {
  chrome.storage.local.get(["kzbConfig", "selectedConfig"], (result) => {
    if (result.selectedConfig) {
      configSelect.value = result.selectedConfig;
      updateClassSections();
    }
    if (result.kzbConfig) {
      const cfg = result.kzbConfig;

      // Load regular inputs
      document.querySelectorAll("[data-cfg]").forEach((el) => {
        const key = el.dataset.cfg;
        if (key in cfg) {
          if (el.type === "checkbox") {
            el.checked = cfg[key];
          } else {
            el.value = cfg[key];
          }
        }
      });

      // Load inventory grid
      if (invGrid && Array.isArray(cfg.Inventory)) {
        invGrid.querySelectorAll(".inv-cell").forEach((cell) => {
          const row = parseInt(cell.dataset.row);
          const col = parseInt(cell.dataset.col);
          const val = cfg.Inventory[row] && cfg.Inventory[row][col];
          cell.classList.toggle("free", val === 1);
        });
      }
    }
  });
}

// Debounced save on any setting change
let saveTimeout = null;
function debouncedSave() {
  clearTimeout(saveTimeout);
  saveTimeout = setTimeout(saveAllSettings, 300);
}

document.querySelectorAll("[data-cfg]").forEach((el) => {
  el.addEventListener("change", debouncedSave);
  if (el.type === "number" || el.type === "range") {
    el.addEventListener("input", debouncedSave);
  }
});

// ─── Sub-options Toggle ──────────────────────────────────────
// When a script checkbox is checked, show its sub-options
function initSubOptsToggle() {
  document.querySelectorAll("[data-subs]").forEach((parentCheckbox) => {
    const targetId = parentCheckbox.dataset.subs;
    const targetDiv = document.getElementById(targetId);
    if (!targetDiv) return;

    // Set initial visibility
    targetDiv.classList.toggle("visible", parentCheckbox.checked);

    // Toggle on change
    parentCheckbox.addEventListener("change", () => {
      targetDiv.classList.toggle("visible", parentCheckbox.checked);
    });
  });
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
    btnMapActivate.disabled = !mapInfo.enabled;
    btnMapDeactivate.disabled = mapInfo.enabled;
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
  chrome.storage.local.set({ selectedConfig: val });
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
  if (confirm("Kill all KZB processes? This cannot be undone!")) {
    await send("killMap");
  }
});

opacitySlider.addEventListener("input", () => {
  opacityValue.textContent = opacitySlider.value;
});

opacitySlider.addEventListener("change", async () => {
  await send("setOpacity", { value: parseInt(opacitySlider.value, 10) });
});

// Cache stats button
const btnCacheStats = $("btn-cache-stats");
if (btnCacheStats) {
  btnCacheStats.addEventListener("click", async () => {
    await send("getCacheStats");
  });
}

// ─── Bulk Config Push ────────────────────────────────────────
// When settings change, push entire config object to agent
function pushConfigToAgent() {
  const settings = {};

  document.querySelectorAll("[data-cfg]").forEach((el) => {
    const key = el.dataset.cfg;
    if (el.type === "checkbox") {
      settings[key] = el.checked;
    } else if (el.type === "number" || el.type === "range") {
      settings[key] = parseFloat(el.value) || 0;
    } else if (el.tagName === "TEXTAREA") {
      settings[key] = el.value;
    } else {
      settings[key] = el.value;
    }
  });

  // Include inventory
  if (invGrid) {
    const invArray = [];
    for (let row = 0; row < 4; row++) {
      invArray[row] = [];
      for (let col = 0; col < 10; col++) {
        const cell = invGrid.querySelector(`[data-row="${row}"][data-col="${col}"]`);
        invArray[row][col] = cell && cell.classList.contains("free") ? 1 : 0;
      }
    }
    settings["Inventory"] = invArray;
  }

  send("update_config", { data: settings });
}

// Push on any data-cfg change (debounced)
let pushTimeout = null;
document.querySelectorAll("[data-cfg]").forEach((el) => {
  el.addEventListener("change", () => {
    clearTimeout(pushTimeout);
    pushTimeout = setTimeout(pushConfigToAgent, 500);
  });
});

// ─── Start ───────────────────────────────────────────────────
document.addEventListener("DOMContentLoaded", () => {
  initInventoryGrid();
  initSubOptsToggle();
  loadAllSettings();
  refresh();
  pollTimer = setInterval(refresh, 2000);
});

// Cleanup when popup closes
window.addEventListener("unload", () => {
  if (pollTimer) clearInterval(pollTimer);
});
