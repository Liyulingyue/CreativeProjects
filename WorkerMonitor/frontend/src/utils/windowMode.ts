export async function enterCompactMode() {
  await window.ipc.invoke("enter_compact_mode");
}

export async function enterExpandedMode() {
  await window.ipc.invoke("enter_expanded_mode");
}

export async function hideToTray() {
  await window.ipc.invoke("hide_to_tray");
}
