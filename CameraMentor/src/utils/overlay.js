import { COLORS } from './constants';
import { getPath, normalizeBox } from './helpers';

export function renderOverlay(ctx, data, overlayField) {
  ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);

  if (!overlayField) return;

  const detections = getPath(data, overlayField);
  if (!Array.isArray(detections) || detections.length === 0) return;

  const cw = ctx.canvas.width;
  const ch = ctx.canvas.height;
  const fontSize = Math.max(14, cw / 35);
  const lineWidth = Math.max(2, cw / 400);

  detections.forEach((det, i) => {
    const box = normalizeBox(det);
    if (!box) return;

    const x = box.x * cw;
    const y = box.y * ch;
    const w = box.w * cw;
    const h = box.h * ch;

    const color = COLORS[i % COLORS.length];
    const label = det.label || det.class || det.name || `#${i}`;
    const conf = det.confidence ?? det.score ?? det.conf;
    const labelText = conf != null ? `${label} ${(conf * 100).toFixed(0)}%` : label;

    ctx.strokeStyle = color;
    ctx.lineWidth = lineWidth;
    ctx.strokeRect(x, y, w, h);

    ctx.fillStyle = color + '1a';
    ctx.fillRect(x, y, w, h);

    ctx.font = `bold ${fontSize}px -apple-system, sans-serif`;
    ctx.textBaseline = 'middle';
    const textWidth = ctx.measureText(labelText).width;
    const labelH = fontSize * 1.5;
    const labelY = y < labelH ? y + h : y - labelH;

    ctx.fillStyle = color;
    ctx.fillRect(x, labelY, textWidth + fontSize * 0.6, labelH);

    ctx.fillStyle = '#000';
    ctx.fillText(labelText, x + fontSize * 0.3, labelY + labelH / 2);
  });
}

export function clearOverlay(ctx) {
  if (!ctx) return;
  ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
}
