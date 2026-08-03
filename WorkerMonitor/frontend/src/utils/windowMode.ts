declare global {
  interface Window {
    __TAURI__?: object;
  }
}

export async function enterCompactMode() {
  if (!window.__TAURI__) {
    console.warn("[windowMode] Not in Tauri, skipping enterCompactMode");
    return;
  }
  try {
    const { getCurrentWindow, LogicalSize } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    await win.setDecorations(false);
    await win.setAlwaysOnTop(true);
    await win.setSkipTaskbar(true);
    await win.setResizable(false);
    await win.setSize(new LogicalSize(220, 148));
  } catch (err) {
    console.error("[windowMode] enterCompactMode error:", err);
  }
}

export async function enterExpandedMode() {
  if (!window.__TAURI__) {
    console.warn("[windowMode] Not in Tauri, skipping enterExpandedMode");
    return;
  }
  try {
    const { getCurrentWindow, LogicalSize } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    await win.setDecorations(true);
    await win.setAlwaysOnTop(false);
    await win.setSkipTaskbar(false);
    await win.setResizable(true);
    await win.setSize(new LogicalSize(1024, 720));
  } catch (err) {
    console.error("[windowMode] enterExpandedMode error:", err);
  }
}

export async function hideToTray() {
  if (!window.__TAURI__) {
    console.warn("[windowMode] Not in Tauri, skipping hideToTray");
    return;
  }
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const win = getCurrentWindow();
    await win.hide();
  } catch (err) {
    console.error("[windowMode] hideToTray error:", err);
  }
}
