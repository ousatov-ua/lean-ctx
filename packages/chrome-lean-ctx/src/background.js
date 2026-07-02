const NATIVE_HOST = "com.leanctx.bridge";

const DEFAULTS = {
  enabled: true,
  autoCompressPaste: true,
  threshold: 50,
};

let settings = { ...DEFAULTS };
// Enterprise policy (chrome.storage.managed, enterprise#29): IT-managed values
// override user choices; managedKeys tells the popup which controls to lock.
let managed = {};

let nativeAvailable = null; // null = unknown, true/false = tested

function recomputeSettings(local) {
  settings = { ...DEFAULTS, ...(local || {}), ...managed };
}

function reloadAllSettings() {
  chrome.storage.managed.get(null, (policy) => {
    // storage.managed throws for unmanaged profiles in some Chromium builds —
    // treat any error as "no policy".
    managed = chrome.runtime.lastError ? {} : policy || {};
    chrome.storage.local.get(["settings"], (result) => {
      recomputeSettings(result.settings);
    });
  });
}

reloadAllSettings();

chrome.storage.onChanged.addListener((changes, areaName) => {
  if (areaName === "managed" || changes.settings) {
    reloadAllSettings();
  }
});

function sendNativeMessage(msg) {
  return new Promise((resolve) => {
    try {
      chrome.runtime.sendNativeMessage(NATIVE_HOST, msg, (response) => {
        if (chrome.runtime.lastError) {
          console.log("lean-ctx native error:", chrome.runtime.lastError.message);
          nativeAvailable = false;
          resolve({ error: chrome.runtime.lastError.message });
        } else {
          nativeAvailable = true;
          resolve(response);
        }
      });
    } catch (e) {
      nativeAvailable = false;
      resolve({ error: e.message || "native messaging failed" });
    }

    setTimeout(() => resolve({ error: "timeout" }), 8000);
  });
}

function compressFallback(text) {
  let result = text;
  result = result.replace(/\r\n/g, "\n");
  result = result.replace(/\n{3,}/g, "\n\n");
  result = result.replace(/[ \t]+$/gm, "");
  result = result.replace(/^\s*\/\/.*$/gm, "");
  result = result.replace(/^\s*#(?!!).*$/gm, "");

  const inputTokens = estimateTokens(text);
  const outputTokens = estimateTokens(result);
  const savings = inputTokens > 0 ? ((inputTokens - outputTokens) / inputTokens) * 100 : 0;

  return { compressed: result, inputTokens, outputTokens, savings };
}

function estimateTokens(text) {
  return Math.ceil(text.length / 4);
}

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  if (message.action === "compress") {
    const text = message.text;
    if (!settings.enabled || estimateTokens(text) < settings.threshold) {
      sendResponse({ compressed: text, savings: 0, skipped: true });
      return true;
    }

    sendNativeMessage({ action: "compress", text }).then((result) => {
      if (result.error) {
        sendResponse(compressFallback(text));
      } else {
        sendResponse(result);
      }
    });
    return true;
  }

  if (message.action === "getSettings") {
    sendResponse({
      ...settings,
      managedKeys: Object.keys(managed),
      gatewayBaseUrl: managed.gatewayBaseUrl || null,
    });
    return true;
  }

  if (message.action === "getStats") {
    chrome.storage.local.get(["stats"], (result) => {
      sendResponse(result.stats || { totalSaved: 0, totalCommands: 0 });
    });
    return true;
  }

  if (message.action === "pingNative") {
    sendNativeMessage({ action: "ping" }).then((result) => {
      if (result.error) {
        sendResponse({ nativeOk: false, error: result.error });
      } else {
        sendResponse({ nativeOk: true, binary: result.binary || "connected" });
      }
    });
    return true;
  }

  return false;
});
