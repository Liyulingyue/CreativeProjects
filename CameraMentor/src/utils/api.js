import { mockResponse, blobToBase64 } from './helpers';

export async function sendFrame(blob, settings) {
  if (settings.mockMode) {
    await new Promise((r) => setTimeout(r, 200));
    return mockResponse();
  }

  if (!settings.apiUrl) throw new Error('未配置 API 地址');

  const headers = {};
  try {
    const extra = JSON.parse(settings.extraHeaders);
    Object.assign(headers, extra);
  } catch {
    // ignore malformed JSON
  }

  let body;
  if (settings.requestFormat === 'multipart') {
    body = new FormData();
    body.append(settings.imageField, blob, 'frame.jpg');
  } else {
    const base64 = await blobToBase64(blob);
    headers['Content-Type'] = 'application/json';
    body = JSON.stringify({ [settings.imageField]: base64 });
  }

  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), 15000);

  try {
    const res = await fetch(settings.apiUrl, {
      method: 'POST',
      headers,
      body,
      signal: controller.signal,
    });
    if (!res.ok) throw new Error(`API ${res.status}: ${res.statusText}`);
    return await res.json();
  } catch (err) {
    if (err.name === 'AbortError') throw new Error('API 请求超时 (15s)');
    throw err;
  } finally {
    clearTimeout(timeout);
  }
}
