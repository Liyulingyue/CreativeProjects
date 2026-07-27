import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

export async function enterCompactMode() {
  const win = getCurrentWindow();
  await win.setDecorations(false);
  await win.setAlwaysOnTop(true);
  await win.setSkipTaskbar(true);
  await win.setResizable(false);
  await win.setSize(new LogicalSize(220, 148));
}

export async function enterExpandedMode() {
  const win = getCurrentWindow();
  await win.setDecorations(true);
  await win.setAlwaysOnTop(false);
  await win.setSkipTaskbar(false);
  await win.setResizable(true);
  await win.setSize(new LogicalSize(1024, 720));
}

export async function hideToTray() {
  const win = getCurrentWindow();
  await win.hide();
}
