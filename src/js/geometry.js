// Pure coordinate math for the editor. No DOM, no module state: every
// function takes what it needs and returns a value, so the whole
// transform stack (image pixels -> crop offset -> pretty margin -> fit
// scale x zoom + pan) lives in one tested place instead of being
// re-derived by each tool. Node runs these directly under `npm test`.

// The visible region in full-image pixels. A null crop means the whole
// image is visible.
export function viewRect(crop, imageW, imageH) {
  return crop || { x: 0, y: 0, w: imageW, h: imageH };
}

// Pretty mode padding around the screenshot, in image pixels.
export function prettyPad(viewW, viewH) {
  return Math.max(48, Math.round(Math.max(viewW, viewH) / 10));
}

// Corner radius of the screenshot card in pretty mode, in image pixels.
export function prettyRadius(viewW) {
  return Math.max(8, Math.round(viewW / 90));
}

// The gradient padding is 0 unless pretty mode is on.
export function margin(pretty, viewW, viewH) {
  return pretty ? prettyPad(viewW, viewH) : 0;
}

// Stroke width scaled to the capture so retina shots do not get hairlines.
export function strokeWidth(imageW) {
  return Math.max(4, Math.round(imageW / 320));
}

// A client (mouse) point to full-image pixels. `rect` is the canvas
// bounding box, `canvasW/H` the bitmap size, `m` the pretty margin, and
// `vx/vy` the crop origin.
export function canvasToImage(clientX, clientY, rect, canvasW, canvasH, m, vx, vy) {
  return {
    x: (clientX - rect.left) * (canvasW / rect.width) - m + vx,
    y: (clientY - rect.top) * (canvasH / rect.height) - m + vy,
  };
}

// The largest scale that still fits the canvas bitmap inside the stage,
// never magnifying past 1:1 (zoom handles magnification separately).
export function fitScale(canvasW, canvasH, availW, availH) {
  return Math.min(availW / canvasW, availH / canvasH, 1);
}

// Keep a pan offset from moving the content fully off the stage: once an
// edge reaches the matching stage edge, stop.
export function clampPan(pan, contentSize, stageSize) {
  const max = Math.max(0, (contentSize - stageSize) / 2);
  return Math.min(max, Math.max(-max, pan));
}

// New pan so the point under the cursor stays fixed while zooming. `frac`
// is the cursor position within the canvas (0..1), `stageCenter` the
// stage-center client coordinate, `content` the new on-screen content
// size along this axis, and `client` the cursor client coordinate.
export function panToKeepCursor(client, stageCenter, frac, content) {
  return client - stageCenter - (frac - 0.5) * content;
}

// Clamp an export scale so width and height stay under the canvas ceiling
// (~16k per side) where drawImage would silently produce nothing.
export function clampExportScale(scale, canvasW, canvasH) {
  if (!(scale > 0)) scale = 1;
  const limit = 16000 / Math.max(canvasW, canvasH);
  return Math.min(scale, Math.max(1, limit));
}
