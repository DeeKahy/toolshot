import * as geo from "./geometry.js";
import { History } from "./history.js";
import { normalizeRect, arrowHead } from "./shapes.js";

const invoke = window.__TAURI__.core.invoke;
const isMac = navigator.platform.toUpperCase().includes("MAC");
if (!isMac) {
  for (const el of document.querySelectorAll(".modKey")) el.textContent = "Ctrl";
}

const canvas = document.getElementById("shot");
const ctx = canvas.getContext("2d");
const colorInput = document.getElementById("color");
const stage = document.getElementById("stage");
const prettyBtn = document.getElementById("prettyBtn");
const gradientsEl = document.getElementById("gradients");
const zoomBtn = document.getElementById("zoomBtn");
const scaleSelect = document.getElementById("exportScale");
const undoBtn = document.getElementById("undoBtn");
const redoBtn = document.getElementById("redoBtn");
const thicknessSelect = document.getElementById("thickness");

const GRADIENTS = [
  { name: "Auto (from screenshot)", auto: true },
  { name: "Violet", from: "#667eea", to: "#764ba2" },
  { name: "Ocean", from: "#36d1dc", to: "#5b86e5" },
  { name: "Sunset", from: "#ff9966", to: "#ff5e62" },
  { name: "Forest", from: "#11998e", to: "#38ef7d" },
  { name: "Candy", from: "#fc5c7d", to: "#6a82fb" },
  { name: "Graphite", from: "#485563", to: "#29323c" },
];

// Shapes that redact image content: they render below annotations and,
// in pretty mode, inside the rounded clip.
const REDACTION = new Set(["blur", "blackout"]);
// Tools that act on a single click rather than a drag.
const CLICK_TOOLS = new Set(["text", "badge", "eyedropper"]);

let image = null;         // the captured screenshot
let sampleCtx = null;     // offscreen copy of the raw image, for the eyedropper
let tool = null;          // active tool id, or null
const shapes = [];        // committed annotations, in full-image coordinates
let draft = null;         // shape being dragged right now
let crop = null;          // {x, y, w, h} visible region, null = full image
let lineWidth = 6;        // current stroke width in image pixels, see updateLineWidth
const history = new History();

// Line thickness is a multiplier on a size-proportional base stroke, so
// the same setting looks proportionally identical on a small and a large
// capture (a big screenshot gets thicker lines, not hairlines). Each
// shape snapshots the resolved width when created, so changing this only
// affects new annotations, never ones already drawn.
const THICKNESS = [
  { name: "Thin", mult: 2 },
  { name: "Medium", mult: 3.5 },
  { name: "Thick", mult: 5.5 },
  { name: "Extra", mult: 8.5 },
];
let thicknessIndex = parseInt(localStorage.getItem("thickness") || "1", 10);
if (!(thicknessIndex >= 0 && thicknessIndex < THICKNESS.length)) thicknessIndex = 1;

function updateLineWidth() {
  if (!image) return;
  const base = geo.strokeWidth(image.naturalWidth); // proportional to capture size
  lineWidth = Math.max(2, Math.round(base * THICKNESS[thicknessIndex].mult));
}

// Text and badges scale off the same current stroke width so thickness
// drives every tool.
function textSizeFor(width) {
  return Math.max(16, Math.round(width * 4));
}
function badgeRadiusFor(width) {
  return Math.max(width * 2.4, 14);
}
const scratch = document.createElement("canvas"); // downscale buffer for blur

// Pretty mode settings survive across captures, the editor window is
// recreated for every capture so plain variables would reset each time.
let pretty = localStorage.getItem("prettyOn") === "1";
let gradientIndex = parseInt(localStorage.getItem("prettyGradient") || "0", 10);
if (!(gradientIndex >= 0 && gradientIndex < GRADIENTS.length)) gradientIndex = 0;

// Quick-pick palette. Red leads and is the default annotation color; the
// rest are high-contrast staples so most captures need no trip to the OS
// color picker.
const PRESET_COLORS = [
  "#ff3b30", // red
  "#ff9500", // orange
  "#ffcc00", // yellow
  "#34c759", // green
  "#007aff", // blue
  "#af52de", // purple
  "#000000", // black
  "#ffffff", // white
];
const presetsEl = document.getElementById("colorPresets");

const savedColor = localStorage.getItem("annotationColor");
if (/^#[0-9a-f]{6}$/i.test(savedColor || "")) colorInput.value = savedColor;

function syncColorUi() {
  const cur = colorInput.value.toLowerCase();
  for (const el of presetsEl.children) {
    el.classList.toggle("active", el.dataset.color === cur);
  }
}

function setColor(c) {
  colorInput.value = c;
  localStorage.setItem("annotationColor", c);
  syncColorUi();
}

for (const c of PRESET_COLORS) {
  const b = document.createElement("span");
  b.className = "color-preset";
  b.dataset.color = c.toLowerCase();
  b.style.background = c;
  b.title = c;
  b.addEventListener("click", () => setColor(c));
  presetsEl.appendChild(b);
}

colorInput.addEventListener("input", syncColorUi);
colorInput.addEventListener("change", () => {
  localStorage.setItem("annotationColor", colorInput.value);
  syncColorUi();
});
syncColorUi();

thicknessSelect.value = String(thicknessIndex);
thicknessSelect.addEventListener("change", () => {
  thicknessIndex = parseInt(thicknessSelect.value, 10) || 0;
  localStorage.setItem("thickness", String(thicknessIndex));
  updateLineWidth();
});

// Shapes keep full-image coordinates even after cropping, the canvas is
// just translated by the crop origin when drawing.
function viewRect() {
  return geo.viewRect(crop, image.naturalWidth, image.naturalHeight);
}

// The capture PNG now arrives as raw bytes over binary IPC (no base64),
// so a fullscreen retina shot opens without the JSON detour.
invoke("get_capture_png")
  .then((buf) => {
    const blob = new Blob([new Uint8Array(buf)], { type: "image/png" });
    const url = URL.createObjectURL(blob);
    const img = new Image();
    img.onload = () => {
      image = img;
      updateLineWidth();
      buildSampleCanvas();
      computeAutoColors();
      setCanvasSize();
      redraw();
      URL.revokeObjectURL(url);
    };
    img.src = url;
  })
  .catch((e) => console.error("failed to load capture", e));

// A clean copy of the raw screenshot, so the eyedropper reads original
// pixels regardless of annotations or pretty framing on the main canvas.
function buildSampleCanvas() {
  const s = document.createElement("canvas");
  s.width = image.naturalWidth;
  s.height = image.naturalHeight;
  const c = s.getContext("2d", { willReadFrequently: true });
  c.drawImage(image, 0, 0);
  sampleCtx = c;
}

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
function margin() {
  const v = viewRect();
  return geo.margin(pretty, v.w, v.h);
}

// View zoom and pan are display-only: they change the canvas CSS size and
// a translate, never the canvas bitmap, so the copied pixels are untouched.
// viewZoom is relative to the fit scale, 1 = the whole image fits the stage.
let viewZoom = 1;
let panX = 0;
let panY = 0;
let fitScale = 1;
const MAX_ZOOM = 12;

// The canvas bitmap is view-sized (plus pretty padding), this only picks
// its display size so it fits the stage, times the user zoom.
function layout() {
  if (!image) return;
  const availW = stage.clientWidth - 32;
  const availH = stage.clientHeight - 32;
  fitScale = geo.fitScale(canvas.width, canvas.height, availW, availH);
  applyView();
}
window.addEventListener("resize", layout);

function applyView() {
  const w = Math.max(1, canvas.width * fitScale * viewZoom);
  const h = Math.max(1, canvas.height * fitScale * viewZoom);
  panX = geo.clampPan(panX, w, stage.clientWidth);
  panY = geo.clampPan(panY, h, stage.clientHeight);
  canvas.style.width = w + "px";
  canvas.style.height = h + "px";
  canvas.style.transform = "translate(" + panX + "px, " + panY + "px)";
  zoomBtn.textContent =
    viewZoom === 1 ? "Fit" : Math.round(fitScale * viewZoom * 100) + "%";
}

// Zoom keeping the stage-center point fixed. The canvas is flex-centered,
// so scaling the pan by the same factor as the size does exactly that.
function zoomBy(factor) {
  const prev = viewZoom;
  viewZoom = Math.min(MAX_ZOOM, Math.max(1, viewZoom * factor));
  if (viewZoom === 1) {
    panX = 0;
    panY = 0;
  } else {
    panX *= viewZoom / prev;
    panY *= viewZoom / prev;
  }
  applyView();
}

// Zoom keeping the point under the cursor fixed.
function zoomAt(factor, cx, cy) {
  const r = canvas.getBoundingClientRect();
  const fx = (cx - r.left) / r.width;
  const fy = (cy - r.top) / r.height;
  const prev = viewZoom;
  viewZoom = Math.min(MAX_ZOOM, Math.max(1, viewZoom * factor));
  if (viewZoom === 1) {
    panX = 0;
    panY = 0;
  } else if (viewZoom !== prev) {
    const sr = stage.getBoundingClientRect();
    const w = canvas.width * fitScale * viewZoom;
    const h = canvas.height * fitScale * viewZoom;
    panX = geo.panToKeepCursor(cx, sr.left + sr.width / 2, fx, w);
    panY = geo.panToKeepCursor(cy, sr.top + sr.height / 2, fy, h);
  }
  applyView();
}

function resetView() {
  viewZoom = 1;
  panX = 0;
  panY = 0;
  applyView();
}

zoomBtn.addEventListener("click", resetView);

stage.addEventListener(
  "wheel",
  (e) => {
    if (!image) return;
    e.preventDefault();
    if (e.ctrlKey || e.metaKey) {
      // Mouse wheel with the modifier, or a trackpad pinch (which the
      // webview reports as ctrl+wheel).
      zoomAt(Math.exp(-e.deltaY * 0.0022), e.clientX, e.clientY);
    } else {
      panX -= e.deltaX;
      panY -= e.deltaY;
      applyView();
    }
  },
  { passive: false }
);

// Panning by drag: middle mouse anywhere, or hold Space and drag.
let panDrag = null;
let spaceHeld = false;

stage.addEventListener("mousedown", (e) => {
  if (!image) return;
  if (e.button === 1 || (spaceHeld && e.button === 0)) {
    e.preventDefault();
    panDrag = { x: e.clientX, y: e.clientY };
    stage.classList.add("panning");
  }
});

document.addEventListener("keyup", (e) => {
  if (e.key === " ") {
    spaceHeld = false;
    stage.classList.remove("pan-ready");
  }
});

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
  const w = s.width || lineWidth;
  g.strokeStyle = s.color;
  g.fillStyle = s.color;
  g.lineWidth = w;
  g.lineJoin = "round";
  g.lineCap = "round";
  if (s.kind === "rect") {
    const r = normalizeRect(s);
    g.strokeRect(r.x, r.y, r.w, r.h);
  } else if (s.kind === "arrow") {
    const head = Math.max(w * 3.5, 14);
    const a = arrowHead(s.x0, s.y0, s.x1, s.y1, head);
    g.beginPath();
    g.moveTo(s.x0, s.y0);
    g.lineTo(a.shaftEnd.x, a.shaftEnd.y);
    g.stroke();
    g.beginPath();
    g.moveTo(a.tip.x, a.tip.y);
    g.lineTo(a.left.x, a.left.y);
    g.lineTo(a.right.x, a.right.y);
    g.closePath();
    g.fill();
  } else if (s.kind === "pen") {
    if (s.points.length < 2) return;
    g.beginPath();
    g.moveTo(s.points[0].x, s.points[0].y);
    for (let i = 1; i < s.points.length; i++) g.lineTo(s.points[i].x, s.points[i].y);
    g.stroke();
  } else if (s.kind === "text") {
    drawText(s);
  } else if (s.kind === "badge") {
    drawBadge(s);
  } else if (s.kind === "blur") {
    drawBlur(s);
  } else if (s.kind === "blackout") {
    const r = normalizeRect(s);
    if (r.w < 2 || r.h < 2) return;
    g.fillStyle = "#000";
    g.fillRect(r.x, r.y, r.w, r.h);
  } else if (s.kind === "crop") {
    drawCropPreview(s);
  }
}

// A contrasting outline color: white for most fills so text pops on dark
// backgrounds, flipping to near-black when the text itself is very light
// so a white letter on a white wall does not vanish.
function outlineColor(hex) {
  const m = /^#?([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex || "");
  if (!m) return "rgba(255, 255, 255, 0.95)";
  const r = parseInt(m[1], 16), g = parseInt(m[2], 16), b = parseInt(m[3], 16);
  const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
  return lum > 0.7 ? "rgba(0, 0, 0, 0.9)" : "rgba(255, 255, 255, 0.95)";
}

function drawText(s) {
  const g = ctx;
  g.textBaseline = "top";
  g.font = s.size + "px -apple-system, Segoe UI, sans-serif";
  g.lineJoin = "round";
  g.strokeStyle = outlineColor(s.color);
  g.lineWidth = Math.max(2, s.size * 0.16);
  g.fillStyle = s.color;
  const lines = s.text.split("\n");
  const lineHeight = s.size * 1.25;
  lines.forEach((line, i) => {
    const y = s.y + i * lineHeight;
    // Stroke first, fill on top, so the halo sits behind the letters.
    g.strokeText(line, s.x, y);
    g.fillText(line, s.x, y);
  });
}

function drawBadge(s) {
  const g = ctx;
  const r = s.radius || badgeRadiusFor(s.width || lineWidth);

  // Speech-bubble tail: a triangle from two points on the circle to the
  // target, drawn first so the circle on top hides its base and only the
  // pointing part sticks out.
  if (s.tail) {
    const dir = Math.atan2(s.tail.y - s.y, s.tail.x - s.x);
    const perp = dir + Math.PI / 2;
    const baseHalf = r * 0.6;
    g.beginPath();
    g.fillStyle = s.color;
    g.moveTo(s.x + Math.cos(perp) * baseHalf, s.y + Math.sin(perp) * baseHalf);
    g.lineTo(s.x - Math.cos(perp) * baseHalf, s.y - Math.sin(perp) * baseHalf);
    g.lineTo(s.tail.x, s.tail.y);
    g.closePath();
    g.fill();
  }

  g.beginPath();
  g.fillStyle = s.color;
  g.arc(s.x, s.y, r, 0, Math.PI * 2);
  g.fill();
  g.fillStyle = "#fff";
  g.textBaseline = "middle";
  g.textAlign = "center";
  g.font = "bold " + Math.round(r * 1.15) + "px -apple-system, Segoe UI, sans-serif";
  g.fillText(String(s.n), s.x, s.y + r * 0.05);
  g.textAlign = "left";
}

function drawBlur(s) {
  const r = normalizeRect(s);
  if (r.w < 2 || r.h < 2) return;
  // Mosaic: shrink the region, then scale it back up with smoothing off.
  const block = Math.max(6, Math.round(image.naturalWidth / 120));
  const sw = Math.max(1, Math.round(r.w / block));
  const sh = Math.max(1, Math.round(r.h / block));
  scratch.width = sw;
  scratch.height = sh;
  const v = viewRect();
  const m = margin();
  // Source coordinates are raw canvas pixels, unaffected by the crop
  // translation, so shift by the crop origin and pretty padding here.
  scratch.getContext("2d").drawImage(canvas, r.x - v.x + m, r.y - v.y + m, r.w, r.h, 0, 0, sw, sh);
  ctx.imageSmoothingEnabled = false;
  ctx.drawImage(scratch, 0, 0, sw, sh, r.x, r.y, r.w, r.h);
  ctx.imageSmoothingEnabled = true;
}

function drawCropPreview(s) {
  const g = ctx;
  const v = viewRect();
  const r = normalizeRect(s);
  g.save();
  g.fillStyle = "rgba(0, 0, 0, 0.5)";
  g.beginPath();
  g.rect(v.x, v.y, v.w, v.h);
  g.rect(r.x, r.y, r.w, r.h);
  g.fill("evenodd");
  g.strokeStyle = "#fff";
  g.lineWidth = Math.max(2, lineWidth / 2);
  g.setLineDash([lineWidth, lineWidth]);
  g.strokeRect(r.x, r.y, r.w, r.h);
  g.restore();
}

function setCanvasSize() {
  const v = viewRect();
  const m = margin();
  canvas.width = v.w + m * 2;
  canvas.height = v.h + m * 2;
  // The bitmap just changed shape (crop, pretty toggle), a stale zoom
  // and pan would point at nothing recognizable.
  viewZoom = 1;
  panX = 0;
  panY = 0;
  layout();
  updateScaleTitle();
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
    roundedRect(ctx, m, m, v.w, v.h, geo.prettyRadius(v.w));
    ctx.fill();
    ctx.restore();
  }

  // The screenshot with redactions baked in (blur, blackout) gets the
  // rounded clip in pretty mode. Annotations render afterwards, above the
  // border, so a corner can never slice an arrow and edge marks overhang
  // onto the gradient.
  ctx.save();
  if (pretty) {
    roundedRect(ctx, m, m, v.w, v.h, geo.prettyRadius(v.w));
    ctx.clip();
  }
  ctx.translate(m - v.x, m - v.y);
  ctx.drawImage(image, 0, 0);
  for (const s of shapes) if (REDACTION.has(s.kind)) drawShape(s);
  if (draft && REDACTION.has(draft.kind)) drawShape(draft);
  ctx.restore();

  ctx.save();
  ctx.translate(m - v.x, m - v.y);
  for (const s of shapes) if (!REDACTION.has(s.kind)) drawShape(s);
  if (draft && !REDACTION.has(draft.kind)) drawShape(draft);
  ctx.restore();
}

// Mouse position in full-image pixels, the canvas is CSS-scaled to fit
// and shows the cropped region inside the pretty padding.
function canvasPos(e) {
  const r = canvas.getBoundingClientRect();
  const v = viewRect();
  return geo.canvasToImage(e.clientX, e.clientY, r, canvas.width, canvas.height, margin(), v.x, v.y);
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

// --- History-backed edits --------------------------------------------

function syncHistoryButtons() {
  undoBtn.classList.toggle("disabled", !history.canUndo());
  redoBtn.classList.toggle("disabled", !history.canRedo());
}
syncHistoryButtons();

function addShape(shape) {
  history.commit(
    () => shapes.push(shape),
    () => {
      const i = shapes.indexOf(shape);
      if (i >= 0) shapes.splice(i, 1);
    }
  );
  syncHistoryButtons();
  redraw();
}

function commitCrop(next) {
  const prev = crop;
  history.commit(
    () => {
      crop = next;
      setCanvasSize();
    },
    () => {
      crop = prev;
      setCanvasSize();
    }
  );
  syncHistoryButtons();
  redraw();
}

function undo() {
  if (history.undo()) {
    syncHistoryButtons();
    redraw();
  }
}

function redo() {
  if (history.redo()) {
    syncHistoryButtons();
    redraw();
  }
}

undoBtn.addEventListener("click", undo);
redoBtn.addEventListener("click", redo);

// --- Click tools: text, badge, eyedropper ----------------------------

function nextBadge() {
  let n = 0;
  for (const s of shapes) if (s.kind === "badge" && s.n > n) n = s.n;
  return n + 1;
}

function eyedrop(p) {
  if (!sampleCtx) return;
  const x = Math.round(p.x);
  const y = Math.round(p.y);
  if (x < 0 || y < 0 || x >= image.naturalWidth || y >= image.naturalHeight) return;
  const d = sampleCtx.getImageData(x, y, 1, 1).data;
  const hex = "#" + [d[0], d[1], d[2]].map((v) => v.toString(16).padStart(2, "0")).join("");
  colorInput.value = hex;
  localStorage.setItem("annotationColor", hex);
  setTool(null); // one-shot, like most eyedroppers
}

// Inline text editing: a textarea floats over the click point while you
// type, then bakes into a text shape on commit. Positioned in client
// space and scaled to match the canvas so it looks like the final pixels.
let textEditor = null;

function startText(clientX, clientY, p) {
  commitText(); // bank any text already being typed before opening a new box
  const size = textSizeFor(lineWidth); // image-space px
  const displayScale = fitScale * viewZoom;

  const ta = document.createElement("textarea");
  ta.className = "text-editor";
  ta.rows = 1;
  ta.style.left = clientX + "px";
  ta.style.top = clientY + "px";
  ta.style.color = colorInput.value;
  ta.style.font = size * displayScale + "px -apple-system, Segoe UI, sans-serif";
  ta.style.lineHeight = size * displayScale * 1.25 + "px";
  // Preview the same contrasting halo the committed text will get.
  const halo = outlineColor(colorInput.value);
  ta.style.textShadow =
    `-1px -1px 0 ${halo}, 1px -1px 0 ${halo}, -1px 1px 0 ${halo}, 1px 1px 0 ${halo}`;
  document.body.appendChild(ta);

  // `ready` guards the blur handler: focusing inside a mousedown can be
  // undone by the browser's own focus handling, and that spurious blur
  // would otherwise commit an empty box and close the editor instantly.
  textEditor = { el: ta, x: p.x, y: p.y, size, color: colorInput.value, ready: false };

  ta.addEventListener("keydown", (e) => {
    e.stopPropagation(); // keep editor shortcuts (and Space) from firing while typing
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      commitText();
    } else if (e.key === "Escape") {
      e.preventDefault();
      cancelText();
    }
  });
  // Grow the box to fit as you type so long lines stay visible.
  ta.addEventListener("input", () => {
    ta.style.width = "auto";
    ta.style.height = "auto";
    ta.style.width = ta.scrollWidth + 4 + "px";
    ta.style.height = ta.scrollHeight + "px";
  });
  ta.addEventListener("blur", () => {
    if (textEditor && textEditor.ready) commitText();
  });

  // Focus after the triggering mouse event settles, then arm the blur
  // commit. Without the delay the focus does not stick.
  setTimeout(() => {
    ta.focus();
    if (textEditor && textEditor.el === ta) textEditor.ready = true;
  }, 0);
}

function commitText() {
  if (!textEditor) return;
  const { el, x, y, size, color } = textEditor;
  const text = el.value.replace(/\s+$/, "");
  textEditor = null; // clear first so the blur handler does not re-enter
  el.remove();
  if (text) addShape({ kind: "text", color, x, y, size, text, width: lineWidth });
}

function cancelText() {
  if (!textEditor) return;
  const el = textEditor.el;
  textEditor = null;
  el.remove();
}

// --- Pointer handling ------------------------------------------------

canvas.addEventListener("mousedown", (e) => {
  if (!tool || !image || e.button !== 0 || spaceHeld || panDrag) return;
  const p = canvasPos(e);
  if (tool === "eyedropper") {
    eyedrop(p);
    return;
  }
  if (tool === "badge") {
    // Press places the circle; dragging out pulls a speech-bubble tail
    // toward the release point. A plain click stays a tailless badge.
    draft = {
      kind: "badge",
      color: colorInput.value,
      x: p.x,
      y: p.y,
      n: nextBadge(),
      width: lineWidth,
      tail: null,
    };
    return;
  }
  if (tool === "text") {
    // Stop the browser from stealing focus back off the editor we open.
    e.preventDefault();
    startText(e.clientX, e.clientY, p);
    return;
  }
  if (tool === "pen") {
    draft = { kind: "pen", color: colorInput.value, points: [{ x: p.x, y: p.y }], width: lineWidth };
    return;
  }
  draft = { kind: tool, color: colorInput.value, x0: p.x, y0: p.y, x1: p.x, y1: p.y, width: lineWidth };
});

window.addEventListener("mousemove", (e) => {
  if (panDrag) {
    panX += e.clientX - panDrag.x;
    panY += e.clientY - panDrag.y;
    panDrag = { x: e.clientX, y: e.clientY };
    applyView();
    return;
  }
  if (!draft) return;
  const p = canvasPos(e);
  if (draft.kind === "pen") {
    draft.points.push({ x: p.x, y: p.y });
  } else if (draft.kind === "badge") {
    // Only grow a tail once the drag clears the circle, so a small
    // wobble on a click does not sprout one.
    const r = badgeRadiusFor(draft.width);
    const far = Math.hypot(p.x - draft.x, p.y - draft.y) > r * 1.1;
    draft.tail = far ? { x: p.x, y: p.y } : null;
  } else {
    draft.x1 = p.x;
    draft.y1 = p.y;
  }
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
  commitCrop({ x: Math.round(x0), y: Math.round(y0), w, h });
  setTool(null);
}

window.addEventListener("mouseup", () => {
  if (panDrag) {
    panDrag = null;
    stage.classList.remove("panning");
    return;
  }
  if (!draft) return;
  if (draft.kind === "pen") {
    const d = draft;
    draft = null;
    if (d.points.length > 1) addShape(d);
    else redraw();
    return;
  }
  if (draft.kind === "badge") {
    // A badge is valid with or without a drag, so always commit it.
    const d = draft;
    draft = null;
    addShape(d);
    return;
  }
  const tiny = Math.abs(draft.x1 - draft.x0) < 3 && Math.abs(draft.y1 - draft.y0) < 3;
  if (draft.kind === "crop") {
    const d = draft;
    draft = null;
    if (!tiny) applyCrop(d);
    else redraw();
    return;
  }
  const d = draft;
  draft = null;
  if (!tiny) addShape(d);
  else redraw();
});

function roundedRect(c, x, y, w, h, r) {
  c.beginPath();
  c.moveTo(x + r, y);
  c.arcTo(x + w, y, x + w, y + h, r);
  c.arcTo(x + w, y + h, x, y + h, r);
  c.arcTo(x, y + h, x, y, r);
  c.arcTo(x, y, x + w, y, r);
  c.closePath();
}

// Output scale, applied only at copy time so the working canvas stays 1:1
// with the capture. Never persisted, it is a per-shot decision.
function exportScale() {
  return geo.clampExportScale(parseFloat(scaleSelect.value), canvas.width, canvas.height);
}

function updateScaleTitle() {
  if (!image) return;
  const s = exportScale();
  scaleSelect.title =
    "Copied size: " +
    Math.round(canvas.width * s) + " x " + Math.round(canvas.height * s) + " px";
}
scaleSelect.addEventListener("change", updateScaleTitle);

// The exported pixels: the working canvas, scaled if the user chose a
// non-1x output. Nearest neighbor going up so a small capture stays
// crisp, smoothing going down. Returns {data, w, h}.
function exportPixels() {
  const s = exportScale();
  if (s === 1) {
    return { data: ctx.getImageData(0, 0, canvas.width, canvas.height).data, w: canvas.width, h: canvas.height };
  }
  const out = document.createElement("canvas");
  out.width = Math.max(1, Math.round(canvas.width * s));
  out.height = Math.max(1, Math.round(canvas.height * s));
  const oc = out.getContext("2d");
  oc.imageSmoothingEnabled = s < 1;
  oc.imageSmoothingQuality = "high";
  oc.drawImage(canvas, 0, 0, out.width, out.height);
  return { data: oc.getImageData(0, 0, out.width, out.height).data, w: out.width, h: out.height };
}

// Pack width/height header + RGBA bytes into one payload for the raw IPC.
function packPixels(px) {
  const payload = new Uint8Array(8 + px.data.length);
  const view = new DataView(payload.buffer);
  view.setUint32(0, px.w, true);
  view.setUint32(4, px.h, true);
  payload.set(px.data, 8);
  return payload;
}

function copyAndClose() {
  if (!image) return;
  commitText();
  invoke("copy_annotated", packPixels(exportPixels())).catch((e) => console.error("copy failed", e));
}
document.getElementById("copyBtn").addEventListener("click", copyAndClose);

function saveToFile() {
  if (!image) return;
  commitText();
  const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
  invoke("save_annotated", packPixels(exportPixels()), {
    headers: { "x-filename": "toolshot-" + stamp + ".png" },
  }).catch((e) => console.error("save failed", e));
}
document.getElementById("saveBtn").addEventListener("click", saveToFile);

document.addEventListener("keydown", (e) => {
  const mod = isMac ? e.metaKey : e.ctrlKey;
  if (e.key === " ") {
    // preventDefault also keeps Space from clicking a focused button.
    e.preventDefault();
    if (!spaceHeld) {
      spaceHeld = true;
      stage.classList.add("pan-ready");
    }
  } else if (mod && e.key.toLowerCase() === "c") {
    e.preventDefault();
    copyAndClose();
  } else if (mod && e.key.toLowerCase() === "s") {
    e.preventDefault();
    saveToFile();
  } else if (mod && e.shiftKey && e.key.toLowerCase() === "z") {
    e.preventDefault();
    redo();
  } else if (mod && e.key.toLowerCase() === "y") {
    e.preventDefault();
    redo();
  } else if (mod && e.key.toLowerCase() === "z") {
    e.preventDefault();
    undo();
  } else if (mod && (e.key === "=" || e.key === "+")) {
    e.preventDefault();
    zoomBy(1.25);
  } else if (mod && e.key === "-") {
    e.preventDefault();
    zoomBy(1 / 1.25);
  } else if (mod && e.key === "0") {
    e.preventDefault();
    resetView();
  } else if (e.key === "Escape") {
    if (textEditor) {
      cancelText();
    } else if (draft) {
      draft = null;
      redraw();
    } else if (tool) {
      setTool(null);
    } else {
      invoke("close_editor");
    }
  }
});
