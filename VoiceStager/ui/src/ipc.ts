export function sendIpc(msg: { type: string; value?: string }) {
  const json = JSON.stringify(msg);
  const ipc = (window as any).ipc;
  if (ipc && ipc.postMessage) {
    ipc.postMessage(json);
  } else {
    window.postMessage({ type: '__ipc__', data: json }, '*');
  }
}
