import QtQuick
import qs.Commons

// Omakboard's mark: a keyboard on a 16 × 16 grid, drawn in whole pixels. While `busy`, one key
// presses and springs back, as in the README logo (scripts/pixel-mark.py draws the same sprite).
Canvas {
  id: mark

  property color ink: Color.menu.text
  property color accent: Color.accent
  property bool busy: false
  // Advances while busy; the key is down on odd frames.
  property int frame: 0

  implicitWidth: 32
  implicitHeight: 32

  // k outline and keys, p the key that presses (drawn up; one row lower when pressed).
  readonly property var sprite: [
    "................",
    "....k...........",
    "....k...........",
    "....k...........",
    ".kkkkkkkkkkkkkk.",
    ".k............k.",
    ".k.k.k.k.k.k..k.",
    ".k............k.",
    ".k..k.p.k.k.k.k.",
    ".k............k.",
    ".k.k.kkkkkk.k.k.",
    ".k............k.",
    ".kkkkkkkkkkkkkk.",
    "................",
    "................",
    "................"
  ]

  function css(color) {
    return "rgb(" + Math.round(color.r * 255) + "," + Math.round(color.g * 255) + "," + Math.round(color.b * 255) + ")"
  }

  Timer {
    interval: 180
    repeat: true
    running: mark.busy && mark.visible
    onTriggered: mark.frame++
    onRunningChanged: if (!running) mark.frame = 0
  }

  onFrameChanged: requestPaint()
  onInkChanged: requestPaint()
  onAccentChanged: requestPaint()
  onWidthChanged: requestPaint()
  onHeightChanged: requestPaint()

  onPaint: {
    var ctx = getContext("2d")
    ctx.reset()
    // Whole pixels only: the grid never scales to a fraction. The sprite's rows 0-12 are drawn
    // centred, so the keyboard sits in the middle of a square slot.
    var cell = Math.max(1, Math.floor(Math.min(width, height) / 16))
    var ox = Math.floor((width - 16 * cell) / 2)
    var oy = Math.floor((height - 13 * cell) / 2)
    var down = frame % 2 === 1
    var ink = css(mark.ink)
    var accent = css(mark.accent)
    for (var y = 0; y < 16; y++) {
      for (var x = 0; x < 16; x++) {
        var ch = sprite[y].charAt(x)
        if (ch === "k") {
          ctx.fillStyle = ink
          ctx.fillRect(ox + x * cell, oy + y * cell, cell, cell)
        } else if (ch === "p") {
          ctx.fillStyle = accent
          ctx.fillRect(ox + x * cell, oy + (down ? y + 1 : y) * cell, cell, cell)
        }
      }
    }
  }
}
