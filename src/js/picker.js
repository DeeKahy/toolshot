const invoke = window.__TAURI__.core.invoke;
const isMac = navigator.platform.toUpperCase().includes("MAC");
if (!isMac) document.getElementById("modKey").textContent = "Ctrl";

let hexValue = "";

function toHsl(r, g, b) {
  r /= 255; g /= 255; b /= 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h;
  switch (max) {
    case r: h = (g - b) / d + (g < b ? 6 : 0); break;
    case g: h = (b - r) / d + 2; break;
    default: h = (r - g) / d + 4;
  }
  return [h * 60, s, l];
}

function toHsb(r, g, b) {
  r /= 255; g /= 255; b /= 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  const v = max, d = max - min;
  const s = max === 0 ? 0 : d / max;
  if (d === 0) return [0, s, v];
  let h;
  switch (max) {
    case r: h = (g - b) / d + (g < b ? 6 : 0); break;
    case g: h = (b - r) / d + 2; break;
    default: h = (r - g) / d + 4;
  }
  return [h * 60, s, v];
}

function addRow(name, value, isDefault) {
  const row = document.createElement("div");
  row.className = "row" + (isDefault ? " default" : "");
  const n = document.createElement("span");
  n.className = "name";
  n.textContent = name;
  const v = document.createElement("span");
  v.className = "value";
  v.textContent = value;
  const c = document.createElement("span");
  c.className = "copied";
  c.textContent = "copied";
  row.appendChild(n);
  row.appendChild(v);
  row.appendChild(c);
  row.addEventListener("click", () => {
    invoke("copy_text", { text: value }).then(() => {
      c.style.opacity = "1";
      setTimeout(() => { c.style.opacity = "0"; }, 900);
    });
  });
  document.getElementById("rows").appendChild(row);
}

invoke("get_picked_color")
  .then(([r, g, b]) => {
    hexValue = "#" + [r, g, b].map((x) => x.toString(16).padStart(2, "0")).join("");
    document.getElementById("swatch").style.background = hexValue;

    const [h, s, l] = toHsl(r, g, b);
    const [hh, hs, hv] = toHsb(r, g, b);

    addRow("hex", hexValue, true);
    addRow("rgb", "rgb(" + r + ", " + g + ", " + b + ")", false);
    addRow("hsl", "hsl(" + Math.round(h) + ", " + Math.round(s * 100) + "%, " + Math.round(l * 100) + "%)", false);
    addRow("hsb", "hsb(" + Math.round(hh) + ", " + Math.round(hs * 100) + "%, " + Math.round(hv * 100) + "%)", false);
    addRow("swift", "Color(red: " + (r / 255).toFixed(3) + ", green: " + (g / 255).toFixed(3) + ", blue: " + (b / 255).toFixed(3) + ")", false);
  })
  .catch((e) => console.error("no color", e));

document.addEventListener("keydown", (e) => {
  const mod = isMac ? e.metaKey : e.ctrlKey;
  if (mod && e.key.toLowerCase() === "c") {
    e.preventDefault();
    invoke("copy_text", { text: hexValue }).then(() => invoke("close_color_popup"));
  } else if (e.key === "Escape") {
    invoke("close_color_popup");
  }
});
