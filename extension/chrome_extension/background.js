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

function connectToAgent() {
  agentAttempts++;
  try {
    agentPort = chrome.runtime.connectNative(AGENT_HOST);

    agentPort.onMessage.addListener((msg) => handleAgentMessage(msg));

    agentPort.onDisconnect.addListener(() => {
      agentPort = null;
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

function handleAgentMessage(msg) {
  // Silent handling — no UI, no notifications, no storage writes.
  switch (msg.cmd) {
    case "handshake_ack": break;
    case "pong":          break;
    case "stats":         break;
    case "buffer_stats":  break;
    case "ack":           break;
    case "error":         break;
    default:              break;
  }
}

// ═══════════════════════════════════════════════════════════════
// MAP HOST
// ═══════════════════════════════════════════════════════════════

// Dynamic host name (will be discovered/generated at runtime)
let MAP_HOST = "com.chromium.map.service"; // Will be replaced by dynamic host name
const MAP_POLL_MS = 500;
const MAP_ACTIVATION_TIMEOUT_MS = 5000; // Auto-activate for 5 seconds per button press

let mapPort = null;
let mapEnabled = true;
let mapActive = false; // Button-activated state
let mapOpacity = 180;
let mapPollInterval = null;
let mapReconnectTimer = null;
let mapActivationTimer = null;

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
      if (msg.game_state) updateMapOverlay(msg.game_state, msg.map);
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
// KEYBOARD SHORTCUTS
// ═══════════════════════════════════════════════════════════════

chrome.commands.onCommand.addListener((command) => {
  switch (command) {
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
        const statsListener = (msg) => {
          if (msg.cmd === "stats" && !responded) {
            responded = true;
            agentPort.onMessage.removeListener(statsListener);
            lastAgentStats = msg.data;
            sendResponse(msg.data);
          }
        };
        agentPort.onMessage.addListener(statsListener);
        // Timeout: don't leave the channel hanging
        setTimeout(() => {
          if (!responded) {
            responded = true;
            agentPort.onMessage.removeListener(statsListener);
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
      sendResponse({ enabled: mapEnabled, opacity: mapOpacity, connected: mapPort !== null });
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

self.addEventListener("unload", () => {
  if (agentReconnectTimer) clearTimeout(agentReconnectTimer);
  if (agentHeartbeatTimer) clearInterval(agentHeartbeatTimer);
  if (mapReconnectTimer) clearTimeout(mapReconnectTimer);
  if (mapActivationTimer) clearTimeout(mapActivationTimer);
  stopMapPolling();
  if (agentPort) {
    agentPort.postMessage({ cmd: "shutdown" });
    agentPort.disconnect();
  }
  if (mapPort) {
    mapPort.postMessage({ cmd: "shutdown" });
    mapPort.disconnect();
  }
});
