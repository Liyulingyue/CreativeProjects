import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

export async function enterCompactMode() {
  try {
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
  try {
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
  try {
    const win = getCurrentWindow();
    await win.hide();
  } catch (err) {
    console.error("[windowMode] hideToTray error:", err);
  }
}
