.pragma library

// Pure helpers for the Omakboard bar widget and overlay. No QML types here, so node can test
// them (plugin/tests/model.test.js).

var PLUGIN_ID = "io.github.elberacasa.omakboard"

// The `omakboard devices --json` schema the bar widget understands.
var REPORT_SCHEMA = 1

// The `omakboard serve` protocol the overlay needs.
var PROTOCOL = 1

function parseJson(text) {
  try {
    return JSON.parse(text)
  } catch (e) {
    return null
  }
}

// ---- Bar widget ----------------------------------------------------------------------

// What the widget shows for one probe: `exitCode` of `omakboard devices --json` (-1 before the
// first run finishes) and its parsed output.
//   visible  whether to take a bar slot at all
//   active   drawn in the accent colour: a keyboard the user can reach
//   action   what a click does: "install" runs the plugin's installer, "open" the overlay
function indicatorState(exitCode, report) {
  var hidden = { visible: false, active: false, action: "open", tooltip: "" }
  if (exitCode === -1) return hidden
  if (exitCode === 127) {
    return {
      visible: true,
      active: false,
      action: "install",
      tooltip: "Omakboard is not installed yet.\nClick to run its installer."
    }
  }
  if (exitCode !== 0 || !report) {
    return {
      visible: true,
      active: false,
      action: "open",
      tooltip: "omakboard could not list keyboards (exit " + exitCode + ").\nClick for details."
    }
  }
  if (report.schema !== REPORT_SCHEMA) {
    return {
      visible: true,
      active: false,
      action: "install",
      tooltip: "The omakboard command and this plugin are out of step.\nClick to update the command."
    }
  }
  var devices = Array.isArray(report.devices) ? report.devices : []
  if (devices.length === 0) return hidden

  var d = devices[0]
  var lines = [d.name]
  lines.push(d.access === "ok" ? supportLine(d.support) : "Cannot reach its settings yet.")
  lines.push("Click to open.")
  return { visible: true, active: d.access === "ok", action: "open", tooltip: lines.join("\n") }
}

function supportLine(support) {
  if (support === "verified") return "Ready to tune."
  if (support === "untested") return "Untested model: changes ask for your consent first."
  return "Detected. Tuning arrives once its protocol is decoded."
}

// `script` quoted for a shell, so a plugin folder with spaces or quotes stays one word.
function shellQuote(script) {
  return "'" + String(script).replace(/'/g, "'\\''") + "'"
}

// A local path from a file: URL, e.g. Qt.resolvedUrl("../install.sh").
function localPath(url) {
  return decodeURIComponent(String(url).replace(/^file:\/\//, ""))
}

// The plugin's own installer in Omarchy's floating terminal, as the bar's update button runs
// omarchy-update. It is the reviewed script in this checkout, never a download piped to a shell.
function installArgv(installScript) {
  return ["omarchy-launch-floating-terminal-with-presentation", "bash " + shellQuote(installScript)]
}

function doctorArgv() {
  return ["omarchy-launch-floating-terminal-with-presentation", "omakboard doctor"]
}

function toggleArgv() {
  return ["omarchy-shell", "shell", "toggle", PLUGIN_ID, "{}"]
}

// What an open payload asks for, e.g. {"tab": "lighting"} or {"select": ["KeyW", "KeyA"]}.
// Unknown or malformed fields are ignored, so any payload opens the overlay.
function openRequest(payload) {
  var p = payload && typeof payload === "object" ? payload : {}
  var tab = p.tab === "lighting" || p.tab === "triggers" ? p.tab : null
  var select = Array.isArray(p.select) ? p.select.filter(function(c) { return typeof c === "string" }) : null
  if (typeof p.select === "string" && GROUPS[p.select]) select = GROUPS[p.select]
  return { tab: tab, select: select }
}

// ---- Server ---------------------------------------------------------------------------

function helperOutdated(state) {
  return !state || typeof state.protocol !== "number" || state.protocol < PROTOCOL
}

// Why `omakboard serve` stopped, for the overlay's setup card.
function errorMessage(stderr, exitCode) {
  if (exitCode === 127) return "The omakboard command is not installed or not on PATH."
  var lines = String(stderr || "").trim().split("\n").filter(function(l) { return l.trim() !== "" })
  if (lines.length > 0) return lines[lines.length - 1].replace(/^omakboard: /, "")
  return "omakboard serve stopped (exit " + exitCode + ")."
}

// A failure that means the keyboard can no longer be reached, so its controls cannot save.
function connectionLost(message) {
  var kind = setupKind(message)
  return kind === "install" || kind === "permission" || kind === "unplugged" || kind === "other-model"
}

// What stands between the overlay and the keyboard.
function setupKind(message) {
  var m = String(message || "")
  if (m === "") return ""
  if (/not installed|not on PATH/.test(m)) return "install"
  if (/permission denied/i.test(m)) return "permission"
  if (/no supported keyboard/i.test(m)) return "unplugged"
  if (/reports product/.test(m)) return "other-model"
  if (/another omakboard/.test(m)) return "busy"
  return "error"
}

// Words and the one action for each setup state. `action` is "install", "doctor" or "".
function setupCopy(kind, message) {
  switch (kind) {
  case "install":
    return { title: "Install the omakboard command", body: "The overlay talks to your keyboard through the omakboard command, which is not installed yet. Its installer downloads it, checks it, and asks for your password once to let your user reach the keyboard.", action: "install" }
  case "outdated":
    return { title: "Update the omakboard command", body: "This plugin needs a newer omakboard command than the one installed.", action: "install" }
  case "permission":
    return { title: "Let your user reach the keyboard", body: "Your keyboard is plugged in, but its settings interface is closed to your user. The installer adds the udev rule that opens it; then replug the keyboard.", action: "install" }
  case "unplugged":
    return { title: "Plug in your keyboard", body: "No supported keyboard is connected. Omakboard works over USB with the AULA WIN 60 HE.", action: "" }
  case "other-model":
    return { title: "This keyboard is not supported yet", body: message, action: "doctor" }
  case "busy":
    return { title: "The keyboard is busy", body: "Another omakboard command is talking to the keyboard. Try again in a moment.", action: "" }
  default:
    return { title: "Could not read the keyboard", body: message, action: "doctor" }
  }
}

// ---- Units -----------------------------------------------------------------------------

function clamp(value, low, high) {
  return Math.max(low, Math.min(high, value))
}

function mm(steps, limits) {
  return steps * limits.unit_um / 1000
}

// "1.70": two decimals in 0.01 mm steps, three in 0.001 mm steps.
function formatMm(steps, limits) {
  return mm(steps, limits).toFixed(limits.unit_um >= 10 ? 2 : 3)
}

function stepsFromMm(value, limits) {
  return Math.round(value * 1000 / limits.unit_um)
}

// How far into its range an actuation depth sits, 0 at the shallowest.
function depthFraction(travel, limits) {
  var span = limits.travel_max - limits.travel_min
  return span > 0 ? clamp((travel - limits.travel_min) / span, 0, 1) : 0
}

// ---- Triggers --------------------------------------------------------------------------

function isRapid(mode) {
  return mode === "rapid" || mode === "rapid-separate"
}

function sameTrigger(a, b) {
  return !!a && !!b && a.mode === b.mode && a.travel === b.travel && a.press === b.press && a.release === b.release
}

function copyTriggers(triggers) {
  var out = {}
  Object.keys(triggers || {}).forEach(function(code) {
    var t = triggers[code]
    out[code] = { mode: t.mode, travel: t.travel, press: t.press, release: t.release }
  })
  return out
}

// One value when every selected key agrees, else null.
function common(codes, triggers, field) {
  var value = null
  for (var i = 0; i < codes.length; i++) {
    var t = triggers[codes[i]]
    if (!t) continue
    if (value === null) value = t[field]
    else if (t[field] !== value) return null
  }
  return value
}

// What the selected keys share, for the trigger panel. Null fields are mixed.
function selectionSummary(codes, triggers) {
  var known = codes.filter(function(c) { return !!triggers[c] })
  var modes = {}
  known.forEach(function(c) { modes[triggers[c].mode] = true })
  var kinds = Object.keys(modes)
  var rapidKeys = known.filter(function(c) { return isRapid(triggers[c].mode) })
  return {
    count: known.length,
    travel: common(known, triggers, "travel"),
    mode: kinds.length === 1 ? kinds[0] : null,
    // true, false, or null when some keys have it and some do not.
    rapid: rapidKeys.length === 0 ? false : (rapidKeys.length === known.length ? true : null),
    separate: rapidKeys.length > 0 && rapidKeys.every(function(c) { return triggers[c].mode === "rapid-separate" }),
    press: common(rapidKeys, triggers, "press"),
    release: common(rapidKeys, triggers, "release")
  }
}

// Applies one edit to the selected keys and returns new triggers; the input is left alone.
//   { travel }         actuation depth, clamped to the keyboard's range
//   { rapid: bool }    rapid trigger on (keeping a key's sensitivities, or the default) or off
//   { separate: bool } separate press and release on rapid trigger keys
//   { press }, { release }  sensitivities; on linked rapid trigger either sets both
function editTriggers(triggers, codes, edit, limits) {
  var out = copyTriggers(triggers)
  codes.forEach(function(code) {
    var t = out[code]
    if (!t) return
    if (edit.travel !== undefined) {
      t.travel = clamp(Math.round(edit.travel), limits.travel_min, limits.travel_max)
    }
    if (edit.rapid === true && !isRapid(t.mode)) {
      t.mode = "rapid"
      t.press = limits.sensitivity_default
      t.release = limits.sensitivity_default
    } else if (edit.rapid === false) {
      t.mode = "fixed"
    }
    if (edit.separate !== undefined && isRapid(t.mode)) {
      if (edit.separate) {
        t.mode = "rapid-separate"
      } else {
        t.mode = "rapid"
        t.release = t.press
      }
    }
    var sensitivity = function(v) { return clamp(Math.round(v), 1, limits.sensitivity_max) }
    if (edit.press !== undefined && isRapid(t.mode)) {
      t.press = sensitivity(edit.press)
      if (t.mode === "rapid") t.release = t.press
    }
    if (edit.release !== undefined && isRapid(t.mode)) {
      t.release = sensitivity(edit.release)
      if (t.mode === "rapid") t.press = t.release
    }
  })
  return out
}

// The keys whose trigger differs from what the keyboard has, as the server's apply takes them.
function triggerChanges(draft, original) {
  return Object.keys(draft || {}).filter(function(code) {
    return !sameTrigger(draft[code], original[code])
  }).map(function(code) {
    return { code: code, trigger: draft[code] }
  })
}

function changedCodes(draft, original) {
  var out = {}
  triggerChanges(draft, original).forEach(function(c) { out[c.code] = true })
  return out
}

// ---- Lighting --------------------------------------------------------------------------

function sameColor(a, b) {
  return !!a && !!b && a.r === b.r && a.g === b.g && a.b === b.b
}

function sameLighting(a, b) {
  if (!a || !b) return a === b
  return a.mode === b.mode && a.brightness === b.brightness && a.speed === b.speed
    && sameColor(a.foreground, b.foreground) && sameColor(a.background, b.background)
    && a.direction === b.direction && a.full_color === b.full_color && a.power === b.power
}

function withLighting(lighting, edit) {
  var out = JSON.parse(JSON.stringify(lighting))
  Object.keys(edit).forEach(function(key) { out[key] = edit[key] })
  return out
}

function rgbToHex(c) {
  var hex = function(v) { return ("0" + clamp(v | 0, 0, 255).toString(16)).slice(-2) }
  return "#" + hex(c.r) + hex(c.g) + hex(c.b)
}

function hexToRgb(text) {
  var m = /^#?([0-9a-fA-F]{6})$/.exec(String(text).trim())
  if (!m) return null
  var n = parseInt(m[1], 16)
  return { r: (n >> 16) & 255, g: (n >> 8) & 255, b: n & 255 }
}

// Colours offered as swatches, besides the theme's accent.
var SWATCHES = ["#ff0000", "#ff7a00", "#ffd000", "#00e060", "#00d8c0", "#00a0ff", "#3050ff", "#9040ff", "#ff40c0", "#ffffff"]

// Nerd Font glyphs for lighting modes, by id.
// Custom lights keys in their own saved colours; every other effect but Static can cycle colours.
var CUSTOM_MODE = 10

function supportsRainbow(mode) {
  return mode !== 0
}

function modeNote(mode) {
  if (mode === CUSTOM_MODE) return "Custom lights each key in the colours saved on the keyboard. Set them in AULA's driver."
  return ""
}

function modeGlyph(id) {
  var glyphs = { 10: "\uf00a", 0: "", 1: "", 2: "", 3: "", 4: "", 6: "", 7: "", 8: "", 9: "", 11: "", 12: "", 14: "", 15: "", 16: "" }
  return glyphs[id] || ""
}

// ---- Selection -------------------------------------------------------------------------

var GROUPS = {
  wasd: ["KeyW", "KeyA", "KeyS", "KeyD"],
  letters: "ABCDEFGHIJKLMNOPQRSTUVWXYZ".split("").map(function(l) { return "Key" + l }),
  numbers: "1234567890".split("").map(function(d) { return "Digit" + d })
}

// A selection is an object keyed by key code.
function selectionOf(codes) {
  var out = {}
  codes.forEach(function(c) { out[c] = true })
  return out
}

function allCodes(layout) {
  return layout ? layout.keys.map(function(k) { return k.code }) : []
}

// Selected codes in layout order.
function selectedCodes(selection, layout) {
  return allCodes(layout).filter(function(c) { return selection[c] === true })
}

// A click: alone it selects just that key (or clears it when it was the only one); with
// shift or ctrl it adds or removes the key.
function clickSelection(selection, code, additive) {
  if (additive) {
    var out = Object.assign({}, selection)
    if (out[code]) delete out[code]
    else out[code] = true
    return out
  }
  var only = Object.keys(selection).length === 1 && selection[code] === true
  return only ? {} : selectionOf([code])
}

// Keys that a rectangle in layout units touches.
function codesInRect(layout, rect) {
  if (!layout) return []
  return layout.keys.filter(function(k) {
    return k.x < rect.x + rect.width && rect.x < k.x + k.width && k.y < rect.y + rect.height && rect.y < k.y + k.height
  }).map(function(k) { return k.code })
}

// The key at a point in layout units, or null.
function keyAt(layout, x, y) {
  if (!layout) return null
  for (var i = 0; i < layout.keys.length; i++) {
    var k = layout.keys[i]
    if (x >= k.x && x < k.x + k.width && y >= k.y && y < k.y + k.height) return k
  }
  return null
}

// "W, A, S, D" for a few keys, "12 keys" for more.
function selectionLabel(codes, layout) {
  if (codes.length === 0) return "No keys selected"
  if (layout && codes.length === layout.keys.length) return "All " + codes.length + " keys"
  if (codes.length > 6) return codes.length + " keys"
  var labels = {}
  if (layout) layout.keys.forEach(function(k) { labels[k.code] = k.label })
  return codes.map(function(c) { return labels[c] || c }).join("  ")
}

function describeTrigger(t, limits) {
  if (!t) return ""
  var depth = formatMm(t.travel, limits) + " mm"
  if (t.mode === "rapid") return depth + " · rapid trigger " + formatMm(t.press, limits) + " mm"
  if (t.mode === "rapid-separate") {
    return depth + " · rapid trigger " + formatMm(t.press, limits) + " / " + formatMm(t.release, limits) + " mm"
  }
  return depth
}

// The footer line after a save.
function savedStatus(changed, undone) {
  var list = changed || []
  if (list.length === 0) return "Already set on the keyboard."
  var keys = list.filter(function(c) { return c !== "lighting" })
  var parts = []
  if (keys.length > 0) parts.push(keys.length > 6 ? keys.length + " keys" : keys.join(", "))
  if (list.indexOf("lighting") !== -1) parts.push("lighting")
  return (undone ? "Undid " : "Saved ") + parts.join(" and ") + (undone ? "." : ", checked on the keyboard.")
}
