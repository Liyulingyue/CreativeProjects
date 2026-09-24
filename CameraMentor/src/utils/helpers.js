export function getPath(obj, path) {
  if (!path) return null;
  return path.split('.').reduce((acc, key) => acc?.[key], obj);
}

export function normalizeBox(det) {
  if (det.bbox && Array.isArray(det.bbox) && det.bbox.length === 4) {
    const b = det.bbox;
    return { x: b[0], y: b[1], w: b[2], h: b[3] };
  }
  if (det.x != null && det.y != null) {
    if (det.w != null && det.h != null) return { x: det.x, y: det.y, w: det.w, h: det.h };
    if (det.width != null && det.height != null) return { x: det.x, y: det.y, w: det.width, h: det.height };
    if (det.x2 != null && det.y2 != null) return { x: det.x, y: det.y, w: det.x2 - det.x, h: det.y2 - det.y };
  }
  if (det.x1 != null && det.y1 != null && det.x2 != null && det.y2 != null) {
    return { x: det.x1, y: det.y1, w: det.x2 - det.x1, h: det.y2 - det.y1 };
  }
  return null;
}

export function mockResponse() {
  return {
    boxes: [
      { x: 0.15, y: 0.2, w: 0.35, h: 0.4, label: 'Object', confidence: 0.92 },
      { x: 0.58, y: 0.3, w: 0.25, h: 0.35, label: 'Person', confidence: 0.85 },
      { x: 0.05, y: 0.7, w: 0.15, h: 0.12, label: 'Text', confidence: 0.78 },
    ],
    analysis: `检测到 3 个目标:\n- Object (92%)\n- Person (85%)\n- Text (78%)\n时间: ${new Date().toLocaleTimeString()}`,
    timestamp: Date.now(),
  };
}

export function blobToBase64(blob) {
  return new Promise((resolve) => {
    const reader = new FileReader();
    reader.onloadend = () => {
      const result = reader.result;
      resolve(result.substring(result.indexOf(',') + 1));
    };
    reader.readAsDataURL(blob);
  });
}
