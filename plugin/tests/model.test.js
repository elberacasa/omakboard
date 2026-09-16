// Tests for plugin/Model.js. Run with: node --test "plugin/tests/**/*.test.js"

const test = require("node:test")
const assert = require("node:assert/strict")
const fs = require("node:fs")
const path = require("node:path")
const vm = require("node:vm")

function loadLibrary(file) {
  const source = fs
    .readFileSync(path.join(__dirname, "..", file), "utf8")
    .replace(/^\.pragma library\s*$/m, "")
  const context = vm.createContext({ decodeURIComponent })
  vm.runInContext(source, context)
  return context
}

const Model = loadLibrary("Model.js")
// Values made inside the VM have its prototypes; compare them as plain data.
const plain = (v) => JSON.parse(JSON.stringify(v))

const LIMITS = { travel_min: 8, travel_max: 340, unit_um: 10, sensitivity_max: 170, sensitivity_default: 50, brightness_max: 4, speed_max: 4 }
const FIXED = { mode: "fixed", travel: 170, press: 1, release: 1 }
const LAYOUT = {
  width: 587,
  height: 217,
  keys: [
    { code: "KeyQ", label: "Q", index: 45, x: 59, y: 53, width: 35, height: 35 },
    { code: "KeyW", label: "W", index: 46, x: 97, y: 53, width: 35, height: 35 },
    { code: "KeyA", label: "A", index: 68, x: 69, y: 91, width: 35, height: 35 },
    { code: "Space", label: "Space", index: 116, x: 160, y: 167, width: 217, height: 35 }
  ]
}

function triggers() {
  return { KeyQ: { ...FIXED }, KeyW: { ...FIXED }, KeyA: { ...FIXED }, Space: { ...FIXED } }
}

const win60 = (overrides = {}) => ({
  schema: 1,
  version: "0.1.0",
  devices: [{ name: "AULA WIN 60 HE", usb_id: "2e3c:c365", support: "verified", access: "ok", ...overrides }]
})

// ---- Bar widget

test("hidden until the first probe finishes, or with no keyboard", () => {
  assert.equal(Model.indicatorState(-1, null).visible, false)
  assert.equal(Model.indicatorState(0, { schema: 1, devices: [] }).visible, false)
})

test("a missing command offers the installer", () => {
  const s = Model.indicatorState(127, null)
  assert.equal(s.visible, true)
  assert.equal(s.action, "install")
})

test("a reachable keyboard is active and opens the overlay", () => {
  const s = Model.indicatorState(0, win60())
  assert.equal(s.active, true)
  assert.equal(s.action, "open")
  assert.match(s.tooltip, /AULA WIN 60 HE/)
  assert.match(s.tooltip, /Ready to tune/)
})

test("an unreachable keyboard is inactive but still opens the overlay, which explains", () => {
  const s = Model.indicatorState(0, win60({ access: "denied" }))
  assert.equal(s.active, false)
  assert.equal(s.action, "open")
  assert.match(s.tooltip, /Cannot reach/)
})

test("a newer report schema asks to update the command", () => {
  assert.equal(Model.indicatorState(0, { ...win60(), schema: 2 }).action, "install")
})

test("commands the widget and overlay run", () => {
  assert.deepEqual(plain(Model.installArgv("/home/a b/it's/install.sh")), [
    "omarchy-launch-floating-terminal-with-presentation",
    "bash '/home/a b/it'\\''s/install.sh'"
  ])
  assert.deepEqual(plain(Model.toggleArgv()), ["omarchy-shell", "shell", "toggle", "io.github.elberacasa.omakboard", "{}"])
  assert.equal(Model.localPath("file:///home/a%20b/install.sh"), "/home/a b/install.sh")
})

test("open payloads", () => {
  assert.deepEqual(plain(Model.openRequest({ tab: "lighting" })), { tab: "lighting", select: null })
  assert.deepEqual(plain(Model.openRequest({ select: ["KeyW", 3, "KeyA"] })), { tab: null, select: ["KeyW", "KeyA"] })
  assert.deepEqual(plain(Model.openRequest({ select: "wasd", tab: "nope" })), { tab: null, select: ["KeyW", "KeyA", "KeyS", "KeyD"] })
  assert.deepEqual(plain(Model.openRequest(null)), { tab: null, select: null })
})

// ---- Server and setup

test("helper protocol", () => {
  assert.equal(Model.helperOutdated({ protocol: 1 }), false)
  assert.equal(Model.helperOutdated({ protocol: 0 }), true)
  assert.equal(Model.helperOutdated({}), true)
})

test("server exit messages", () => {
  assert.match(Model.errorMessage("", 127), /not installed/)
  assert.equal(Model.errorMessage("warming up\nomakboard: no supported keyboard is plugged in\n", 1), "no supported keyboard is plugged in")
  assert.match(Model.errorMessage("", 3), /exit 3/)
})

test("setup states and their one action", () => {
  assert.equal(Model.setupKind(""), "")
  assert.equal(Model.setupKind("The omakboard command is not installed or not on PATH."), "install")
  assert.equal(Model.setupKind("permission denied opening /dev/hidraw4; install ..."), "permission")
  assert.equal(Model.setupKind("no supported keyboard is plugged in"), "unplugged")
  assert.equal(Model.setupKind("this keyboard reports product \"SI2828KZHEARGB\" ..."), "other-model")
  assert.equal(Model.setupKind("another omakboard is talking to the keyboard; try again"), "busy")
  assert.equal(Model.setupKind("the keyboard did not answer"), "error")
  assert.equal(Model.setupCopy("permission", "").action, "install")
  assert.equal(Model.setupCopy("unplugged", "").action, "")
  assert.equal(Model.setupCopy("error", "boom").body, "boom")
})

test("lost connections are told apart from ordinary errors", () => {
  assert.equal(Model.connectionLost("no supported keyboard is plugged in"), true)
  assert.equal(Model.connectionLost("permission denied opening /dev/hidraw4"), true)
  assert.equal(Model.connectionLost("lighting (direction 0 -> 4) did not read back as written"), false)
  assert.equal(Model.connectionLost("travel 400 is outside 8-340"), false)
})

test("effects: rainbow and notes", () => {
  assert.equal(Model.supportsRainbow(0), false)
  assert.equal(Model.supportsRainbow(7), true)
  assert.match(Model.modeNote(10), /saved on the keyboard/)
  assert.equal(Model.modeNote(2), "")
  assert.notEqual(Model.modeGlyph(10), Model.modeGlyph(99))
})

// ---- Units

test("millimetres", () => {
  assert.equal(Model.formatMm(170, LIMITS), "1.70")
  assert.equal(Model.formatMm(8, LIMITS), "0.08")
  assert.equal(Model.formatMm(1234, { ...LIMITS, unit_um: 1 }), "1.234")
  assert.equal(Model.stepsFromMm(1.2, LIMITS), 120)
  assert.equal(Model.stepsFromMm(0.3, LIMITS), 30)
  assert.equal(Model.depthFraction(8, LIMITS), 0)
  assert.equal(Model.depthFraction(340, LIMITS), 1)
  assert.equal(Model.depthFraction(174, LIMITS), 0.5)
})

// ---- Triggers

test("a selection's shared values, and mixed ones", () => {
  const t = triggers()
  let s = Model.selectionSummary(["KeyW", "KeyA"], t)
  assert.deepEqual(plain(s), { count: 2, travel: 170, mode: "fixed", rapid: false, separate: false, press: null, release: null })
  t.KeyA = { mode: "rapid", travel: 120, press: 30, release: 30 }
  s = Model.selectionSummary(["KeyW", "KeyA"], t)
  assert.equal(s.travel, null)
  assert.equal(s.mode, null)
  assert.equal(s.rapid, null)
  assert.equal(s.press, 30)
  s = Model.selectionSummary(["KeyA"], t)
  assert.equal(s.rapid, true)
  assert.equal(s.separate, false)
  assert.equal(Model.selectionSummary([], t).count, 0)
})

test("actuation edits clamp to the keyboard's range and leave the input alone", () => {
  const t = triggers()
  const out = Model.editTriggers(t, ["KeyW", "KeyA"], { travel: 120 }, LIMITS)
  assert.equal(out.KeyW.travel, 120)
  assert.equal(out.KeyA.travel, 120)
  assert.equal(out.KeyQ.travel, 170)
  assert.equal(t.KeyW.travel, 170)
  assert.equal(Model.editTriggers(t, ["KeyW"], { travel: 999 }, LIMITS).KeyW.travel, 340)
  assert.equal(Model.editTriggers(t, ["KeyW"], { travel: 2 }, LIMITS).KeyW.travel, 8)
})

test("rapid trigger on takes the default sensitivity, and keeps a key's own", () => {
  const t = triggers()
  t.KeyA = { mode: "rapid-separate", travel: 120, press: 20, release: 40 }
  const on = Model.editTriggers(t, ["KeyW", "KeyA"], { rapid: true }, LIMITS)
  assert.deepEqual(plain(on.KeyW), { mode: "rapid", travel: 170, press: 50, release: 50 })
  assert.deepEqual(plain(on.KeyA), { mode: "rapid-separate", travel: 120, press: 20, release: 40 })
  const off = Model.editTriggers(on, ["KeyW", "KeyA"], { rapid: false }, LIMITS)
  assert.equal(off.KeyW.mode, "fixed")
  assert.equal(off.KeyA.mode, "fixed")
  assert.equal(off.KeyA.press, 20)
})

test("sensitivities: linked sets both, separate sets one, fixed keys are skipped", () => {
  const t = Model.editTriggers(triggers(), ["KeyW"], { rapid: true }, LIMITS)
  let out = Model.editTriggers(t, ["KeyW", "KeyQ"], { press: 30 }, LIMITS)
  assert.deepEqual(plain(out.KeyW), { mode: "rapid", travel: 170, press: 30, release: 30 })
  assert.deepEqual(plain(out.KeyQ), FIXED)
  out = Model.editTriggers(out, ["KeyW"], { separate: true }, LIMITS)
  out = Model.editTriggers(out, ["KeyW"], { release: 60 }, LIMITS)
  assert.deepEqual(plain(out.KeyW), { mode: "rapid-separate", travel: 170, press: 30, release: 60 })
  out = Model.editTriggers(out, ["KeyW"], { separate: false }, LIMITS)
  assert.deepEqual(plain(out.KeyW), { mode: "rapid", travel: 170, press: 30, release: 30 })
  assert.equal(Model.editTriggers(out, ["KeyW"], { press: 500 }, LIMITS).KeyW.press, 170)
  assert.equal(Model.editTriggers(out, ["KeyW"], { press: 0 }, LIMITS).KeyW.press, 1)
})

test("only changed keys are sent", () => {
  const original = triggers()
  const draft = Model.editTriggers(original, ["KeyW", "KeyA"], { travel: 120 }, LIMITS)
  draft.KeyQ = { ...FIXED }
  assert.deepEqual(plain(Model.triggerChanges(draft, original)), [
    { code: "KeyW", trigger: { mode: "fixed", travel: 120, press: 1, release: 1 } },
    { code: "KeyA", trigger: { mode: "fixed", travel: 120, press: 1, release: 1 } }
  ])
  assert.deepEqual(plain(Model.changedCodes(draft, original)), { KeyW: true, KeyA: true })
  assert.deepEqual(plain(Model.triggerChanges(original, original)), [])
})

// ---- Lighting

const LIGHTING = { mode: 0, brightness: 4, speed: 3, foreground: { r: 255, g: 0, b: 0 }, background: { r: 0, g: 0, b: 0 }, direction: 0, full_color: false, power: 0 }

test("lighting edits and comparison", () => {
  const next = Model.withLighting(LIGHTING, { brightness: 2, foreground: { r: 0, g: 160, b: 255 } })
  assert.equal(next.brightness, 2)
  assert.equal(LIGHTING.brightness, 4)
  assert.equal(Model.sameLighting(LIGHTING, Model.withLighting(LIGHTING, {})), true)
  assert.equal(Model.sameLighting(LIGHTING, next), false)
})

test("colours", () => {
  assert.equal(Model.rgbToHex({ r: 0, g: 160, b: 255 }), "#00a0ff")
  assert.deepEqual(plain(Model.hexToRgb("00a0ff")), { r: 0, g: 160, b: 255 })
  assert.deepEqual(plain(Model.hexToRgb("#FF8800")), { r: 255, g: 136, b: 0 })
  assert.equal(Model.hexToRgb("#abc"), null)
  assert.equal(Model.hexToRgb("zzzzzz"), null)
  assert.ok(Model.SWATCHES.every((s) => Model.hexToRgb(s) !== null))
})

// ---- Selection

test("clicks select one key, and shift or ctrl adds and removes", () => {
  let s = Model.clickSelection({}, "KeyW", false)
  assert.deepEqual(plain(s), { KeyW: true })
  assert.deepEqual(plain(Model.clickSelection(s, "KeyW", false)), {})
  s = Model.clickSelection(s, "KeyA", true)
  assert.deepEqual(Object.keys(s).sort(), ["KeyA", "KeyW"])
  assert.deepEqual(plain(Model.clickSelection(s, "KeyA", false)), { KeyA: true })
  assert.deepEqual(plain(Model.clickSelection(s, "KeyA", true)), { KeyW: true })
})

test("keys under a point and inside a dragged rectangle", () => {
  assert.equal(Model.keyAt(LAYOUT, 100, 60).code, "KeyW")
  assert.equal(Model.keyAt(LAYOUT, 95, 60), null)
  assert.deepEqual(plain(Model.codesInRect(LAYOUT, { x: 80, y: 60, width: 30, height: 40 })), ["KeyQ", "KeyW", "KeyA"])
  assert.deepEqual(plain(Model.codesInRect(LAYOUT, { x: 0, y: 0, width: 5, height: 5 })), [])
})

test("selection order, groups and labels", () => {
  const sel = Model.selectionOf(["Space", "KeyW"])
  assert.deepEqual(plain(Model.selectedCodes(sel, LAYOUT)), ["KeyW", "Space"])
  assert.equal(Model.selectionLabel(["KeyW", "Space"], LAYOUT), "W  Space")
  assert.equal(Model.selectionLabel([], LAYOUT), "No keys selected")
  assert.equal(Model.selectionLabel(Model.allCodes(LAYOUT), LAYOUT), "All 4 keys")
  assert.deepEqual(plain(Model.GROUPS.wasd), ["KeyW", "KeyA", "KeyS", "KeyD"])
  assert.equal(Model.GROUPS.letters.length, 26)
})

test("describing a trigger and a save", () => {
  assert.equal(Model.describeTrigger(FIXED, LIMITS), "1.70 mm")
  assert.equal(Model.describeTrigger({ mode: "rapid", travel: 120, press: 30, release: 30 }, LIMITS), "1.20 mm · rapid trigger 0.30 mm")
  assert.equal(Model.describeTrigger({ mode: "rapid-separate", travel: 120, press: 20, release: 40 }, LIMITS), "1.20 mm · rapid trigger 0.20 / 0.40 mm")
  assert.equal(Model.savedStatus(["W", "A"], false), "Saved W, A, checked on the keyboard.")
  assert.equal(Model.savedStatus(["lighting"], true), "Undid lighting.")
  assert.equal(Model.savedStatus([], false), "Already set on the keyboard.")
  assert.equal(Model.savedStatus(["1", "2", "3", "4", "5", "6", "7", "lighting"], false), "Saved 7 keys and lighting, checked on the keyboard.")
})
