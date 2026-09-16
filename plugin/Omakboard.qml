import QtQuick
import Quickshell
import Quickshell.Io
import Quickshell.Wayland
import qs.Commons
import qs.Ui
import "Model.js" as Model

// Omakboard's overlay: your keyboard, key by key. Changes save themselves a moment after the
// last edit, through one long-lived `omakboard serve` that keeps the keyboard open. The first
// write is backed up, every write is read back from the keyboard, and Undo puts back what a
// write replaced.
Item {
  id: root

  // Injected by the shell.
  property var shell: null
  property var manifest: null

  property bool opened: false
  property bool mounted: false

  // The last answer from `omakboard serve`, or null before the first.
  property var state: null
  property string loadError: ""
  property bool helperOutdated: false
  property string notice: ""
  property bool noticeIsError: false

  // "triggers" or "lighting".
  property string tab: "triggers"

  // What the keyboard has, and the edits on top of it.
  property var originalTriggers: ({})
  property var draftTriggers: ({})
  property var originalLighting: null
  property var draftLighting: null
  property var selection: ({})

  property bool loading: false
  property bool saving: false
  property bool undoing: false

  readonly property bool ready: root.state !== null
  readonly property var layout: root.ready ? root.state.layout : null
  readonly property var limits: root.ready ? root.state.limits : null
  readonly property var keyboard: root.ready ? root.state.keyboard : null
  readonly property var selectedCodes: Model.selectedCodes(root.selection, root.layout)
  readonly property var summary: Model.selectionSummary(root.selectedCodes, root.draftTriggers)
  readonly property var dirtyKeys: Model.changedCodes(root.draftTriggers, root.originalTriggers)
  readonly property bool lightingDirty: root.ready && !Model.sameLighting(root.draftLighting, root.originalLighting)
  readonly property bool dirty: Object.keys(root.dirtyKeys).length > 0 || root.lightingDirty
  readonly property int undoDepth: root.ready ? root.state.undo : 0
  readonly property bool busy: root.saving || root.undoing
  readonly property string setupKind: root.ready ? "" : (root.helperOutdated ? "outdated" : Model.setupKind(root.loadError))

  readonly property int cardWidth: Math.min(Style.space(1180), panel.width - Style.gapsOut * 2)
  readonly property int cardHeight: Math.min(Style.space(860), panel.height - Style.gapsOut * 2)

  // What the open payload asked for, applied once the keyboard has been read.
  property var pendingOpen: null

  function open(payloadJson) {
    exitAnimation.stop()
    root.mounted = true
    root.opened = true
    enterAnimation.restart()
    root.pendingOpen = Model.openRequest(Model.parseJson(payloadJson))
    root.applyOpenRequest()
    if (!root.dirty && !root.busy) root.refresh()
    Qt.callLater(function() { keyCatcher.forceActiveFocus() })
  }

  function close() {
    if (!root.mounted || !root.opened) return
    // Edits waiting for the timer are saved now; the save finishes after the overlay closes.
    root.saveNow()
    root.opened = false
    enterAnimation.stop()
    exitAnimation.restart()
  }

  function finishClose() {
    root.mounted = false
    root.stopServerWhenIdle()
    if (root.shell && root.manifest) root.shell.hide(root.manifest.id)
  }

  // The server keeps the keyboard open; it stops once the overlay is closed and idle.
  function stopServerWhenIdle() {
    if (!root.opened && server.inFlight === 0 && !saveTimer.running) server.stop()
  }

  function refresh() {
    if (root.loading) return
    root.loading = true
    server.request({ cmd: "state" }, function(ok, result) {
      root.loading = false
      if (!ok) {
        root.failLoad(result)
        return
      }
      root.adopt(result, false)
      root.loadError = ""
      root.applyOpenRequest()
      root.stopServerWhenIdle()
    })
  }

  function applyOpenRequest() {
    var request = root.pendingOpen
    if (request === null) return
    if (request.tab !== null) root.tab = request.tab
    if (request.select === null) {
      root.pendingOpen = null
      return
    }
    // Keys can only be selected once the layout is known.
    if (!root.ready) return
    root.pendingOpen = null
    var known = Model.allCodes(root.layout)
    root.selection = Model.selectionOf(request.select.filter(function(c) { return known.indexOf(c) !== -1 }))
  }

  function retry() {
    server.stop()
    root.loadError = ""
    root.helperOutdated = false
    root.refresh()
  }

  // Takes a server answer as what the keyboard has. `keepDraft` keeps edits made meanwhile.
  function adopt(result, keepDraft) {
    root.helperOutdated = Model.helperOutdated(result)
    if (root.helperOutdated) {
      root.state = null
      return
    }
    root.state = result
    root.originalTriggers = Model.copyTriggers(result.triggers)
    root.originalLighting = result.lighting
    if (!keepDraft) {
      root.draftTriggers = Model.copyTriggers(result.triggers)
      root.draftLighting = result.lighting
    }
  }

  function failLoad(message) {
    if (root.ready && !Model.connectionLost(message)) {
      root.say(message, true)
      return
    }
    // The keyboard went away or cannot be reached: show what brings it back, not controls that
    // cannot save.
    root.state = null
    root.selection = {}
    root.loadError = message
  }

  function say(message, isError) {
    root.notice = message
    root.noticeIsError = isError
  }

  // ---- Editing --------------------------------------------------------------------------

  function editTriggers(edit, immediate) {
    if (!root.ready) return
    if (root.selectedCodes.length === 0) {
      root.say("Select keys on the keyboard first: click one, or drag across several.", false)
      return
    }
    root.draftTriggers = Model.editTriggers(root.draftTriggers, root.selectedCodes, edit, root.limits)
    root.schedule(immediate)
  }

  function editLighting(edit, immediate) {
    if (!root.ready) return
    root.draftLighting = Model.withLighting(root.draftLighting, edit)
    root.schedule(immediate)
  }

  function schedule(immediate) {
    root.say("", false)
    if (!root.dirty) {
      saveTimer.stop()
      return
    }
    saveTimer.interval = immediate ? 150 : 650
    saveTimer.restart()
  }

  function saveNow() {
    if (!saveTimer.running) return
    saveTimer.stop()
    root.save()
  }

  // Sends the edits the keyboard does not have yet.
  function save() {
    if (!root.ready || !root.dirty) return
    if (root.busy) {
      saveTimer.restart()
      return
    }
    var sentTriggers = Model.copyTriggers(root.draftTriggers)
    var sentLighting = root.draftLighting
    var request = { cmd: "apply", triggers: Model.triggerChanges(sentTriggers, root.originalTriggers) }
    if (root.lightingDirty) request.lighting = sentLighting
    root.saving = true
    server.request(request, function(ok, result) {
      root.saving = false
      if (ok) {
        var editedMeanwhile = Model.triggerChanges(root.draftTriggers, sentTriggers).length > 0
          || !Model.sameLighting(root.draftLighting, sentLighting)
        root.adopt(result, editedMeanwhile)
        root.say(Model.savedStatus(result.changed, false), false)
      } else {
        // Show what the keyboard really has, not edits it refused.
        root.draftTriggers = Model.copyTriggers(root.originalTriggers)
        root.draftLighting = root.originalLighting
        root.say("Not saved: " + result, true)
        root.refresh()
      }
      root.afterWrite()
    })
  }

  function undo() {
    if (root.busy) return
    saveTimer.stop()
    // The latest change is one not saved yet: drop it without touching the keyboard.
    if (root.dirty) {
      root.draftTriggers = Model.copyTriggers(root.originalTriggers)
      root.draftLighting = root.originalLighting
      root.say("Dropped the changes that were not saved yet.", false)
      return
    }
    if (root.undoDepth === 0) return
    root.undoing = true
    server.request({ cmd: "undo" }, function(ok, result) {
      root.undoing = false
      if (ok) {
        root.adopt(result, false)
        root.say(Model.savedStatus(result.changed, true), false)
      } else {
        root.say("Undo failed: " + result, true)
      }
      root.afterWrite()
    })
  }

  function afterWrite() {
    if (root.dirty && root.opened) saveTimer.restart()
    else if (root.dirty) root.save()
    root.stopServerWhenIdle()
  }

  // ---- Selection --------------------------------------------------------------------------

  function clickKey(code, additive) {
    root.selection = Model.clickSelection(root.selection, code, additive)
  }

  function boxKeys(codes, additive) {
    var next = additive ? Object.assign({}, root.selection) : {}
    codes.forEach(function(c) { next[c] = true })
    root.selection = next
  }

  function selectGroup(name) {
    root.selection = Model.selectionOf(name === "all" ? Model.allCodes(root.layout) : Model.GROUPS[name])
  }

  function nudgeTravel(steps) {
    if (root.selectedCodes.length === 0 || root.summary.travel === null) return
    root.editTriggers({ travel: root.summary.travel + steps }, false)
  }

  // ---- Setup ------------------------------------------------------------------------------

  readonly property string installScript: Model.localPath(Qt.resolvedUrl("../install.sh"))

  function installHelper() {
    Util.execArgv(Model.installArgv(root.installScript))
    root.say("Finish the installer in the terminal, then choose Try again.", false)
  }

  OmakboardServer {
    id: server
    onFailed: function(message) {
      root.loading = false
      root.saving = false
      root.undoing = false
      root.failLoad(message)
    }
  }

  // While the overlay waits for the keyboard to come back, it keeps looking.
  Timer {
    interval: 2000
    repeat: true
    running: root.opened && !root.ready && !root.loading
      && (root.setupKind === "unplugged" || root.setupKind === "permission" || root.setupKind === "busy")
    onTriggered: root.refresh()
  }

  Timer {
    id: saveTimer
    interval: 650
    onTriggered: root.save()
  }

  ParallelAnimation {
    id: enterAnimation
    NumberAnimation { target: card; property: "opacity"; from: 0; to: 1; duration: 180; easing.type: Easing.OutCubic }
    NumberAnimation { target: rise; property: "y"; from: Style.space(10); to: 0; duration: 240; easing.type: Easing.OutCubic }
  }

  SequentialAnimation {
    id: exitAnimation
    ParallelAnimation {
      NumberAnimation { target: card; property: "opacity"; to: 0; duration: 120; easing.type: Easing.InCubic }
      NumberAnimation { target: rise; property: "y"; to: Style.space(6); duration: 120; easing.type: Easing.InCubic }
    }
    ScriptAction { script: root.finishClose() }
  }

  component Label: Text {
    textFormat: Text.PlainText
    color: Color.menu.text
    elide: Text.ElideRight
    font.family: Style.font.menuFamily
    font.pixelSize: Style.font.body
  }

  // A small rounded fact in the header.
  component Chip: Rectangle {
    property string text: ""
    property bool accented: false
    width: chipLabel.implicitWidth + Style.spacing.lg * 2
    height: chipLabel.implicitHeight + Style.spacing.xs * 2
    radius: height / 2
    color: accented ? Util.alpha(Color.accent, 0.14) : Util.alpha(Color.menu.text, 0.06)
    border.width: Math.max(1, Style.normalBorderWidth)
    border.color: accented ? Util.alpha(Color.accent, 0.6) : Util.alpha(Color.menu.text, 0.12)

    Label {
      id: chipLabel
      anchors.centerIn: parent
      text: parent.text
      color: parent.accented ? Color.accent : Color.menu.text
      opacity: parent.accented ? 1 : 0.75
      font.pixelSize: Style.font.caption
    }
  }

  PanelWindow {
    id: panel
    visible: root.mounted
    anchors { top: true; bottom: true; left: true; right: true }
    color: "transparent"
    WlrLayershell.namespace: "omakboard"
    WlrLayershell.layer: WlrLayer.Overlay
    WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
    exclusionMode: ExclusionMode.Ignore

    Rectangle {
      anchors.fill: parent
      color: Color.menu.scrim
      opacity: card.opacity
    }

    MouseArea {
      anchors.fill: parent
      onClicked: root.close()
    }

    BorderSurface {
      id: card
      width: root.cardWidth
      height: root.cardHeight
      anchors.centerIn: parent
      radius: Style.cornerRadius
      color: Color.menu.background
      borderSpec: Border.surfaceSpec("menu", "border", Color.menu.border, Math.max(1, Style.space(2)))
      padding: Style.spacing.panelPadding
      opacity: 0

      transform: Translate { id: rise }

      MouseArea { anchors.fill: parent; onClicked: keyCatcher.forceActiveFocus() }

      Item {
        id: keyCatcher
        anchors.fill: parent
        focus: true

        Keys.onPressed: function(event) {
          var ctrl = (event.modifiers & Qt.ControlModifier) !== 0
          var shift = (event.modifiers & Qt.ShiftModifier) !== 0
          if (event.key === Qt.Key_Escape) {
            if (root.tab === "triggers" && root.selectedCodes.length > 0) root.selection = {}
            else root.close()
          } else if (ctrl && event.key === Qt.Key_Z) {
            root.undo()
          } else if (ctrl && event.key === Qt.Key_A && root.tab === "triggers") {
            root.selectGroup("all")
          } else if (event.text === "1") {
            root.tab = "triggers"
          } else if (event.text === "2") {
            root.tab = "lighting"
          } else if (root.tab === "triggers" && (event.key === Qt.Key_Up || event.text === "k")) {
            root.nudgeTravel(shift ? -1 : -5)
          } else if (root.tab === "triggers" && (event.key === Qt.Key_Down || event.text === "j")) {
            root.nudgeTravel(shift ? 1 : 5)
          } else if (root.tab === "triggers" && event.text === "r" && root.selectedCodes.length > 0) {
            root.editTriggers({ rapid: root.summary.rapid !== true }, true)
          } else {
            return
          }
          event.accepted = true
        }
      }

      Item {
        anchors.fill: parent
        anchors.topMargin: card.contentTopInset
        anchors.rightMargin: card.contentRightInset
        anchors.bottomMargin: card.contentBottomInset
        anchors.leftMargin: card.contentLeftInset

        // ---- Header ---------------------------------------------------------------------
        Item {
          id: header
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.top: parent.top
          height: Math.max(Style.space(44), Style.spacing.controlHeight + Style.spacing.sm)

          Row {
            anchors.left: parent.left
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.spacing.lg

            // The key presses while a change is being written to the keyboard.
            PixelMark {
              anchors.verticalCenter: parent.verticalCenter
              width: Style.space(32)
              height: width
              busy: root.busy || saveTimer.running
            }

            Column {
              anchors.verticalCenter: parent.verticalCenter

              Label {
                text: "Omakboard"
                font.pixelSize: Style.font.heading
                font.bold: true
              }

              Label {
                text: root.keyboard ? root.keyboard.name : "Hall-effect keyboard"
                opacity: 0.55
                font.pixelSize: Style.font.caption
              }
            }
          }

          ButtonGroup {
            anchors.centerIn: parent
            visible: root.ready
            options: [
              { value: "triggers", label: "Actuation", icon: "" },
              { value: "lighting", label: "Lighting", icon: "" }
            ]
            value: root.tab
            foreground: Color.menu.text
            background: Color.menu.background
            fontFamily: Style.font.menuFamily
            onChanged: function(value) {
              root.tab = value
              keyCatcher.forceActiveFocus()
            }
          }

          Row {
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.spacing.md

            Chip {
              anchors.verticalCenter: parent.verticalCenter
              visible: root.keyboard !== null && root.keyboard.firmware !== null
              text: root.keyboard ? "Firmware " + root.keyboard.firmware : ""
            }

            Chip {
              anchors.verticalCenter: parent.verticalCenter
              visible: root.keyboard !== null && root.keyboard.poll_rate_hz !== null
              text: root.keyboard && root.keyboard.poll_rate_hz ? (root.keyboard.poll_rate_hz / 1000) + " kHz polling" : ""
            }

            Chip {
              anchors.verticalCenter: parent.verticalCenter
              visible: root.keyboard !== null
              accented: true
              text: root.keyboard && root.keyboard.support === "verified" ? "Verified" : "Read only"
            }

            Button {
              anchors.verticalCenter: parent.verticalCenter
              iconText: ""
              tooltipText: "Close (Esc)"
              foreground: Color.menu.text
              onClicked: root.close()
            }
          }
        }

        // ---- Footer ---------------------------------------------------------------------
        Item {
          id: footer
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.bottom: parent.bottom
          height: Style.spacing.controlHeight + Style.spacing.sm * 2

          Row {
            anchors.left: parent.left
            anchors.right: actions.left
            anchors.rightMargin: Style.spacing.panelGap
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.spacing.md

            Rectangle {
              anchors.verticalCenter: parent.verticalCenter
              visible: root.busy || saveTimer.running
              width: Style.space(8)
              height: width
              radius: width / 2
              color: Color.accent

              SequentialAnimation on opacity {
                running: root.busy || saveTimer.running
                loops: Animation.Infinite
                NumberAnimation { to: 0.3; duration: 450 }
                NumberAnimation { to: 1; duration: 450 }
              }
            }

            Label {
              anchors.verticalCenter: parent.verticalCenter
              width: parent.width - Style.space(20)
              text: root.saving ? "Saving to the keyboard…"
                : root.undoing ? "Undoing…"
                : saveTimer.running ? "Saving in a moment…"
                : root.notice !== "" ? root.notice
                : root.ready ? "Changes save to the keyboard's memory as you make them." : ""
              color: !root.busy && !saveTimer.running && root.notice !== "" && root.noticeIsError ? Color.urgent : Color.menu.text
              opacity: root.notice !== "" || root.busy || saveTimer.running ? 1 : 0.5
            }
          }

          Row {
            id: actions
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: Style.spacing.md

            Label {
              anchors.verticalCenter: parent.verticalCenter
              visible: root.ready
              opacity: 0.45
              text: root.tab === "triggers"
                ? "drag to select    ctrl+a all    ↑↓ depth    r rapid trigger    ctrl+z undo    esc close"
                : "1 actuation    2 lighting    ctrl+z undo    esc close"
              font.pixelSize: Style.font.caption
            }

            Button {
              anchors.verticalCenter: parent.verticalCenter
              visible: root.undoDepth > 0 || root.dirty
              text: "Undo"
              iconText: ""
              bordered: true
              enabled: !root.busy
              opacity: enabled ? 1 : 0.5
              foreground: Color.menu.text
              fontFamily: Style.font.menuFamily
              onClicked: root.undo()
            }
          }
        }

        // ---- Workspace ------------------------------------------------------------------
        Item {
          id: workspace
          anchors.left: parent.left
          anchors.right: parent.right
          anchors.top: header.bottom
          anchors.topMargin: Style.spacing.panelGap
          anchors.bottom: footer.top
          anchors.bottomMargin: Style.spacing.panelGap

          Label {
            anchors.centerIn: parent
            visible: !root.ready && root.setupKind === ""
            opacity: 0.6
            text: "Reading your keyboard…"
            font.pixelSize: Style.font.title
          }

          // What stands between the overlay and the keyboard, and the one step that fixes it.
          Column {
            id: setupCard
            readonly property var copy: Model.setupCopy(root.setupKind, root.loadError)
            anchors.centerIn: parent
            width: Math.min(parent.width, Style.space(540))
            visible: root.setupKind !== ""
            spacing: Style.spacing.lg

            PixelMark {
              anchors.horizontalCenter: parent.horizontalCenter
              width: Style.space(64)
              height: width
              busy: root.loading
            }

            Label {
              width: parent.width
              horizontalAlignment: Text.AlignHCenter
              wrapMode: Text.Wrap
              text: setupCard.copy.title
              font.pixelSize: Style.font.title
              font.bold: true
            }

            Label {
              width: parent.width
              horizontalAlignment: Text.AlignHCenter
              wrapMode: Text.Wrap
              opacity: 0.7
              text: setupCard.copy.body
            }

            Row {
              anchors.horizontalCenter: parent.horizontalCenter
              spacing: Style.spacing.md

              Button {
                visible: setupCard.copy.action === "install"
                text: root.setupKind === "outdated" ? "Update" : "Run the installer"
                bordered: true
                foreground: Color.accent
                onClicked: root.installHelper()
              }

              Button {
                visible: setupCard.copy.action === "doctor"
                text: "Diagnostics"
                bordered: true
                foreground: Color.accent
                onClicked: Util.execArgv(Model.doctorArgv())
              }

              Button {
                text: "Try again"
                bordered: true
                foreground: Color.menu.text
                onClicked: root.retry()
              }
            }
          }

          // ---- Actuation ---------------------------------------------------------------
          Item {
            anchors.fill: parent
            visible: root.ready && root.tab === "triggers"

            KeyCanvas {
              id: triggerCanvas
              anchors.left: parent.left
              anchors.right: parent.right
              anchors.top: parent.top
              height: Math.min(parent.height * 0.5, width * (root.layout ? root.layout.height / root.layout.width : 0.37))
              layout: root.layout
              limits: root.limits
              triggers: root.draftTriggers
              dirty: root.dirtyKeys
              selection: root.selection
              onKeyClicked: function(code, additive) {
                root.clickKey(code, additive)
                keyCatcher.forceActiveFocus()
              }
              onBoxSelected: function(codes, additive) {
                root.boxKeys(codes, additive)
                keyCatcher.forceActiveFocus()
              }
              onEmptyClicked: {
                root.selection = {}
                keyCatcher.forceActiveFocus()
              }
            }

            // Selection shortcuts, and what is selected or under the pointer.
            Item {
              id: selectionBar
              anchors.left: parent.left
              anchors.right: parent.right
              anchors.top: triggerCanvas.bottom
              anchors.topMargin: Style.spacing.lg
              height: Style.spacing.controlHeight

              Row {
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                spacing: Style.spacing.sm

                Repeater {
                  model: [
                    { name: "all", label: "All" },
                    { name: "wasd", label: "WASD" },
                    { name: "letters", label: "Letters" },
                    { name: "numbers", label: "Numbers" }
                  ]

                  Button {
                    required property var modelData
                    anchors.verticalCenter: parent.verticalCenter
                    text: modelData.label
                    bordered: true
                    foreground: Color.menu.text
                    fontSize: Style.font.bodySmall
                    onClicked: {
                      root.selectGroup(modelData.name)
                      keyCatcher.forceActiveFocus()
                    }
                  }
                }

                Button {
                  anchors.verticalCenter: parent.verticalCenter
                  visible: root.selectedCodes.length > 0
                  text: "Clear"
                  foreground: Color.menu.text
                  fontSize: Style.font.bodySmall
                  onClicked: root.selection = {}
                }
              }

              Label {
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width * 0.5
                horizontalAlignment: Text.AlignRight
                readonly property var hovered: triggerCanvas.hoveredCode !== "" ? root.draftTriggers[triggerCanvas.hoveredCode] : null
                text: hovered
                  ? Model.selectionLabel([triggerCanvas.hoveredCode], root.layout) + "   " + Model.describeTrigger(hovered, root.limits)
                  : Model.selectionLabel(root.selectedCodes, root.layout)
                opacity: hovered ? 0.9 : 0.6
              }
            }

            TriggerPanel {
              anchors.left: parent.left
              anchors.right: parent.right
              anchors.top: selectionBar.bottom
              anchors.topMargin: Style.spacing.lg
              anchors.bottom: parent.bottom
              summary: root.summary
              limits: root.limits
              onEdited: function(edit, immediate) { root.editTriggers(edit, immediate) }
            }
          }

          // ---- Lighting ----------------------------------------------------------------
          Item {
            anchors.fill: parent
            visible: root.ready && root.tab === "lighting"

            KeyCanvas {
              id: lightCanvas
              anchors.left: parent.left
              anchors.right: parent.right
              anchors.top: parent.top
              height: Math.min(parent.height * 0.42, width * (root.layout ? root.layout.height / root.layout.width : 0.37))
              mode: "lighting"
              layout: root.layout
              glow: root.draftLighting ? Model.rgbToHex(root.draftLighting.foreground) : "transparent"
              rainbow: root.draftLighting !== null && root.draftLighting.full_color && Model.supportsRainbow(root.draftLighting.mode)
              glowLevel: root.draftLighting && root.limits && root.limits.brightness_max > 0
                ? root.draftLighting.brightness / root.limits.brightness_max : 0
            }

            LightingPanel {
              anchors.left: parent.left
              anchors.right: parent.right
              anchors.top: lightCanvas.bottom
              anchors.topMargin: Style.spacing.xl
              anchors.bottom: parent.bottom
              lighting: root.draftLighting
              modes: root.ready ? root.state.modes : []
              limits: root.limits
              onEdited: function(edit, immediate) { root.editLighting(edit, immediate) }
            }
          }
        }
      }
    }
  }
}
