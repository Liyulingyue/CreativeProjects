let isBlocked = false;

export function setIpcBlock(blocked: boolean) {
  isBlocked = blocked;
}

export function sendIpc(msg: { type: string; value?: string }) {
  if (isBlocked) {
    console.log('IPC blocked:', msg.type);
    return;
  }
  const json = JSON.stringify(msg);
  const ipc = (window as any).ipc;
  if (ipc && ipc.postMessage) {
    ipc.postMessage(json);
  } else {
    window.postMessage({ type: '__ipc__', data: json }, '*');
  }
}
