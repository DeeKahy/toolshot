const invoke = window.__TAURI__.core.invoke;
const isMac = navigator.platform.toUpperCase().includes("MAC");
if (!isMac) {
  document.getElementById("modKey").textContent = "Ctrl";
  document.getElementById("modKey2").textContent = "Ctrl";
}

const canvas = document.getElementById("shot");
const ctx = canvas.getContext("2d");
const colorInput = document.getElementById("color");
const stage = document.getElementById("stage");
const prettyBtn = document.getElementById("prettyBtn");
const gradientsEl = document.getElementById("gradients");

const GRADIENTS = [
  { name: "Auto (from screenshot)", auto: true },
  { name: "Violet", from: "#667eea", to: "#764ba2" },
  { name: "Ocean", from: "#36d1dc", to: "#5b86e5" },
  { name: "Sunset", from: "#ff9966", to: "#ff5e62" },
  { name: "Forest", from: "#11998e", to: "#38ef7d" },
  { name: "Candy", from: "#fc5c7d", to: "#6a82fb" },
  { name: "Graphite", from: "#485563", to: "#29323c" },
];

let image = null;         // the captured screenshot
let tool = null;          // "rect" | "arrow" | "blur" | "crop" | null
let shapes = [];          // committed annotations, in full-image coordinates
let draft = null;         // shape being dragged right now
let crop = null;          // {x, y, w, h} visible region, null = full image
let history = [];         // undo entries: {kind:"shape"} | {kind:"crop", prev}
let lineWidth = 6;        // in image pixels, set from image size on load
const scratch = document.createElement("canvas"); // downscale buffer for blur

// Pretty mode settings survive across captures, the editor window is
// recreated for every capture so plain variables would reset each time.
let pretty = localStorage.getItem("prettyOn") === "1";
let gradientIndex = parseInt(localStorage.getItem("prettyGradient") || "0", 10);
if (!(gradientIndex >= 0 && gradientIndex < GRADIENTS.length)) gradientIndex = 0;

const savedColor = localStorage.getItem("annotationColor");
if (/^#[0-9a-f]{6}$/i.test(savedColor || "")) colorInput.value = savedColor;
colorInput.addEventListener("change", () => {
  localStorage.setItem("annotationColor", colorInput.value);
});

// Shapes keep full-image coordinates even after cropping, the canvas is
// just translated by the crop origin when drawing.
function viewRect() {
  return crop || { x: 0, y: 0, w: image.naturalWidth, h: image.naturalHeight };
}

invoke("get_capture_png")
  .then((b64) => {
    const img = new Image();
    img.onload = () => {
      image = img;
      // Retina captures are 2x, keep strokes visually consistent.
      lineWidth = Math.max(4, Math.round(img.naturalWidth / 320));
      computeAutoColors();
      setCanvasSize();
      redraw();
    };
    img.src = "data:image/png;base64," + b64;
  })
  .catch((e) => console.error("failed to load capture", e));

// The auto gradient, resolved from the capture once it loads. The fallback
// only shows in the instant before the image arrives.
let autoColors = { from: "#667eea", to: "#764ba2" };

// Derive a gradient from the screenshot itself: bucket pixels by hue,
// weighted by saturation, take the strongest one or two hues and pin
// lightness to a calm midtone so any capture yields a usable background.
function computeAutoColors() {
  const SIZE = 48;
  const sample = document.createElement("canvas");
  sample.width = SIZE;
  sample.height = SIZE;
  const c = sample.getContext("2d", { willReadFrequently: true });
  c.drawImage(image, 0, 0, SIZE, SIZE);
  const data = c.getImageData(0, 0, SIZE, SIZE).data;

  const BUCKETS = 12;
  const weight = new Array(BUCKETS).fill(0);
  const hueX = new Array(BUCKETS).fill(0);
  const hueY = new Array(BUCKETS).fill(0);
  const satSum = new Array(BUCKETS).fill(0);
  let lightSum = 0;
  const total = SIZE * SIZE;

  for (let i = 0; i < data.length; i += 4) {
    const r = data[i] / 255, g = data[i + 1] / 255, b = data[i + 2] / 255;
    const max = Math.max(r, g, b), min = Math.min(r, g, b);
    const l = (max + min) / 2;
    lightSum += l;
    const d = max - min;
    if (d < 0.05) continue; // near-gray, only informs overall lightness
    const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    let h;
    switch (max) {
      case r: h = (g - b) / d + (g < b ? 6 : 0); break;
      case g: h = (b - r) / d + 2; break;
      default: h = (r - g) / d + 4;
    }
    h *= 60;
    // Saturated midtones say the most about the picture's accent color.
    const w = s * Math.max(0, 1 - Math.abs(l - 0.5) * 1.6);
    if (w <= 0) continue;
    const bucket = Math.floor(h / (360 / BUCKETS)) % BUCKETS;
    weight[bucket] += w;
    // Average hues on the circle so a bucket straddling 0 does not break.
    const rad = (h * Math.PI) / 180;
    hueX[bucket] += Math.cos(rad) * w;
    hueY[bucket] += Math.sin(rad) * w;
    satSum[bucket] += s * w;
  }

  const hsl = (h, s, l) =>
    "hsl(" + Math.round((h + 360) % 360) + ", " +
    Math.round(Math.min(1, Math.max(0, s)) * 100) + "%, " +
    Math.round(Math.min(1, Math.max(0, l)) * 100) + "%)";

  const order = weight.map((_, i) => i).sort((a, b) => weight[b] - weight[a]);
  const best = order[0];

  if (weight[best] < total * 0.02) {
    // Mostly grayscale capture: a neutral gradient tracking its brightness,
    // with a whisper of blue so it does not look like dead concrete.
    const avgLight = lightSum / total;
    const l = avgLight > 0.5 ? 0.72 : 0.3;
    autoColors = { from: hsl(220, 0.08, l + 0.06), to: hsl(220, 0.1, l - 0.1) };
  } else {
    const hue1 = (Math.atan2(hueY[best], hueX[best]) * 180) / Math.PI;
    const sat = Math.min(0.65, Math.max(0.35, satSum[best] / weight[best]));
    const second = order[1];
    const hue2 = weight[second] > weight[best] * 0.35
      ? (Math.atan2(hueY[second], hueX[second]) * 180) / Math.PI
      : hue1 + 35;
    autoColors = { from: hsl(hue1, sat, 0.55), to: hsl(hue2, sat * 0.9, 0.36) };
  }

  gradientsEl.children[0].style.background = gradientCss(autoColors);
}

// Pretty mode geometry, in image pixels, relative to the visible view.
function prettyPad() {
  const v = viewRect();
  return Math.max(48, Math.round(Math.max(v.w, v.h) / 10));
}

function prettyRadius() {
  return Math.max(8, Math.round(viewRect().w / 90));
}

// Gradient padding around the screenshot when pretty mode is on. It is
// part of the canvas bitmap itself, so the preview and the copied image
// are the same pixels by construction.
function margin() {
  return pretty ? prettyPad() : 0;
}

// The canvas bitmap is view-sized (plus pretty padding), this only picks
// its display size so it fits the stage.
function layout() {
  if (!image) return;
  const availW = stage.clientWidth - 32;
  const availH = stage.clientHeight - 32;
  const scale = Math.min(availW / canvas.width, availH / canvas.height, 1);
  canvas.style.width = Math.max(1, Math.floor(canvas.width * scale)) + "px";
  canvas.style.height = Math.max(1, Math.floor(canvas.height * scale)) + "px";
}
window.addEventListener("resize", layout);

function gradientColors() {
  const g = GRADIENTS[gradientIndex];
  return g.auto ? autoColors : g;
}

function gradientCss(g) {
  return "linear-gradient(135deg, " + g.from + ", " + g.to + ")";
}

function syncPrettyUi() {
  prettyBtn.classList.toggle("active", pretty);
  gradientsEl.classList.toggle("visible", pretty);
  for (const [i, el] of [...gradientsEl.children].entries()) {
    el.classList.toggle("active", i === gradientIndex);
  }
  if (image) {
    setCanvasSize();
    redraw();
  }
}

GRADIENTS.forEach((g, i) => {
  const b = document.createElement("span");
  b.className = "swatch";
  b.title = g.name;
  b.style.background = gradientCss(g.auto ? autoColors : g);
  b.addEventListener("click", () => {
    gradientIndex = i;
    localStorage.setItem("prettyGradient", String(i));
    syncPrettyUi();
  });
  gradientsEl.appendChild(b);
});
syncPrettyUi();

function drawShape(s) {
  const g = ctx;
  g.strokeStyle = s.color;
  g.fillStyle = s.color;
  g.lineWidth = lineWidth;
  g.lineJoin = "round";
  g.lineCap = "round";
  if (s.kind === "rect") {
    g.strokeRect(
      Math.min(s.x0, s.x1),
      Math.min(s.y0, s.y1),
      Math.abs(s.x1 - s.x0),
      Math.abs(s.y1 - s.y0)
    );
  } else if (s.kind === "arrow") {
    const angle = Math.atan2(s.y1 - s.y0, s.x1 - s.x0);
    const head = Math.max(lineWidth * 3.5, 14);
    // Stop the shaft short so it does not poke past the head.
    const endX = s.x1 - head * 0.6 * Math.cos(angle);
    const endY = s.y1 - head * 0.6 * Math.sin(angle);
    g.beginPath();
    g.moveTo(s.x0, s.y0);
    g.lineTo(endX, endY);
    g.stroke();
    g.beginPath();
    g.moveTo(s.x1, s.y1);
    g.lineTo(s.x1 - head * Math.cos(angle - 0.45), s.y1 - head * Math.sin(angle - 0.45));
    g.lineTo(s.x1 - head * Math.cos(angle + 0.45), s.y1 - head * Math.sin(angle + 0.45));
    g.closePath();
    g.fill();
  } else if (s.kind === "blur") {
    const x = Math.min(s.x0, s.x1);
    const y = Math.min(s.y0, s.y1);
    const w = Math.abs(s.x1 - s.x0);
    const h = Math.abs(s.y1 - s.y0);
    if (w < 2 || h < 2) return;
    // Mosaic: shrink the region, then scale it back up with smoothing
    // off. Blurs render below annotations, so they only ever see image
    // content and other blurs.
    const block = Math.max(6, Math.round(image.naturalWidth / 120));
    const sw = Math.max(1, Math.round(w / block));
    const sh = Math.max(1, Math.round(h / block));
    scratch.width = sw;
    scratch.height = sh;
    const v = viewRect();
    const m = margin();
    // Source coordinates are raw canvas pixels, unaffected by the crop
    // translation, so shift by the crop origin and pretty padding here.
    scratch.getContext("2d").drawImage(canvas, x - v.x + m, y - v.y + m, w, h, 0, 0, sw, sh);
    g.imageSmoothingEnabled = false;
    g.drawImage(scratch, 0, 0, sw, sh, x, y, w, h);
    g.imageSmoothingEnabled = true;
  } else if (s.kind === "crop") {
    // Selection preview only: dim everything outside the dragged rect.
    const v = viewRect();
    const x = Math.min(s.x0, s.x1);
    const y = Math.min(s.y0, s.y1);
    const w = Math.abs(s.x1 - s.x0);
    const h = Math.abs(s.y1 - s.y0);
    g.save();
    g.fillStyle = "rgba(0, 0, 0, 0.5)";
    g.beginPath();
    g.rect(v.x, v.y, v.w, v.h);
    g.rect(x, y, w, h);
    g.fill("evenodd");
    g.strokeStyle = "#fff";
    g.lineWidth = Math.max(2, lineWidth / 2);
    g.setLineDash([lineWidth, lineWidth]);
    g.strokeRect(x, y, w, h);
    g.restore();
  }
}

function setCanvasSize() {
  const v = viewRect();
  const m = margin();
  canvas.width = v.w + m * 2;
  canvas.height = v.h + m * 2;
  layout();
}

function redraw() {
  if (!image) return;
  const v = viewRect();
  const m = margin();
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.clearRect(0, 0, canvas.width, canvas.height);

  if (pretty) {
    const grad = gradientColors();
    const g = ctx.createLinearGradient(0, 0, canvas.width, canvas.height);
    g.addColorStop(0, grad.from);
    g.addColorStop(1, grad.to);
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    // Drop shadow under the screenshot card, before the clip so the
    // clip cannot cut it off.
    ctx.save();
    ctx.shadowColor = "rgba(0, 0, 0, 0.4)";
    ctx.shadowBlur = m * 0.45;
    ctx.shadowOffsetY = m * 0.18;
    ctx.fillStyle = "#000";
    roundedRect(ctx, m, m, v.w, v.h, prettyRadius());
    ctx.fill();
    ctx.restore();
  }

  // The screenshot with blurs baked in (they redact image content) gets
  // the rounded clip in pretty mode. Annotations render afterwards,
  // above the border, so a corner can never slice an arrow and edge
  // marks overhang onto the gradient.
  ctx.save();
  if (pretty) {
    roundedRect(ctx, m, m, v.w, v.h, prettyRadius());
    ctx.clip();
  }
  ctx.translate(m - v.x, m - v.y);
  ctx.drawImage(image, 0, 0);
  for (const s of shapes) if (s.kind === "blur") drawShape(s);
  if (draft && draft.kind === "blur") drawShape(draft);
  ctx.restore();

  ctx.save();
  ctx.translate(m - v.x, m - v.y);
  for (const s of shapes) if (s.kind !== "blur") drawShape(s);
  if (draft && draft.kind !== "blur") drawShape(draft);
  ctx.restore();
}

// Mouse position in full-image pixels, the canvas is CSS-scaled to fit
// and shows the cropped region inside the pretty padding.
function canvasPos(e) {
  const r = canvas.getBoundingClientRect();
  const v = viewRect();
  const m = margin();
  return {
    x: (e.clientX - r.left) * (canvas.width / r.width) - m + v.x,
    y: (e.clientY - r.top) * (canvas.height / r.height) - m + v.y,
  };
}

function setTool(next) {
  tool = next;
  for (const el of document.querySelectorAll(".tool[data-tool]")) {
    el.classList.toggle("active", el.dataset.tool === tool);
  }
  canvas.classList.toggle("armed", tool !== null);
}

for (const el of document.querySelectorAll(".tool[data-tool]")) {
  el.addEventListener("click", () => {
    const picked = el.dataset.tool;
    setTool(tool === picked ? null : picked);
  });
}

prettyBtn.addEventListener("click", () => {
  pretty = !pretty;
  localStorage.setItem("prettyOn", pretty ? "1" : "0");
  syncPrettyUi();
});

canvas.addEventListener("mousedown", (e) => {
  if (!tool || !image || e.button !== 0) return;
  const p = canvasPos(e);
  draft = { kind: tool, color: colorInput.value, x0: p.x, y0: p.y, x1: p.x, y1: p.y };
});

window.addEventListener("mousemove", (e) => {
  if (!draft) return;
  const p = canvasPos(e);
  draft.x1 = p.x;
  draft.y1 = p.y;
  redraw();
});

function applyCrop(d) {
  // Clamp to the visible region, the drag can leave the canvas.
  const v = viewRect();
  const x0 = Math.max(v.x, Math.min(d.x0, d.x1));
  const y0 = Math.max(v.y, Math.min(d.y0, d.y1));
  const x1 = Math.min(v.x + v.w, Math.max(d.x0, d.x1));
  const y1 = Math.min(v.y + v.h, Math.max(d.y0, d.y1));
  const w = Math.round(x1 - x0);
  const h = Math.round(y1 - y0);
  if (w < 10 || h < 10) return;
  history.push({ kind: "crop", prev: crop });
  crop = { x: Math.round(x0), y: Math.round(y0), w, h };
  setCanvasSize();
  setTool(null);
}

window.addEventListener("mouseup", () => {
  if (!draft) return;
  const tiny = Math.abs(draft.x1 - draft.x0) < 3 && Math.abs(draft.y1 - draft.y0) < 3;
  if (draft.kind === "crop") {
    if (!tiny) applyCrop(draft);
  } else if (!tiny) {
    shapes.push(draft);
    history.push({ kind: "shape" });
  }
  draft = null;
  redraw();
});

function undo() {
  const entry = history.pop();
  if (!entry) return;
  if (entry.kind === "shape") {
    shapes.pop();
  } else if (entry.kind === "crop") {
    crop = entry.prev;
    setCanvasSize();
  }
  redraw();
}
document.getElementById("undoBtn").addEventListener("click", undo);

function roundedRect(c, x, y, w, h, r) {
  c.beginPath();
  c.moveTo(x + r, y);
  c.arcTo(x + w, y, x + w, y + h, r);
  c.arcTo(x + w, y + h, x, y + h, r);
  c.arcTo(x, y + h, x, y, r);
  c.arcTo(x, y, x + w, y, r);
  c.closePath();
}

// What you see is what gets copied: the working canvas already contains
// the pretty framing when it is on.
function copyAndClose() {
  if (!image) return;
  const data = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
  const payload = new Uint8Array(8 + data.length);
  const view = new DataView(payload.buffer);
  view.setUint32(0, canvas.width, true);
  view.setUint32(4, canvas.height, true);
  payload.set(data, 8);
  invoke("copy_annotated", payload).catch((e) => console.error("copy failed", e));
}
document.getElementById("copyBtn").addEventListener("click", copyAndClose);

document.addEventListener("keydown", (e) => {
  const mod = isMac ? e.metaKey : e.ctrlKey;
  if (mod && e.key.toLowerCase() === "c") {
    e.preventDefault();
    copyAndClose();
  } else if (mod && e.key.toLowerCase() === "z") {
    e.preventDefault();
    undo();
  } else if (e.key === "Escape") {
    if (draft) {
      draft = null;
      redraw();
    } else {
      invoke("close_editor");
    }
  }
});
