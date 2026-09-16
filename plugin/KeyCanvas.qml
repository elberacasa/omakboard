import QtQuick
import qs.Commons
import "Model.js" as Model

// The keyboard, drawn from its real layout. Each key shows its actuation depth as a number and
// a fill along its bottom edge, and a rapid trigger tag. Click selects a key, shift or ctrl
// click adds and removes, and dragging across the board selects every key the box touches.
// In lighting mode the keys glow in the chosen colour instead.
Item {
  id: canvas

  // `layout`, `limits` from `omakboard serve`; `triggers` the draft being edited.
  property var layout: null
  property var limits: null
  property var triggers: ({})
  // Keys whose draft differs from the keyboard: marked until saved.
  property var dirty: ({})
  property var selection: ({})
  // "triggers" or "lighting".
  property string mode: "triggers"
  property color glow: "transparent"
  property real glowLevel: 0

  property string hoveredCode: ""

  signal keyClicked(string code, bool additive)
  signal boxSelected(var codes, bool additive)
  signal emptyClicked()

  readonly property real scale: layout ? Math.min(width / layout.width, height / layout.height) : 1
  readonly property real boardWidth: layout ? layout.width * scale : 0
  readonly property real boardHeight: layout ? layout.height * scale : 0
  readonly property bool interactive: mode === "triggers"

  component Label: Text {
    textFormat: Text.PlainText
    color: Color.menu.text
    font.family: Style.font.menuFamily
    elide: Text.ElideRight
  }

  Rectangle {
    id: board
    width: canvas.boardWidth
    height: canvas.boardHeight
    anchors.centerIn: parent
    radius: Math.max(Style.cornerRadius, Style.space(8))
    color: Util.alpha(Color.menu.text, 0.035)
    border.width: Math.max(1, Style.normalBorderWidth)
    border.color: Util.alpha(Color.menu.text, 0.10)

    Repeater {
      model: canvas.layout ? canvas.layout.keys : []

      delegate: Rectangle {
        id: cap
        required property var modelData
        readonly property var trigger: canvas.triggers[modelData.code]
        readonly property bool selected: canvas.selection[modelData.code] === true
        readonly property bool hovered: canvas.hoveredCode === modelData.code
        readonly property bool changed: canvas.dirty[modelData.code] === true
        readonly property bool rapid: trigger ? Model.isRapid(trigger.mode) : false
        readonly property real unit: canvas.scale

        x: modelData.x * unit
        y: modelData.y * unit
        width: modelData.width * unit
        height: modelData.height * unit
        radius: Math.max(3, 2 * unit)
        color: {
          if (canvas.mode === "lighting") return Util.alpha(canvas.glow, 0.06 + 0.34 * canvas.glowLevel)
          if (selected) return Util.alpha(Color.accent, hovered ? 0.30 : 0.22)
          return Util.alpha(Color.menu.text, hovered ? 0.12 : 0.06)
        }
        border.width: selected && canvas.interactive ? Math.max(2, Style.space(2)) : Math.max(1, Style.normalBorderWidth)
        border.color: {
          if (canvas.mode === "lighting") return Util.alpha(canvas.glow, 0.20 + 0.55 * canvas.glowLevel)
          if (selected) return Color.accent
          return Util.alpha(Color.menu.text, hovered ? 0.35 : 0.16)
        }

        Behavior on color { ColorAnimation { duration: 120 } }

        Label {
          anchors.left: parent.left
          anchors.top: parent.top
          anchors.leftMargin: Math.max(3, 4 * cap.unit)
          anchors.topMargin: Math.max(2, 3 * cap.unit)
          width: parent.width - anchors.leftMargin * 2
          text: cap.modelData.label
          font.pixelSize: Math.max(8, Math.round(7.2 * cap.unit))
          font.bold: true
          opacity: canvas.mode === "lighting" ? 0.85 : 0.9
        }

        // Saved yet? A dot until the keyboard has it.
        Rectangle {
          visible: cap.changed && canvas.interactive
          anchors.right: parent.right
          anchors.top: parent.top
          anchors.margins: Math.max(3, 4 * cap.unit)
          width: Math.max(5, 3.2 * cap.unit)
          height: width
          radius: width / 2
          color: Color.accent
        }

        Rectangle {
          id: rtTag
          visible: cap.rapid && canvas.interactive
          anchors.left: parent.left
          anchors.bottom: depthTrack.top
          anchors.leftMargin: Math.max(3, 4 * cap.unit)
          anchors.bottomMargin: Math.max(1, 1.5 * cap.unit)
          width: rtLabel.implicitWidth + Math.max(4, 3 * cap.unit)
          height: rtLabel.implicitHeight
          radius: height / 3
          color: Util.alpha(Color.accent, cap.selected ? 0.35 : 0.22)

          Label {
            id: rtLabel
            anchors.centerIn: parent
            text: "RT"
            color: Color.accent
            font.pixelSize: Math.max(7, Math.round(5.2 * cap.unit))
            font.bold: true
          }
        }

        Label {
          visible: canvas.interactive && cap.trigger !== undefined
          anchors.right: parent.right
          anchors.bottom: depthTrack.top
          anchors.rightMargin: Math.max(3, 4 * cap.unit)
          anchors.bottomMargin: Math.max(1, 1 * cap.unit)
          text: cap.trigger && canvas.limits ? Model.formatMm(cap.trigger.travel, canvas.limits) : ""
          font.pixelSize: Math.max(7, Math.round(5.8 * cap.unit))
          opacity: cap.selected ? 0.95 : 0.6
        }

        // Actuation depth: the fuller, the deeper the key presses before it acts.
        Rectangle {
          id: depthTrack
          visible: canvas.interactive
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.bottom: parent.bottom
          anchors.margins: Math.max(3, 4 * cap.unit)
          height: Math.max(2, 1.6 * cap.unit)
          radius: height / 2
          color: Util.alpha(Color.menu.text, 0.12)

          Rectangle {
            anchors.left: parent.left
            anchors.top: parent.top
            anchors.bottom: parent.bottom
            radius: parent.radius
            width: cap.trigger && canvas.limits
              ? Math.max(parent.height, parent.width * cap.trigger.travel / canvas.limits.travel_max)
              : 0
            color: cap.selected || cap.rapid ? Color.accent : Util.alpha(Color.menu.text, 0.55)
            Behavior on width { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
          }
        }
      }
    }

    // The box being dragged out.
    Rectangle {
      id: band
      visible: pointer.boxing
      x: Math.min(pointer.startX, pointer.endX)
      y: Math.min(pointer.startY, pointer.endY)
      width: Math.abs(pointer.endX - pointer.startX)
      height: Math.abs(pointer.endY - pointer.startY)
      color: Util.alpha(Color.accent, 0.12)
      border.width: Math.max(1, Style.normalBorderWidth)
      border.color: Color.accent
      radius: Math.max(2, Style.cornerRadius / 2)
    }

    MouseArea {
      id: pointer
      anchors.fill: parent
      enabled: canvas.interactive
      hoverEnabled: true
      cursorShape: canvas.hoveredCode !== "" ? Qt.PointingHandCursor : Qt.ArrowCursor

      property real startX: 0
      property real startY: 0
      property real endX: 0
      property real endY: 0
      property bool pressedDown: false
      property bool boxing: false

      function keyUnder(x, y) {
        return Model.keyAt(canvas.layout, x / canvas.scale, y / canvas.scale)
      }

      onPressed: function(mouse) {
        pressedDown = true
        boxing = false
        startX = endX = mouse.x
        startY = endY = mouse.y
      }

      onPositionChanged: function(mouse) {
        var key = keyUnder(mouse.x, mouse.y)
        canvas.hoveredCode = key ? key.code : ""
        if (!pressedDown) return
        endX = mouse.x
        endY = mouse.y
        if (!boxing && Math.abs(endX - startX) + Math.abs(endY - startY) > Style.space(6)) boxing = true
      }

      onReleased: function(mouse) {
        var additive = (mouse.modifiers & (Qt.ShiftModifier | Qt.ControlModifier)) !== 0
        if (boxing) {
          var s = canvas.scale
          var rect = { x: band.x / s, y: band.y / s, width: band.width / s, height: band.height / s }
          canvas.boxSelected(Model.codesInRect(canvas.layout, rect), additive)
        } else {
          var key = keyUnder(mouse.x, mouse.y)
          if (key) canvas.keyClicked(key.code, additive)
          else canvas.emptyClicked()
        }
        pressedDown = false
        boxing = false
      }

      onExited: canvas.hoveredCode = ""
    }
  }
}
