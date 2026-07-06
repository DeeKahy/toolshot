const invoke = window.__TAURI__.core.invoke;
const bg = document.getElementById("bg");
const dim = document.getElementById("dim");
const highlight = document.getElementById("highlight");
const label = document.getElementById("label");
const guideV = document.getElementById("guideV");
const guideH = document.getElementById("guideH");
const selection = document.getElementById("selection");
const selectionSize = selection.querySelector(".size");
const loupe = document.getElementById("loupe");
const loupeCanvas = document.getElementById("loupeCanvas");
const loupeCtx = loupeCanvas.getContext("2d");
const loupeInfo = document.getElementById("loupeInfo");
const notice = document.getElementById("notice");
const hint = document.getElementById("hint");

const LOUPE_SIZE = 143;    // canvas px, must be SAMPLE * an odd zoom factor
const SAMPLE = 13;         // physical pixels shown across the loupe
const CELL = LOUPE_SIZE / SAMPLE;
const DRAG_THRESHOLD = 4;  // css px of movement before a click becomes a drag

let windows = [];
let current = null;
let done = false;
let screen = null;         // { canvas, ctx, scale } of the frozen frame
let dragStart = null;
let dragging = false;
// null until the backend answers, input is ignored before that so
// capture-mode visuals can never leak into picker mode.
let mode = null;           // "capture" or "pick" (color picker)

function showNotice(title, body) {
  notice.innerHTML = "";
  const h = document.createElement("h2");
  h.textContent = title;
  const p = document.createElement("div");
  p.innerHTML = body;
  notice.appendChild(h);
  notice.appendChild(p);
  notice.style.display = "block";
}

async function init() {
  // The window is only transparent on macOS. Elsewhere paint black so
  // an opaque window never flashes the webview's white default.
  if (!navigator.platform.toUpperCase().includes("MAC")) {
    document.documentElement.style.background = "#000";
  }
  mode = await invoke("get_overlay_mode").catch(() => "capture");
  if (mode === "pick") {
    // The frozen frame shows through untinted, the screen looks normal.
    hint.innerHTML = 'Click to pick a color &nbsp;&middot;&nbsp; <kbd>Esc</kbd> to cancel';
  } else {
    dim.style.display = "block";
  }

  const granted = await invoke("check_screen_permission").catch(() => false);
  if (!granted) {
    showNotice(
      "Screen Recording permission needed",
      "Enable it in System Settings &rarr; Privacy &amp; Security &rarr; " +
      "Screen &amp; System Audio Recording, then restart Toolshot.<br>" +
      "When running the raw binary from a terminal, the permission " +
      "belongs to the terminal app.<br><br>" +
      "<kbd>Esc</kbd> or click to close"
    );
    return;
  }

  // The overlay page can be ready before the screen freeze finishes,
  // so poll briefly instead of giving up on the first miss. The pixels
  // come as raw RGBA over binary IPC, no PNG or base64 involved.
  (async () => {
    let meta = null;
    for (let attempt = 0; attempt < 30 && !meta; attempt++) {
      meta = await invoke("get_screen_meta").catch(() => null);
      if (!meta) await new Promise((r) => setTimeout(r, 50));
    }
    if (!meta) {
      console.error("no frozen screen, loupe and drag disabled");
      return;
    }
    try {
      const buf = await invoke("get_screen_rgba");
      const data = new ImageData(new Uint8ClampedArray(buf), meta.width, meta.height);
      bg.width = meta.width;
      bg.height = meta.height;
      const ctx = bg.getContext("2d", { willReadFrequently: true });
      ctx.putImageData(data, 0, 0);
      bg.style.display = "block";
      screen = { canvas: bg, ctx, scale: meta.scale };
    } catch (e) {
      console.error("failed to load frozen screen", e);
    }
  })();

  if (mode === "pick") return;

  try {
    windows = await invoke("list_windows");
  } catch (e) {
    showNotice("Could not list windows", String(e) + "<br><br><kbd>Esc</kbd> or click to close");
    return;
  }
  if (windows.length === 0) {
    showNotice("No windows to capture", "Drag still works &nbsp;&middot;&nbsp; <kbd>Esc</kbd> or click to close");
  }
}
init();

function windowAt(x, y) {
  // The list arrives sorted topmost first, so the first hit wins.
  for (const w of windows) {
    if (x >= w.x && x < w.x + w.width && y >= w.y && y < w.y + w.height) {
      return w;
    }
  }
  return null;
}

function updateHighlight(x, y) {
  const w = windowAt(x, y);
  current = w;
  if (!w || dragging) {
    highlight.style.display = "none";
    label.style.display = "none";
    return;
  }
  highlight.style.display = "block";
  highlight.style.left = w.x + "px";
  highlight.style.top = w.y + "px";
  highlight.style.width = w.width + "px";
  highlight.style.height = w.height + "px";

  const name = w.title ? w.app_name + ": " + w.title : w.app_name;
  label.innerHTML = "";
  const nameSpan = document.createElement("span");
  nameSpan.textContent = name + "  ";
  const dims = document.createElement("span");
  dims.className = "dims";
  dims.textContent = w.width + "×" + w.height;
  label.appendChild(nameSpan);
  label.appendChild(dims);
  label.style.display = "block";
  label.style.left = Math.max(6, w.x + 8) + "px";
  label.style.top = Math.max(6, w.y - 30) + "px";
}

function updateGuides(x, y) {
  guideV.style.display = "block";
  guideH.style.display = "block";
  guideV.style.left = x + "px";
  guideH.style.top = y + "px";
}

function centerHex(px, py) {
  try {
    const d = screen.ctx.getImageData(px, py, 1, 1).data;
    return "#" + [d[0], d[1], d[2]].map((v) => v.toString(16).padStart(2, "0")).join("");
  } catch (e) {
    return "";
  }
}

function updateLoupe(x, y) {
  if (!screen) {
    loupe.style.display = "none";
    return;
  }
  const px = Math.floor(x * screen.scale);
  const py = Math.floor(y * screen.scale);
  const half = Math.floor(SAMPLE / 2);

  loupeCtx.imageSmoothingEnabled = false;
  loupeCtx.fillStyle = "#000";
  loupeCtx.fillRect(0, 0, LOUPE_SIZE, LOUPE_SIZE);
  loupeCtx.drawImage(
    screen.canvas,
    px - half, py - half, SAMPLE, SAMPLE,
    0, 0, LOUPE_SIZE, LOUPE_SIZE
  );

  // Pixel grid.
  loupeCtx.strokeStyle = "rgba(255, 255, 255, 0.13)";
  loupeCtx.lineWidth = 1;
  loupeCtx.beginPath();
  for (let i = 1; i < SAMPLE; i++) {
    loupeCtx.moveTo(i * CELL + 0.5, 0);
    loupeCtx.lineTo(i * CELL + 0.5, LOUPE_SIZE);
    loupeCtx.moveTo(0, i * CELL + 0.5);
    loupeCtx.lineTo(LOUPE_SIZE, i * CELL + 0.5);
  }
  loupeCtx.stroke();

  // Center pixel marker.
  const c = half * CELL;
  loupeCtx.strokeStyle = "#ff4d6a";
  loupeCtx.strokeRect(c + 0.5, c + 0.5, CELL - 1, CELL - 1);

  let info = px + ", " + py;
  const hex = centerHex(px, py);
  if (hex) info += "  " + hex;
  if (dragging && dragStart) {
    const w = Math.abs(x - dragStart.x);
    const h = Math.abs(y - dragStart.y);
    info += "  " + Math.round(w * screen.scale) + "×" + Math.round(h * screen.scale);
  }
  loupeInfo.textContent = info;

  // Keep the loupe out from under the cursor, flip near edges.
  const margin = 20;
  const total = LOUPE_SIZE + 30;
  let lx = x + margin;
  let ly = y + margin;
  if (lx + total > window.innerWidth) lx = x - margin - LOUPE_SIZE;
  if (ly + total > window.innerHeight) ly = y - margin - total;
  loupe.style.left = lx + "px";
  loupe.style.top = ly + "px";
  loupe.style.display = "block";
}

function updateSelection(x, y) {
  const left = Math.min(dragStart.x, x);
  const top = Math.min(dragStart.y, y);
  const w = Math.abs(x - dragStart.x);
  const h = Math.abs(y - dragStart.y);
  selection.style.left = left + "px";
  selection.style.top = top + "px";
  selection.style.width = w + "px";
  selection.style.height = h + "px";
  selection.style.display = "block";
  if (screen) {
    selectionSize.textContent =
      Math.round(w * screen.scale) + "×" + Math.round(h * screen.scale);
  } else {
    selectionSize.textContent = w + "×" + h;
  }
}

function resetDrag() {
  dragStart = null;
  dragging = false;
  selection.style.display = "none";
  dim.style.display = "block";
}

document.addEventListener("mousemove", (e) => {
  if (!mode) return;
  const x = e.clientX, y = e.clientY;
  if (mode === "pick") {
    updateLoupe(x, y);
    return;
  }
  if (dragStart && !dragging) {
    if (Math.abs(x - dragStart.x) > DRAG_THRESHOLD || Math.abs(y - dragStart.y) > DRAG_THRESHOLD) {
      if (screen) {
        dragging = true;
        highlight.style.display = "none";
        label.style.display = "none";
        // The selection undims itself via its box-shadow.
        dim.style.display = "none";
      }
    }
  }
  if (dragging) {
    updateSelection(x, y);
  } else {
    updateHighlight(x, y);
  }
  updateGuides(x, y);
  updateLoupe(x, y);
});

document.addEventListener("mousedown", (e) => {
  if (done || !mode) return;
  if (e.button !== 0) {
    invoke("cancel_overlay");
    return;
  }
  if (mode === "pick") {
    if (!screen) return;
    const px = Math.floor(e.clientX * screen.scale);
    const py = Math.floor(e.clientY * screen.scale);
    const d = screen.ctx.getImageData(px, py, 1, 1).data;
    done = true;
    invoke("pick_color", { r: d[0], g: d[1], b: d[2] }).catch((err) => {
      console.error("pick failed", err);
      done = false;
    });
    return;
  }
  dragStart = { x: e.clientX, y: e.clientY };
});

document.addEventListener("mouseup", (e) => {
  if (done || !mode || e.button !== 0 || !dragStart) return;

  if (dragging) {
    const x = Math.min(dragStart.x, e.clientX);
    const y = Math.min(dragStart.y, e.clientY);
    const w = Math.abs(e.clientX - dragStart.x);
    const h = Math.abs(e.clientY - dragStart.y);
    if (w < 3 || h < 3) {
      // Too small to be a real selection, treat it like a click.
      resetDrag();
      clickCapture();
      return;
    }
    done = true;
    invoke("capture_area", { x, y, width: w, height: h }).catch((err) => {
      console.error("area capture failed", err);
      done = false;
      resetDrag();
    });
    return;
  }

  dragStart = null;
  clickCapture();
});

function clickCapture() {
  if (!current) {
    invoke("cancel_overlay");
    return;
  }
  done = true;
  invoke("capture_window", { id: current.id }).catch((err) => {
    console.error("capture failed", err);
    done = false;
  });
}

document.addEventListener("contextmenu", (e) => e.preventDefault());

document.addEventListener("keydown", (e) => {
  if (e.key !== "Escape") return;
  if (dragging) {
    resetDrag();
  } else {
    invoke("cancel_overlay");
  }
});
