import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Model.js" as Model

// Bar widget: shows a supported keyboard while one is plugged in, or an install prompt
// while the omakboard command is missing. Clicking opens the overlay, or the installer.
BarWidget {
  id: root
  moduleName: "io.github.elberacasa.omakboard"

  property int exitCode: -1
  property var report: null
  readonly property var state: Model.indicatorState(exitCode, report)
  readonly property string installScript: Model.localPath(Qt.resolvedUrl("../install.sh"))

  visible: state.visible
  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  function refresh() {
    if (!probe.running) probe.running = true
  }

  Process {
    id: probe
    // A missing binary exits 127 through the shell instead of failing to start silently.
    command: ["sh", "-c", "command -v omakboard >/dev/null 2>&1 || exit 127; exec omakboard devices --json"]
    stdout: StdioCollector {
      onStreamFinished: root.report = Model.parseJson(text)
    }
    onExited: function(exitCode) {
      root.exitCode = exitCode
    }
  }

  // Scanning sysfs is cheap, and a poll catches plugging and unplugging without a daemon.
  Timer {
    interval: 5000
    running: true
    repeat: true
    triggeredOnStart: true
    onTriggered: root.refresh()
  }

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    // The pixel mark in the bar's colours; its key is lit while the keyboard is reachable.
    iconComponent: Component {
      PixelMark {
        ink: button.foreground
        accent: button.active ? Color.accent : button.foreground
        opacity: button.active ? 1 : 0.55
      }
    }
    active: root.state.active
    tooltipText: root.state.tooltip
    onPressed: function(mouseButton) {
      Util.execArgv(root.state.action === "install" ? Model.installArgv(root.installScript) : Model.toggleArgv())
      root.refresh()
    }
  }
}
