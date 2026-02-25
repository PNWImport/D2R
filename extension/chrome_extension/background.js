// KZB Control Panel — Unified Background Script
// Connects to TWO native messaging hosts:
//   1. com.chromium.* → chrome_helper.exe  (vision/input agent)
//   2. com.chromium.* → chrome_map_helper.exe (map overlay)

// ═══════════════════════════════════════════════════════════════
// VISION AGENT HOST
// ═══════════════════════════════════════════════════════════════

const AGENT_HOST = "com.chromium.display.calibration";
const RECONNECT_BASE_MS = 5000;
const RECONNECT_MAX_MS = 30000;
const HEARTBEAT_MS = 30000;

let agentPort = null;
let agentReconnectTimer = null;
let agentHeartbeatTimer = null;
let agentAttempts = 0;
let agentPaused = false;

function connectToAgent() {
  agentAttempts++;
  try {
    agentPort = chrome.runtime.connectNative(AGENT_HOST);

    agentPort.onMessage.addListener((msg) => handleAgentMessage(msg));

    agentPort.onDisconnect.addListener(() => {
      agentPort = null;
      agentPaused = false;
      if (agentHeartbeatTimer) {
        clearInterval(agentHeartbeatTimer);
        agentHeartbeatTimer = null;
      }
      const delay = Math.min(
        RECONNECT_BASE_MS * Math.pow(1.5, agentAttempts - 1),
        RECONNECT_MAX_MS
      );
      if (agentReconnectTimer) clearTimeout(agentReconnectTimer);
      agentReconnectTimer = setTimeout(connectToAgent, delay);
    });

    agentPort.postMessage({
      cmd: "handshake",
      version: chrome.runtime.getManifest().version,
      extensionId: chrome.runtime.id,
      timestamp: Date.now()
    });

    agentHeartbeatTimer = setInterval(() => {
      if (agentPort) agentPort.postMessage({ cmd: "ping", timestamp: Date.now() });
    }, HEARTBEAT_MS);

    agentAttempts = 0;
  } catch (_) {
    agentReconnectTimer = setTimeout(connectToAgent, RECONNECT_BASE_MS);
  }
}

// Latest FrameState from the vision agent, kept fresh by the debug relay poll.
// Fields mirror FrameState in botter/src/vision/shard_buffer.rs.
let lastFrameState = null;

function handleAgentMessage(msg) {
  // Silent handling — no UI, no notifications, no storage writes.
  switch (msg.cmd) {
    case "handshake_ack": break;
    case "pong":          break;
    case "stats":         break;
    case "buffer_stats":  break;
    case "ack":           break;
    case "error":         break;
    case "frame_state":
      if (msg.available !== false) lastFrameState = msg;
      break;
    default:              break;
  }
}

// ═══════════════════════════════════════════════════════════════
// MAP HOST
// ═══════════════════════════════════════════════════════════════

const MAP_HOST = "com.chromium.canvas.accessibility";
const MAP_POLL_MS = 500;
const MAP_ACTIVATION_TIMEOUT_MS = 5000; // Auto-activate for 5 seconds per button press

let mapPort = null;
let mapEnabled = true;
let mapActive = false; // Button-activated state
let mapOpacity = 180;
let mapPollInterval = null;
let mapReconnectTimer = null;
let mapActivationTimer = null;
// Debug overlay relay (Option B)
let debugOverlayEnabled = false;
let debugRelayInterval = null;

function connectToMapHost() {
  try {
    mapPort = chrome.runtime.connectNative(MAP_HOST);
  } catch (e) {
    mapReconnectTimer = setTimeout(connectToMapHost, 5000);
    return;
  }

  mapPort.onMessage.addListener((msg) => handleMapMessage(msg));

  mapPort.onDisconnect.addListener(() => {
    mapPort = null;
    stopMapPolling();
    stopDebugStatRelay();
    mapReconnectTimer = setTimeout(connectToMapHost, 5000);
  });

  mapPort.postMessage({
    cmd: "handshake",
    version: chrome.runtime.getManifest().version,
  });
}

function handleMapMessage(msg) {
  switch (msg.cmd) {
    case "handshake_ack":
      startMapPolling();
      break;
    case "pong":
      break;
    case "state":
      if (msg.game_state) {
        updateMapOverlay(msg.game_state, msg.map);
        // NOTE: We do NOT relay map host memory-read data to the vision agent.
        // The vision agent uses pure screen capture (DXGI) for all detection —
        // including navigation — to avoid the detection risk of ReadProcessMemory.
        // Navigation uses minimap exit marker detection instead.
      }
      break;
    case "map_data":
      broadcastToTabs({ type: "MAP_RENDER", mapData: msg, opacity: mapOpacity });
      break;
    case "toggle_ack":
      mapEnabled = msg.enabled;
      break;
    case "opacity_ack":
      mapOpacity = msg.opacity;
      break;
    case "activate_ack":
      mapActive = true;
      if (mapActivationTimer) clearTimeout(mapActivationTimer);
      mapActivationTimer = setTimeout(() => {
        mapActive = false;
        if (mapPort) mapPort.postMessage({ cmd: "deactivate_map" });
      }, msg.duration_ms || MAP_ACTIVATION_TIMEOUT_MS);
      break;
    case "deactivate_ack":
      mapActive = false;
      if (mapActivationTimer) {
        clearTimeout(mapActivationTimer);
        mapActivationTimer = null;
      }
      break;
    case "kill_ack":
      mapPort = null;
      stopMapPolling();
      stopDebugStatRelay();
      if (mapReconnectTimer) { clearTimeout(mapReconnectTimer); mapReconnectTimer = null; }
      break;
    case "cache_stats":
      break;
    case "error":
      break;
    default:
      break;
  }
}

function startMapPolling() {
  stopMapPolling();
  mapPollInterval = setInterval(() => {
    if (mapPort && mapEnabled && mapActive) mapPort.postMessage({ cmd: "read_state" });
  }, MAP_POLL_MS);
}

function stopMapPolling() {
  if (mapPollInterval) {
    clearInterval(mapPollInterval);
    mapPollInterval = null;
  }
}

// ── Debug stat relay (Option B) ───────────────────────────────────────────────
// When the in-game debug overlay is active, poll the vision agent for its
// latest FrameState every 100 ms and forward it to the map host as
// update_debug_state so the Win32 overlay window knows what to draw.

function startDebugStatRelay() {
  stopDebugStatRelay();
  debugRelayInterval = setInterval(() => {
    if (!debugOverlayEnabled || !mapPort) return;

    // Request a fresh FrameState from the vision agent on every tick.
    // handleAgentMessage captures the response into lastFrameState.
    if (agentPort) agentPort.postMessage({ cmd: "get_frame_state" });

    // Forward whatever we have (may be one tick stale — fine at 100 ms).
    const s = lastFrameState;
    if (!s) return;

    // Derive in_game: frame is active, not stuck at menu or loading screen
    const in_game = !s.at_menu && !s.loading_screen && (s.frame_width > 0);

    mapPort.postMessage({
      cmd: "update_debug_state",
      hp_pct:               s.hp_pct               ?? 100,
      mp_pct:               s.mana_pct             ?? 100,   // FrameState uses mana_pct
      merc_hp_pct:          s.merc_hp_pct           ?? 100,
      enemy_count:          s.enemy_count           ?? 0,
      nearest_enemy_x:      s.nearest_enemy_x       ?? 640,
      nearest_enemy_y:      s.nearest_enemy_y       ?? 360,
      nearest_enemy_hp_pct: s.nearest_enemy_hp_pct  ?? 0,
      chicken_hp_pct:       0,  // comes from config, not frame — overlay shows threshold line separately
      area_name:            s.area_name             ?? "",
      in_game,
    });
  }, 100);
}

function stopDebugStatRelay() {
  if (debugRelayInterval) {
    clearInterval(debugRelayInterval);
    debugRelayInterval = null;
  }
}

function updateMapOverlay(gameState, mapData) {
  if (!gameState.in_game || gameState.is_town) {
    broadcastToTabs({ type: "MAP_HIDE" });
    return;
  }
  broadcastToTabs({
    type: "MAP_UPDATE",
    gameState: gameState,
    mapData: mapData,
    opacity: mapOpacity,
  });
}

function broadcastToTabs(message) {
  chrome.tabs.query({}, (tabs) => {
    tabs.forEach((tab) => {
      chrome.tabs.sendMessage(tab.id, message).catch(() => {});
    });
  });
}

// ═══════════════════════════════════════════════════════════════
// EXTENSION ICON CLICK → open panel as full browser tab
// ═══════════════════════════════════════════════════════════════

chrome.action.onClicked.addListener(async () => {
  const panelUrl = chrome.runtime.getURL("popup.html");
  // Reuse existing KZB tab if one is already open
  const tabs = await chrome.tabs.query({ url: panelUrl });
  if (tabs.length > 0) {
    chrome.tabs.update(tabs[0].id, { active: true });
    chrome.windows.update(tabs[0].windowId, { focused: true });
  } else {
    chrome.tabs.create({ url: panelUrl });
  }
});

// ═══════════════════════════════════════════════════════════════
// KEYBOARD SHORTCUTS
// ═══════════════════════════════════════════════════════════════

chrome.commands.onCommand.addListener((command) => {
  switch (command) {
    case "toggle-pause":
      if (agentPort) {
        if (agentPaused) {
          agentPort.postMessage({ cmd: "resume" });
          agentPaused = false;
        } else {
          agentPort.postMessage({ cmd: "pause", reason: "hotkey" });
          agentPaused = true;
        }
      }
      break;
    case "toggle-map":
      mapEnabled = !mapEnabled;
      if (mapPort) mapPort.postMessage({ cmd: "toggle_map", enabled: mapEnabled });
      chrome.storage.local.set({ mapEnabled });
      break;
    case "increase-opacity":
      mapOpacity = Math.min(255, mapOpacity + 25);
      if (mapPort) mapPort.postMessage({ cmd: "set_opacity", opacity: mapOpacity });
      chrome.storage.local.set({ mapOpacity });
      break;
    case "decrease-opacity":
      mapOpacity = Math.max(10, mapOpacity - 25);
      if (mapPort) mapPort.postMessage({ cmd: "set_opacity", opacity: mapOpacity });
      chrome.storage.local.set({ mapOpacity });
      break;
  }
});

// ═══════════════════════════════════════════════════════════════
// EXTENSION MESSAGE API (from devtools / popup if added later)
// ═══════════════════════════════════════════════════════════════

// Track last known stats so popup gets instant response
let lastAgentStats = null;

chrome.runtime.onMessage.addListener((request, _sender, sendResponse) => {
  switch (request.cmd) {
    // ── Status overview (instant, no native round-trip) ──
    case "getStatus":
      sendResponse({
        agent: { connected: agentPort !== null, stats: lastAgentStats },
        map: { connected: mapPort !== null, enabled: mapEnabled, opacity: mapOpacity },
      });
      break;

    // ── Agent commands ──
    case "get_stats":
      if (!agentPort) { sendResponse({ error: "agent_not_connected" }); return; }
      agentPort.postMessage({ cmd: "get_stats" });
      {
        let responded = false;
        const port = agentPort; // Capture reference; port may disconnect before timeout
        const statsListener = (msg) => {
          if (msg.cmd === "stats" && !responded) {
            responded = true;
            try { port.onMessage.removeListener(statsListener); } catch (_) {}
            lastAgentStats = msg.data;
            sendResponse(msg.data);
          }
        };
        port.onMessage.addListener(statsListener);
        setTimeout(() => {
          if (!responded) {
            responded = true;
            try { port.onMessage.removeListener(statsListener); } catch (_) {}
            sendResponse(lastAgentStats || { error: "timeout" });
          }
        }, 3000);
      }
      return true; // keep sendResponse alive for async

    case "pause":
      if (agentPort) agentPort.postMessage({ cmd: "pause", reason: request.reason || "manual" });
      sendResponse({ status: "sent" });
      break;

    case "resume":
      if (agentPort) agentPort.postMessage({ cmd: "resume" });
      sendResponse({ status: "sent" });
      break;

    case "update_config":
      if (agentPort && request.data) {
        agentPort.postMessage({ cmd: "update_config", data: request.data });
      }
      sendResponse({ status: "sent" });
      break;

    // ── Map commands ──
    case "getMapStatus":
      sendResponse({ enabled: mapEnabled, active: mapActive, opacity: mapOpacity, connected: mapPort !== null });
      break;

    case "toggleMap":
      mapEnabled = !mapEnabled;
      if (mapPort) mapPort.postMessage({ cmd: "toggle_map", enabled: mapEnabled });
      chrome.storage.local.set({ mapEnabled });
      sendResponse({ enabled: mapEnabled });
      break;

    case "setOpacity":
      mapOpacity = Math.max(10, Math.min(255, request.value));
      if (mapPort) mapPort.postMessage({ cmd: "set_opacity", opacity: mapOpacity });
      chrome.storage.local.set({ mapOpacity });
      sendResponse({ opacity: mapOpacity });
      break;

    case "generateMap":
      if (mapPort) mapPort.postMessage({
        cmd: "generate_map",
        seed: request.seed,
        area_id: request.areaId,
        difficulty: request.difficulty,
      });
      sendResponse({ status: "requested" });
      break;

    case "getCacheStats":
      if (mapPort) mapPort.postMessage({ cmd: "cache_stats" });
      sendResponse({ status: "requested" });
      break;

    case "activateMap":
      if (mapPort) {
        mapPort.postMessage({
          cmd: "activate_map",
          duration_ms: request.durationMs || MAP_ACTIVATION_TIMEOUT_MS,
        });
      }
      sendResponse({ status: "requested" });
      break;

    case "deactivateMap":
      if (mapPort) mapPort.postMessage({ cmd: "deactivate_map" });
      sendResponse({ status: "requested" });
      break;

    case "killMap":
      if (mapPort) {
        mapPort.postMessage({
          cmd: "kill",
          reason: request.reason || "user_kill",
        });
      }
      sendResponse({ status: "requested" });
      break;

    case "getMapActive":
      sendResponse({ active: mapActive });
      break;

    // ── Demo Mode (Option A) ──────────────────────────────────────────────────
    // Tells the map host to return synthetic game state instead of reading D2R
    // memory. Lets you verify the Chrome canvas overlay before offsets are fixed.
    case "setDemoMode":
      if (mapPort) mapPort.postMessage({ cmd: "set_demo_mode", enabled: !!request.enabled });
      sendResponse({ status: "requested" });
      break;

    // ── In-Game Debug Overlay (Option B) ─────────────────────────────────────
    // Creates/destroys a Win32 layered topmost window drawn over D2R.
    // When enabled, we also start relaying vision-agent FrameState to the map
    // host every 100 ms so it knows what detections to render.
    case "setDebugOverlay":
      debugOverlayEnabled = !!request.enabled;
      if (mapPort) mapPort.postMessage({ cmd: "set_debug_overlay", enabled: debugOverlayEnabled });
      if (debugOverlayEnabled) {
        startDebugStatRelay();
      } else {
        stopDebugStatRelay();
      }
      sendResponse({ status: "requested" });
      break;

    default:
      sendResponse({ error: "unknown" });
  }
});

// ═══════════════════════════════════════════════════════════════
// LIFECYCLE
// ═══════════════════════════════════════════════════════════════

// Restore persisted settings
chrome.storage.local.get(["mapEnabled", "mapOpacity"], (result) => {
  if (result.mapEnabled !== undefined) mapEnabled = result.mapEnabled;
  if (result.mapOpacity !== undefined) mapOpacity = result.mapOpacity;
});

// Single top-level connect covers onInstalled, onStartup, and service-worker
// restarts.  Previous code had duplicate connect calls in event listeners which
// caused double native-port opens on Chrome start and extension install.
connectToAgent();
connectToMapHost();

// Service workers don't receive "unload". Chrome automatically disconnects
// native messaging ports when the service worker is terminated, so explicit
// cleanup is not strictly needed. We send a best-effort shutdown on suspend.
chrome.runtime.onSuspend && chrome.runtime.onSuspend.addListener(() => {
  if (agentReconnectTimer) clearTimeout(agentReconnectTimer);
  if (agentHeartbeatTimer) clearInterval(agentHeartbeatTimer);
  if (mapReconnectTimer) clearTimeout(mapReconnectTimer);
  if (mapActivationTimer) clearTimeout(mapActivationTimer);
  stopMapPolling();
  stopDebugStatRelay();
  if (agentPort) {
    try { agentPort.postMessage({ cmd: "shutdown" }); agentPort.disconnect(); } catch (_) {}
  }
  if (mapPort) {
    try { mapPort.postMessage({ cmd: "shutdown" }); mapPort.disconnect(); } catch (_) {}
  }
});
