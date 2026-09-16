import QtQuick
import qs.Commons
import qs.Ui
import "Model.js" as Model

// Actuation and rapid trigger for the selected keys. Values every selected key shares are
// shown; mixed ones say so, and moving a control sets it on all of them.
Item {
  id: panel

  property var summary: null
  property var limits: null

  // `edit` as Model.editTriggers takes it; `immediate` saves at once (a click, a released
  // slider) instead of after a pause (a drag still moving).
  signal edited(var edit, bool immediate)

  readonly property bool active: summary !== null && summary.count > 0 && limits !== null
  readonly property bool rapidOn: active && summary.rapid === true
  readonly property bool separateOn: rapidOn && summary.separate

  function mm(steps) {
    return panel.limits ? Model.formatMm(steps, panel.limits) : ""
  }

  component Label: Text {
    textFormat: Text.PlainText
    color: Color.menu.text
    font.family: Style.font.menuFamily
    font.pixelSize: Style.font.body
    elide: Text.ElideRight
  }

  component Caption: Label {
    opacity: 0.55
    font.pixelSize: Style.font.caption
    wrapMode: Text.Wrap
    elide: Text.ElideNone
  }

  component Card: Rectangle {
    radius: Math.max(Style.cornerRadius, Style.space(8))
    color: Util.alpha(Color.menu.text, 0.035)
    border.width: Math.max(1, Style.normalBorderWidth)
    border.color: Util.alpha(Color.menu.text, 0.10)
  }

  // A slider in travel steps, with its value in mm beside it.
  component StepSlider: Item {
    id: stepSlider
    property string title: ""
    property int value: 0
    property bool mixed: false
    property int minimum: 1
    property int maximum: 100
    signal moved(int value)
    signal released(int value)

    height: Math.max(Style.spacing.controlHeight, valueText.implicitHeight) + titleText.implicitHeight + Style.spacing.xs

    Label {
      id: titleText
      anchors.left: parent.left
      anchors.top: parent.top
      text: stepSlider.title
      opacity: 0.8
    }

    Label {
      id: valueText
      anchors.right: parent.right
      anchors.top: parent.top
      text: stepSlider.mixed ? "Mixed" : panel.mm(slider.liveValue) + " mm"
      font.bold: true
      color: stepSlider.mixed ? Util.alpha(Color.menu.text, 0.6) : Color.menu.text
    }

    PanelSlider {
      id: slider
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.bottom: parent.bottom
      minimum: stepSlider.minimum
      maximum: stepSlider.maximum
      step: 1
      integer: true
      value: stepSlider.value
      trackColor: Util.alpha(Color.menu.text, 0.14)
      fillColor: Color.accent
      knobColor: Color.menu.text
      tickColor: Color.menu.background
      onMoved: function(v) { stepSlider.moved(Math.round(v)) }
      onReleased: function(v) { stepSlider.released(Math.round(v)) }
    }
  }

  Row {
    anchors.fill: parent
    spacing: Style.spacing.xl

    // ---- Actuation point ------------------------------------------------------------
    Card {
      id: actuationCard
      width: (parent.width - gaugeCard.width - parent.spacing * 2) * 0.48
      height: parent.height
      opacity: panel.active ? 1 : 0.45

      Column {
        anchors.fill: parent
        anchors.margins: Style.spacing.huge
        spacing: Style.spacing.lg

        Label {
          text: "Actuation point"
          font.pixelSize: Style.font.title
          font.bold: true
        }

        Caption {
          width: parent.width
          text: "How far a key travels before it presses. Shallower is faster; deeper forgives resting fingers."
        }

        Row {
          spacing: Style.spacing.md

          Label {
            id: bigValue
            text: !panel.active ? "—" : (panel.summary.travel === null ? "Mixed" : panel.mm(panel.summary.travel))
            font.pixelSize: Style.font.heading * 2
            font.bold: true
          }

          Label {
            anchors.baseline: bigValue.baseline
            visible: panel.active && panel.summary.travel !== null
            text: "mm"
            opacity: 0.6
            font.pixelSize: Style.font.title
          }
        }

        Row {
          width: parent.width
          spacing: Style.spacing.md

          Button {
            anchors.verticalCenter: parent.verticalCenter
            text: "−"
            tooltipText: "0.05 mm shallower"
            bordered: true
            enabled: panel.active
            foreground: Color.menu.text
            onClicked: panel.edited({ travel: panel.summary.travel === null ? panel.limits.travel_min : panel.summary.travel - 5 }, true)
          }

          PanelSlider {
            id: travelSlider
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - Style.space(96)
            enabled: panel.active
            minimum: panel.limits ? panel.limits.travel_min : 0
            maximum: panel.limits ? panel.limits.travel_max : 1
            step: 1
            integer: true
            value: panel.active && panel.summary.travel !== null ? panel.summary.travel : (panel.limits ? panel.limits.travel_min : 0)
            trackColor: Util.alpha(Color.menu.text, 0.14)
            fillColor: Color.accent
            knobColor: Color.menu.text
            onMoved: function(v) { panel.edited({ travel: Math.round(v) }, false) }
            onReleased: function(v) { panel.edited({ travel: Math.round(v) }, true) }
          }

          Button {
            anchors.verticalCenter: parent.verticalCenter
            text: "+"
            tooltipText: "0.05 mm deeper"
            bordered: true
            enabled: panel.active
            foreground: Color.menu.text
            onClicked: panel.edited({ travel: panel.summary.travel === null ? panel.limits.travel_max : panel.summary.travel + 5 }, true)
          }
        }

        Row {
          spacing: Style.spacing.sm

          Repeater {
            model: [
              { steps: 40, label: "0.40" },
              { steps: 100, label: "1.00" },
              { steps: 170, label: "1.70 · default" },
              { steps: 250, label: "2.50" }
            ]

            Button {
              required property var modelData
              text: modelData.label
              bordered: true
              selected: panel.active && panel.summary.travel === modelData.steps
              enabled: panel.active
              foreground: Color.menu.text
              fontSize: Style.font.bodySmall
              onClicked: panel.edited({ travel: modelData.steps }, true)
            }
          }
        }
      }
    }

    // ---- Rapid trigger ----------------------------------------------------------------
    Card {
      width: (parent.width - gaugeCard.width - parent.spacing * 2) * 0.52
      height: parent.height
      opacity: panel.active ? 1 : 0.45

      Column {
        anchors.fill: parent
        anchors.margins: Style.spacing.huge
        spacing: Style.spacing.lg

        Item {
          width: parent.width
          height: Math.max(rtTitle.implicitHeight, rtSwitch.implicitHeight)

          Label {
            id: rtTitle
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            text: "Rapid trigger"
            font.pixelSize: Style.font.title
            font.bold: true
          }

          Label {
            anchors.right: rtSwitch.left
            anchors.rightMargin: Style.spacing.md
            anchors.verticalCenter: parent.verticalCenter
            visible: panel.active && panel.summary.rapid === null
            text: "On for some keys"
            opacity: 0.55
            font.pixelSize: Style.font.caption
          }

          ToggleSwitch {
            id: rtSwitch
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            checked: panel.rapidOn
            interactive: panel.active
            onToggled: panel.edited({ rapid: !panel.rapidOn }, true)
          }
        }

        Caption {
          width: parent.width
          text: "Presses again the moment a key moves down, and releases the moment it rises, wherever it is in its travel. The sensitivity is how far it has to move."
        }

        Item {
          width: parent.width
          height: separateSwitch.implicitHeight
          visible: panel.rapidOn

          Label {
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            text: "Separate press and release"
            opacity: 0.8
          }

          ToggleSwitch {
            id: separateSwitch
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            checked: panel.separateOn
            onToggled: panel.edited({ separate: !panel.separateOn }, true)
          }
        }

        StepSlider {
          width: parent.width
          visible: panel.rapidOn
          title: panel.separateOn ? "Press sensitivity" : "Sensitivity"
          minimum: 1
          maximum: panel.limits ? panel.limits.sensitivity_max : 100
          value: panel.rapidOn && panel.summary.press !== null ? panel.summary.press : (panel.limits ? panel.limits.sensitivity_default : 1)
          mixed: panel.rapidOn && panel.summary.press === null
          onMoved: function(v) { panel.edited({ press: v }, false) }
          onReleased: function(v) { panel.edited({ press: v }, true) }
        }

        StepSlider {
          width: parent.width
          visible: panel.separateOn
          title: "Release sensitivity"
          minimum: 1
          maximum: panel.limits ? panel.limits.sensitivity_max : 100
          value: panel.separateOn && panel.summary.release !== null ? panel.summary.release : (panel.limits ? panel.limits.sensitivity_default : 1)
          mixed: panel.separateOn && panel.summary.release === null
          onMoved: function(v) { panel.edited({ release: v }, false) }
          onReleased: function(v) { panel.edited({ release: v }, true) }
        }
      }
    }

    // ---- Travel ---------------------------------------------------------------------
    Card {
      id: gaugeCard
      width: Style.space(170)
      height: parent.height
      opacity: panel.active ? 1 : 0.45

      Label {
        id: gaugeTitle
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.margins: Style.spacing.huge
        text: "Travel"
        font.pixelSize: Style.font.title
        font.bold: true
      }

      Item {
        id: gauge
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: gaugeTitle.bottom
        anchors.bottom: parent.bottom
        anchors.topMargin: Style.spacing.lg
        anchors.bottomMargin: Style.spacing.huge
        anchors.leftMargin: Style.spacing.huge

        readonly property real max: panel.limits ? panel.limits.travel_max : 1
        readonly property bool known: panel.active && panel.summary.travel !== null
        readonly property real at: known ? panel.summary.travel / max : 0

        Rectangle {
          id: track
          x: Style.space(28)
          width: Style.space(10)
          height: parent.height
          radius: width / 2
          color: Util.alpha(Color.menu.text, 0.10)
        }

        // Free travel above the actuation point.
        Rectangle {
          x: track.x
          width: track.width
          height: track.height * gauge.at
          radius: track.radius
          color: Util.alpha(Color.accent, 0.35)
          visible: gauge.known
          Behavior on height { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
        }

        // Where rapid trigger works: everywhere past the actuation point.
        Rectangle {
          x: track.x
          y: track.height * gauge.at
          width: track.width
          height: track.height * (1 - gauge.at)
          radius: track.radius
          visible: gauge.known && panel.rapidOn
          color: Util.alpha(Color.accent, 0.14)
          border.width: Math.max(1, Style.normalBorderWidth)
          border.color: Util.alpha(Color.accent, 0.5)
        }

        Rectangle {
          visible: gauge.known
          x: track.x - Style.space(6)
          y: track.height * gauge.at - height / 2
          width: track.width + Style.space(12)
          height: Math.max(2, Style.space(3))
          radius: height / 2
          color: Color.accent
          Behavior on y { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
        }

        Label {
          x: track.x + track.width + Style.spacing.lg
          y: Math.max(0, Math.min(parent.height - height, track.height * gauge.at - height / 2))
          visible: gauge.known
          text: gauge.known ? panel.mm(panel.summary.travel) + " mm" : ""
          color: Color.accent
          font.bold: true
          font.pixelSize: Style.font.bodySmall
        }

        Label {
          x: track.x + track.width + Style.spacing.lg
          anchors.bottom: parent.bottom
          visible: gauge.known && panel.rapidOn && gauge.at < 0.8
          text: "rapid trigger"
          color: Color.accent
          opacity: 0.8
          font.pixelSize: Style.font.caption
        }

        Label {
          anchors.right: track.left
          anchors.rightMargin: Style.spacing.sm
          anchors.top: parent.top
          text: "0"
          opacity: 0.45
          font.pixelSize: Style.font.caption
        }

        Label {
          anchors.right: track.left
          anchors.rightMargin: Style.spacing.sm
          anchors.bottom: parent.bottom
          text: panel.limits ? panel.mm(panel.limits.travel_max) : ""
          opacity: 0.45
          font.pixelSize: Style.font.caption
        }
      }
    }
  }
}
