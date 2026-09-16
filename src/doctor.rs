//! `omakboard doctor`: what stands between this machine and a tunable keyboard, and the
//! exact step that fixes each thing.

use std::fs;
use std::path::{Path, PathBuf};

use crate::discover::Found;
use crate::hidraw::Access;
use crate::registry::{MODELS, Support};

pub const UDEV_RULE_PATHS: [&str; 2] = [
    "/usr/lib/udev/rules.d/70-omakboard.rules",
    "/etc/udev/rules.d/70-omakboard.rules",
];
pub const PLUGIN_INSTALLER: &str =
    "~/.config/omarchy/plugins/io.github.elberacasa.omakboard/install.sh";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Ok,
    Fix,
    Note,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub level: Level,
    pub message: String,
    pub detail: Vec<String>,
}

impl Check {
    fn new(level: Level, message: impl Into<String>) -> Self {
        Self {
            level,
            message: message.into(),
            detail: Vec::new(),
        }
    }

    fn with(mut self, line: impl Into<String>) -> Self {
        self.detail.push(line.into());
        self
    }
}

/// What the checks look at, gathered separately so the advice can be tested.
pub struct Inputs<'a> {
    pub found: &'a [Found],
    pub udev_rule: Option<PathBuf>,
    /// keyd configuration files whose [ids] section names each found keyboard.
    pub keyd: Vec<(String, Vec<PathBuf>)>,
}

pub fn gather(found: &[Found]) -> Inputs<'_> {
    let udev_rule = UDEV_RULE_PATHS
        .iter()
        .map(PathBuf::from)
        .find(|p| p.exists());
    let keyd = found
        .iter()
        .map(|f| {
            (
                f.model.usb_id(),
                keyd_configs_for(Path::new("/etc/keyd"), &f.model.usb_id()),
            )
        })
        .collect();
    Inputs {
        found,
        udev_rule,
        keyd,
    }
}

pub fn checks(inputs: &Inputs) -> Vec<Check> {
    let mut out = Vec::new();

    if inputs.found.is_empty() {
        let mut check = Check::new(Level::Fix, "No supported keyboard is plugged in")
            .with("Omakboard knows these models:");
        for m in MODELS {
            check = check.with(format!("  {} ({})", m.name, m.usb_id()));
        }
        out.push(check.with("Plug one in over USB, then run `omakboard doctor` again."));
    }

    match &inputs.udev_rule {
        Some(path) => out.push(Check::new(
            Level::Ok,
            format!("udev rule installed at {}", path.display()),
        )),
        None => out.push(
            Check::new(Level::Fix, "The udev rule is not installed")
                .with("Without it only root can reach the keyboard's configuration interface.")
                .with(format!(
                    "Run the plugin's installer: bash {PLUGIN_INSTALLER}"
                )),
        ),
    }

    for f in inputs.found {
        let node = f.node.path.display();
        let iface = f
            .node
            .interface
            .map(|i| format!(", USB interface {i}"))
            .unwrap_or_default();
        out.push(Check::new(
            Level::Ok,
            format!("{} ({}) on {node}{iface}", f.model.name, f.model.usb_id()),
        ));

        match f.access {
            Access::Ok => out.push(Check::new(Level::Ok, format!("Your user can open {node}"))),
            Access::Denied if inputs.udev_rule.is_some() => out.push(
                Check::new(Level::Fix, format!("Your user cannot open {node} yet"))
                    .with("The rule is installed but has not been applied to this keyboard.")
                    .with("Replug it, or run: sudo udevadm trigger --subsystem-match=hidraw --action=change"),
            ),
            Access::Denied => out.push(
                Check::new(Level::Fix, format!("Your user cannot open {node}"))
                    .with("Installing the udev rule fixes this."),
            ),
            Access::Missing => out.push(Check::new(
                Level::Fix,
                format!("{node} disappeared while checking; replug the keyboard"),
            )),
        }

        let level = if f.model.support == Support::Verified {
            Level::Ok
        } else {
            Level::Note
        };
        out.push(Check::new(
            level,
            format!("Support: {}", f.model.support.describe()),
        ));

        let usb_id = f.model.usb_id();
        if let Some((_, files)) = inputs
            .keyd
            .iter()
            .find(|(id, files)| *id == usb_id && !files.is_empty())
        {
            let names: Vec<_> = files.iter().map(|p| p.display().to_string()).collect();
            out.push(
                Check::new(
                    Level::Note,
                    format!("keyd remaps this keyboard ({})", names.join(", ")),
                )
                .with("Omakboard tunes the switches in firmware and leaves remapping to keyd."),
            );
        }
    }
    out
}

/// keyd configs in `dir` whose [ids] section applies to `usb_id` (`2e3c:c365`): named
/// explicitly, or matched by `*` without being excluded with `-2e3c:c365`.
pub fn keyd_configs_for(dir: &Path, usb_id: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut matches: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "conf"))
        .filter(|p| fs::read_to_string(p).is_ok_and(|text| keyd_ids_match(&text, usb_id)))
        .collect();
    matches.sort();
    matches
}

fn keyd_ids_match(config: &str, usb_id: &str) -> bool {
    let mut in_ids = false;
    let (mut named, mut wildcard, mut excluded) = (false, false, false);
    for line in config.lines().map(str::trim) {
        if line.starts_with('[') {
            in_ids = line == "[ids]";
            continue;
        }
        if !in_ids || line.is_empty() || line.starts_with('#') {
            continue;
        }
        // keyd ids may carry a name suffix (`vvvv:pppp:name`); compare the vendor:product part.
        let id_of = |s: &str| {
            s.split(':')
                .take(2)
                .collect::<Vec<_>>()
                .join(":")
                .to_ascii_lowercase()
        };
        match line.strip_prefix('-') {
            Some(rest) => excluded |= id_of(rest) == usb_id,
            None if line == "*" => wildcard = true,
            None => named |= id_of(line) == usb_id,
        }
    }
    named || (wildcard && !excluded)
}

pub fn render(checks: &[Check]) -> String {
    let mut s = format!("Omakboard {}\n\n", env!("CARGO_PKG_VERSION"));
    for c in checks {
        let tag = match c.level {
            Level::Ok => "ok  ",
            Level::Fix => "fix ",
            Level::Note => "note",
        };
        s.push_str(&format!("  {tag}  {}\n", c.message));
        for line in &c.detail {
            s.push_str(&format!("        {line}\n"));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hidraw::Node;
    use crate::registry::lookup;

    fn win60(access: Access) -> Found {
        Found {
            model: lookup(0x2E3C, 0xC365).unwrap(),
            node: Node {
                name: "hidraw4".into(),
                path: "/dev/hidraw4".into(),
                vendor: 0x2E3C,
                product: 0xC365,
                hid_name: "WIN 60 HE".into(),
                interface: Some(2),
                descriptor: Vec::new(),
            },
            access,
        }
    }

    fn fixes(checks: &[Check]) -> Vec<&str> {
        checks
            .iter()
            .filter(|c| c.level == Level::Fix)
            .map(|c| c.message.as_str())
            .collect()
    }

    #[test]
    fn nothing_to_fix_once_installed_and_reachable() {
        let found = [win60(Access::Ok)];
        let inputs = Inputs {
            found: &found,
            udev_rule: Some(UDEV_RULE_PATHS[0].into()),
            keyd: Vec::new(),
        };
        assert!(fixes(&checks(&inputs)).is_empty());
    }

    #[test]
    fn missing_rule_points_at_the_installer() {
        let found = [win60(Access::Denied)];
        let inputs = Inputs {
            found: &found,
            udev_rule: None,
            keyd: Vec::new(),
        };
        let c = checks(&inputs);
        assert_eq!(
            fixes(&c),
            [
                "The udev rule is not installed",
                "Your user cannot open /dev/hidraw4"
            ]
        );
        assert!(c[0].detail.iter().any(|l| l.contains("install.sh")));
    }

    #[test]
    fn rule_present_but_unapplied_says_replug() {
        let found = [win60(Access::Denied)];
        let inputs = Inputs {
            found: &found,
            udev_rule: Some(UDEV_RULE_PATHS[1].into()),
            keyd: Vec::new(),
        };
        let c = checks(&inputs);
        let denied = c.iter().find(|c| c.level == Level::Fix).unwrap();
        assert!(denied.detail.iter().any(|l| l.contains("udevadm trigger")));
    }

    #[test]
    fn verified_models_report_support_as_ok() {
        let found = [win60(Access::Ok)];
        let inputs = Inputs {
            found: &found,
            udev_rule: Some(UDEV_RULE_PATHS[0].into()),
            keyd: Vec::new(),
        };
        assert!(
            checks(&inputs)
                .iter()
                .any(|c| c.level == Level::Ok && c.message == "Support: verified on real hardware")
        );
    }

    #[test]
    fn no_keyboard_lists_known_models() {
        let inputs = Inputs {
            found: &[],
            udev_rule: None,
            keyd: Vec::new(),
        };
        let c = checks(&inputs);
        assert!(
            c[0].detail
                .iter()
                .any(|l| l.contains("AULA WIN 60 HE (2e3c:c365)"))
        );
    }

    #[test]
    fn keyd_ids() {
        let own = "[ids]\n2e3c:c365\n\n[main]\ncapslock = overload(nav, capslock)\n";
        assert!(keyd_ids_match(own, "2e3c:c365"));
        assert!(keyd_ids_match("[ids]\n2E3C:C365:WIN 60 HE\n", "2e3c:c365"));
        assert!(keyd_ids_match("[ids]\n*\n", "2e3c:c365"));
        assert!(!keyd_ids_match("[ids]\n*\n-2e3c:c365\n", "2e3c:c365"));
        assert!(!keyd_ids_match("[ids]\n046d:c099\n", "2e3c:c365"));
        // An id outside [ids] is a key name or value, not a match.
        assert!(!keyd_ids_match("[main]\n2e3c:c365\n", "2e3c:c365"));
    }
}
