const invoke = window.__TAURI__.core.invoke;
const status = document.getElementById("status");
let recording = null; // the .bind element currently listening

function pretty(accel) {
  return accel
    .replaceAll("Key", "")
    .replaceAll("Digit", "")
    .replaceAll("Arrow", "");
}

function render(el, accel) {
  el.classList.remove("recording");
  if (accel) {
    el.textContent = pretty(accel);
    el.classList.remove("unset");
  } else {
    el.textContent = "Not set";
    el.classList.add("unset");
  }
}

let current = {};
invoke("get_settings").then((s) => {
  current = s;
  render(document.getElementById("bind-capture"), s.capture_shortcut);
  render(document.getElementById("bind-fullscreen"), s.fullscreen_shortcut);
  render(document.getElementById("bind-picker"), s.picker_shortcut);
});

const autostart = document.getElementById("autostart");
invoke("get_autostart").then((on) => { autostart.checked = on; });
autostart.addEventListener("change", () => {
  invoke("set_autostart", { enabled: autostart.checked }).catch((e) => {
    status.textContent = String(e);
    autostart.checked = !autostart.checked;
  });
});

const SITE = "https://deekahy.github.io/toolshot/";
const updateBtn = document.getElementById("updateBtn");
let appVersion = null;
let downloadReady = false;

invoke("get_app_version").then((v) => {
  appVersion = v;
  document.getElementById("version").textContent = "Version " + v;
});

function newer(a, b) {
  const pa = a.split(".").map(Number);
  const pb = b.split(".").map(Number);
  for (let i = 0; i < 3; i++) {
    if ((pa[i] || 0) > (pb[i] || 0)) return true;
    if ((pa[i] || 0) < (pb[i] || 0)) return false;
  }
  return false;
}

updateBtn.addEventListener("click", async () => {
  if (downloadReady) {
    invoke("open_url", { url: SITE });
    return;
  }
  updateBtn.textContent = "Checking...";
  try {
    const res = await fetch("https://api.github.com/repos/DeeKahy/toolshot/releases/latest");
    if (!res.ok) throw new Error("HTTP " + res.status);
    const release = await res.json();
    const latest = release.tag_name.replace(/^v/, "");
    if (appVersion && newer(latest, appVersion)) {
      updateBtn.textContent = "Get v" + latest;
      downloadReady = true;
    } else {
      updateBtn.textContent = "Up to date";
    }
  } catch (e) {
    updateBtn.textContent = "Check failed";
    status.textContent = "Update check failed: " + String(e);
  }
});

for (const el of document.querySelectorAll(".bind")) {
  el.addEventListener("click", () => {
    if (recording) render(recording, currentAccel(recording));
    recording = el;
    el.classList.add("recording");
    el.textContent = "Press keys...";
    status.textContent = "";
  });
}

function currentAccel(el) {
  return current[el.dataset.kind + "_shortcut"];
}

function keyOk(code) {
  return /^(Key[A-Z]|Digit[0-9]|F([1-9]|1[0-2])|Space|Comma|Period|Slash|Backslash|Semicolon|Quote|Backquote|Minus|Equal|Enter|Tab|Arrow(Up|Down|Left|Right))$/.test(code);
}

async function saveBinding(el, accel) {
  const kind = el.dataset.kind;
  try {
    await invoke("set_shortcut", { kind, accel });
    current[kind + "_shortcut"] = accel;
    render(el, accel);
  } catch (e) {
    status.textContent = "Could not register: " + String(e);
    render(el, currentAccel(el));
  }
}

document.addEventListener("keydown", (e) => {
  if (!recording) {
    if (e.key === "Escape") invoke("close_settings");
    return;
  }
  e.preventDefault();

  if (e.key === "Escape") {
    render(recording, currentAccel(recording));
    recording = null;
    return;
  }
  if (e.key === "Backspace" || e.key === "Delete") {
    saveBinding(recording, null);
    recording = null;
    return;
  }

  const mods = [];
  if (e.metaKey) mods.push("Cmd");
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (mods.length === 0 || !keyOk(e.code)) return; // keep listening

  const accel = mods.join("+") + "+" + e.code;
  const el = recording;
  recording = null;
  saveBinding(el, accel);
});
