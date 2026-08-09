import { invokeIpc } from "../api";

export async function enterCompactMode() {
  await invokeIpc("enter_compact_mode");
}

export async function enterExpandedMode() {
  await invokeIpc("enter_expanded_mode");
}

export async function hideToTray() {
  await invokeIpc("hide_to_tray");
}

export async function startWindowDrag() {
  await invokeIpc("start_window_drag");
}

export async function quitApp() {
  await invokeIpc("quit_app");
}
