import QtQuick
import qs.Commons
import qs.Ui
import "Model.js" as Model

// The keyboard's lighting: the effects it advertises, brightness, speed and colour.
Item {
  id: panel

  property var lighting: null
  property var modes: []
  property var limits: null

  signal edited(var edit, bool immediate)

  readonly property bool ready: lighting !== null && limits !== null

  component Label: Text {
    textFormat: Text.PlainText
    color: Color.menu.text
    font.family: Style.font.menuFamily
    font.pixelSize: Style.font.body
    elide: Text.ElideRight
  }

  component Card: Rectangle {
    radius: Math.max(Style.cornerRadius, Style.space(8))
    color: Util.alpha(Color.menu.text, 0.035)
    border.width: Math.max(1, Style.normalBorderWidth)
    border.color: Util.alpha(Color.menu.text, 0.10)
  }

  component LevelSlider: Item {
    id: level
    property string title: ""
    property int value: 0
    property int maximum: 4
    property string lowLabel: ""
    property string highLabel: ""
    signal moved(int value)
    signal released(int value)

    height: levelTitle.implicitHeight + slider.implicitHeight + ends.implicitHeight + Style.spacing.xs * 2

    Label {
      id: levelTitle
      anchors.left: parent.left
      anchors.top: parent.top
      text: level.title
      opacity: 0.8
    }

    Label {
      anchors.right: parent.right
      anchors.top: parent.top
      text: Math.round(slider.liveValue) + " / " + level.maximum
      font.bold: true
    }

    PanelSlider {
      id: slider
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.top: levelTitle.bottom
      anchors.topMargin: Style.spacing.xs
      minimum: 0
      maximum: level.maximum
      step: 1
      integer: true
      tickCount: level.maximum + 1
      value: level.value
      trackColor: Util.alpha(Color.menu.text, 0.14)
      fillColor: Color.accent
      knobColor: Color.menu.text
      tickColor: Color.menu.background
      onMoved: function(v) { level.moved(Math.round(v)) }
      onReleased: function(v) { level.released(Math.round(v)) }
    }

    Item {
      id: ends
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.top: slider.bottom
      implicitHeight: lowText.implicitHeight

      Label { id: lowText; anchors.left: parent.left; text: level.lowLabel; opacity: 0.45; font.pixelSize: Style.font.caption }
      Label { anchors.right: parent.right; text: level.highLabel; opacity: 0.45; font.pixelSize: Style.font.caption }
    }
  }

  Row {
    anchors.fill: parent
    spacing: Style.spacing.xl

    // ---- Effects --------------------------------------------------------------------
    Card {
      id: effectsCard
      width: parent.width * 0.56
      height: parent.height

      Label {
        id: effectsTitle
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.margins: Style.spacing.huge
        text: "Effect"
        font.pixelSize: Style.font.title
        font.bold: true
      }

      Label {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.margins: Style.spacing.huge
        visible: text !== ""
        text: panel.ready ? Model.modeNote(panel.lighting.mode) : ""
        wrapMode: Text.Wrap
        elide: Text.ElideNone
        opacity: 0.55
        font.pixelSize: Style.font.caption
      }

      Grid {
        id: effects
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: effectsTitle.bottom
        anchors.margins: Style.spacing.huge
        columns: 4
        spacing: Style.spacing.md

        readonly property real tileWidth: (width - spacing * (columns - 1)) / columns

        Repeater {
          model: panel.modes

          Rectangle {
            id: tile
            required property var modelData
            readonly property bool current: panel.ready && panel.lighting.mode === modelData.id

            width: effects.tileWidth
            height: Style.space(40)
            radius: Math.max(Style.cornerRadius, Style.space(6))
            color: current ? Util.alpha(Color.accent, 0.18) : Util.alpha(Color.menu.text, tileArea.containsMouse ? 0.09 : 0.04)
            border.width: current ? Math.max(2, Style.space(2)) : Math.max(1, Style.normalBorderWidth)
            border.color: current ? Color.accent : Util.alpha(Color.menu.text, tileArea.containsMouse ? 0.3 : 0.12)

            Behavior on color { ColorAnimation { duration: 100 } }

            Row {
              anchors.left: parent.left
              anchors.leftMargin: Style.spacing.lg
              anchors.right: parent.right
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.spacing.md

              Label {
                anchors.verticalCenter: parent.verticalCenter
                text: Model.modeGlyph(tile.modelData.id)
                color: tile.current ? Color.accent : Color.menu.text
                opacity: tile.current ? 1 : 0.6
                font.pixelSize: Style.font.icon
              }

              Label {
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - Style.space(40)
                text: tile.modelData.name
                font.bold: tile.current
              }
            }

            MouseArea {
              id: tileArea
              anchors.fill: parent
              hoverEnabled: true
              cursorShape: Qt.PointingHandCursor
              onClicked: panel.edited({ mode: tile.modelData.id }, true)
            }
          }
        }
      }
    }

    // ---- Brightness, speed, colour ------------------------------------------------------
    Card {
      width: parent.width - effectsCard.width - parent.spacing
      height: parent.height

      Column {
        anchors.fill: parent
        anchors.margins: Style.spacing.huge
        spacing: Style.spacing.xl

        LevelSlider {
          width: parent.width
          title: "Brightness"
          maximum: panel.limits ? panel.limits.brightness_max : 4
          value: panel.ready ? panel.lighting.brightness : 0
          lowLabel: "Off"
          highLabel: "Brightest"
          onMoved: function(v) { panel.edited({ brightness: v }, false) }
          onReleased: function(v) { panel.edited({ brightness: v }, true) }
        }

        LevelSlider {
          width: parent.width
          title: "Speed"
          maximum: panel.limits ? panel.limits.speed_max : 4
          value: panel.ready ? panel.lighting.speed : 0
          lowLabel: "Slow"
          highLabel: "Fast"
          onMoved: function(v) { panel.edited({ speed: v }, false) }
          onReleased: function(v) { panel.edited({ speed: v }, true) }
        }

        Item {
          width: parent.width
          height: Math.max(rainbowText.implicitHeight, rainbowSwitch.implicitHeight)
          visible: panel.ready && Model.supportsRainbow(panel.lighting.mode)

          Column {
            id: rainbowText
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter

            Label {
              text: "Rainbow colours"
              opacity: 0.8
            }

            Label {
              text: "Cycles through every colour instead of one"
              opacity: 0.45
              font.pixelSize: Style.font.caption
            }
          }

          ToggleSwitch {
            id: rainbowSwitch
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            checked: panel.ready && panel.lighting.full_color
            onToggled: panel.edited({ full_color: !panel.lighting.full_color }, true)
          }
        }

        Column {
          width: parent.width
          spacing: Style.spacing.md
          // One colour does not apply while rainbow cycles them all.
          opacity: panel.ready && panel.lighting.full_color && Model.supportsRainbow(panel.lighting.mode) ? 0.4 : 1

          Label {
            text: "Colour"
            opacity: 0.8
          }

          Row {
            spacing: Style.spacing.md

            Rectangle {
              anchors.verticalCenter: parent.verticalCenter
              width: hexField.implicitHeight
              height: width
              radius: Math.max(Style.cornerRadius, Style.space(4))
              color: panel.ready ? Model.rgbToHex(panel.lighting.foreground) : "transparent"
              border.width: Math.max(1, Style.normalBorderWidth)
              border.color: Util.alpha(Color.menu.text, 0.3)
            }

            TextField {
              id: hexField
              width: Style.space(120)
              text: panel.ready ? Model.rgbToHex(panel.lighting.foreground) : ""
              foreground: Color.menu.text
              font.family: Style.font.menuFamily
              maximumLength: 7
              onEditingFinished: {
                var rgb = Model.hexToRgb(text)
                if (rgb) panel.edited({ foreground: rgb }, true)
                else text = Model.rgbToHex(panel.lighting.foreground)
              }
            }
          }

          Flow {
            width: parent.width
            spacing: Style.spacing.sm

            Repeater {
              model: [Model.rgbToHex({ r: Math.round(Color.accent.r * 255), g: Math.round(Color.accent.g * 255), b: Math.round(Color.accent.b * 255) })].concat(Model.SWATCHES)

              Rectangle {
                id: swatch
                required property var modelData
                required property int index
                readonly property bool current: panel.ready && Model.rgbToHex(panel.lighting.foreground) === modelData.toLowerCase()

                width: Style.space(30)
                height: width
                radius: width / 2
                color: modelData
                border.width: current ? Math.max(2, Style.space(3)) : Math.max(1, Style.normalBorderWidth)
                border.color: current ? Color.menu.text : Util.alpha(Color.menu.text, swatchArea.containsMouse ? 0.6 : 0.2)

                MouseArea {
                  id: swatchArea
                  anchors.fill: parent
                  hoverEnabled: true
                  cursorShape: Qt.PointingHandCursor
                  onClicked: panel.edited({ foreground: Model.hexToRgb(swatch.modelData) }, true)
                }

                PanelToolTip {
                  visible: swatchArea.containsMouse
                  text: swatch.index === 0 ? "Your theme's accent" : swatch.modelData
                }
              }
            }
          }
        }
      }
    }
  }
}
