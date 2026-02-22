// =============================================================================
// Chrome Extension Background Script - Dual Host Connection
// =============================================================================
// Connects to TWO Chrome Native Messaging Hosts:
//   1. com.d2vision.agent → chrome_helper.exe (vision/input agent)
//   2. com.d2vision.map   → chrome_map_helper.exe (map overlay)
//
// Add this to your existing background.js alongside the vision host code
// =============================================================================

// ---- Map Host Connection ----

const MAP_HOST_NAME = "com.d2vision.map";
let mapPort = null;
let mapEnabled = true;
let mapOpacity = 180;
let mapPollInterval = null;
const MAP_POLL_MS = 500; // Poll game state every 500ms

function connectToMapHost() {
    try {
        mapPort = chrome.runtime.connectNative(MAP_HOST_NAME);
    } catch (e) {
        console.error("[map] Failed to connect:", e);
        setTimeout(connectToMapHost, 5000);
        return;
    }

    mapPort.onMessage.addListener((msg) => {
        handleMapMessage(msg);
    });

    mapPort.onDisconnect.addListener(() => {
        console.warn("[map] Host disconnected. Reconnecting in 5s...");
        mapPort = null;
        stopMapPolling();
        setTimeout(connectToMapHost, 5000);
    });

    // Handshake
    mapPort.postMessage({
        cmd: "handshake",
        version: chrome.runtime.getManifest().version,
    });

    console.log("[map] Connected to map host");
}

function handleMapMessage(msg) {
    switch (msg.cmd) {
        case "handshake_ack":
            console.log(`[map] Host v${msg.version}, PID ${msg.pid}, D2R: ${msg.d2r_attached}`);
            // Start polling game state
            startMapPolling();
            break;

        case "pong":
            // Latency check
            if (msg.timestamp) {
                const latency = Date.now() - msg.timestamp;
                console.debug(`[map] Ping: ${latency}ms, polls: ${msg.poll_count}`);
            }
            break;

        case "state":
            if (msg.game_state) {
                updateMapOverlay(msg.game_state, msg.map);
            } else if (msg.error) {
                console.debug("[map] State error:", msg.error);
            }
            break;

        case "map_data":
            renderMapData(msg);
            break;

        case "toggle_ack":
            mapEnabled = msg.enabled;
            console.log(`[map] Toggle: ${mapEnabled ? "ON" : "OFF"}`);
            break;

        case "opacity_ack":
            mapOpacity = msg.opacity;
            break;

        case "cache_stats":
            console.log(`[map] Cache: ${msg.cached_maps}/${msg.max_cache}, polls: ${msg.poll_count}`);
            break;

        case "error":
            console.error(`[map] Error (${msg.context}): ${msg.error}`);
            break;

        default:
            console.debug("[map] Unknown:", msg);
    }
}

// ---- Polling ----

function startMapPolling() {
    stopMapPolling();
    mapPollInterval = setInterval(() => {
        if (mapPort && mapEnabled) {
            mapPort.postMessage({ cmd: "read_state" });
        }
    }, MAP_POLL_MS);
}

function stopMapPolling() {
    if (mapPollInterval) {
        clearInterval(mapPollInterval);
        mapPollInterval = null;
    }
}

// ---- Map Overlay Rendering ----
// This sends map data to the content script for canvas overlay

function updateMapOverlay(gameState, mapData) {
    if (!gameState.in_game || gameState.is_town) {
        // Hide overlay when in town or not in game
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

function renderMapData(data) {
    broadcastToTabs({
        type: "MAP_RENDER",
        mapData: data,
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

// ---- Map Control Commands ----

function sendMapCommand(cmd) {
    if (mapPort) {
        mapPort.postMessage(cmd);
    }
}

function toggleMap() {
    mapEnabled = !mapEnabled;
    sendMapCommand({ cmd: "toggle_map", enabled: mapEnabled });
    chrome.storage.local.set({ mapEnabled });
}

function setMapOpacity(value) {
    mapOpacity = Math.max(10, Math.min(255, value));
    sendMapCommand({ cmd: "set_opacity", opacity: mapOpacity });
    chrome.storage.local.set({ mapOpacity });
}

function requestMapGeneration(seed, areaId, difficulty) {
    sendMapCommand({
        cmd: "generate_map",
        seed: seed,
        area_id: areaId,
        difficulty: difficulty,
    });
}

// ---- Keyboard Shortcuts ----

chrome.commands.onCommand.addListener((command) => {
    switch (command) {
        case "toggle-map":
            toggleMap();
            break;
        case "increase-opacity":
            setMapOpacity(mapOpacity + 25);
            break;
        case "decrease-opacity":
            setMapOpacity(mapOpacity - 25);
            break;
    }
});

// ---- Extension Message Handler (from popup/options) ----

chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
    switch (request.action) {
        case "getMapStatus":
            sendResponse({
                enabled: mapEnabled,
                opacity: mapOpacity,
                connected: mapPort !== null,
            });
            break;

        case "toggleMap":
            toggleMap();
            sendResponse({ enabled: mapEnabled });
            break;

        case "setOpacity":
            setMapOpacity(request.value);
            sendResponse({ opacity: mapOpacity });
            break;

        case "generateMap":
            requestMapGeneration(request.seed, request.areaId, request.difficulty);
            sendResponse({ status: "requested" });
            break;

        case "getCacheStats":
            sendMapCommand({ cmd: "cache_stats" });
            sendResponse({ status: "requested" });
            break;
    }
    return true;
});

// ---- Initialize ----

// Restore settings
chrome.storage.local.get(["mapEnabled", "mapOpacity"], (result) => {
    if (result.mapEnabled !== undefined) mapEnabled = result.mapEnabled;
    if (result.mapOpacity !== undefined) mapOpacity = result.mapOpacity;
});

// Connect to map host (call this alongside your vision host connect)
connectToMapHost();
