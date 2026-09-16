use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};

use omakboard::apply::{self, KeyChange, SavedBackup, TriggerEdit};
use omakboard::discover::{self, Found};
use omakboard::doctor::{self, Level};
use omakboard::hidraw::{Access, Hidraw, Sysfs, Transport, TransportError};
use omakboard::layout::COLUMNS;
use omakboard::lock::DeviceLock;
use omakboard::proto::aula_ff1b::{
    self as aula, Lighting, Rgb, Trigger, TriggerMode, TriggerRange,
};
use omakboard::registry::{Model, Support};
use omakboard::serve::{Opener, Server};
use omakboard::session::Session;
use omakboard::snapshot::{self, Info, KeyTrigger};

/// Tune Hall-effect keyboards on Omarchy.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List supported keyboards that are plugged in.
    Devices {
        /// Print JSON, for the bar widget and scripts.
        #[arg(long)]
        json: bool,
    },
    /// Check what is needed to reach the keyboard, and how to fix it.
    Doctor,
    /// Read the keyboard's identity, firmware, lighting and limits.
    Info {
        #[arg(long)]
        json: bool,
    },
    /// Read every key's actuation point and rapid trigger settings.
    Triggers {
        #[arg(long)]
        json: bool,
    },
    /// Save everything readable, with the raw replies, to a JSON file.
    Backup {
        /// Where to write it. Defaults to ~/.local/state/omakboard/backups/.
        #[arg(long, short)]
        output: Option<PathBuf>,
    },
    /// Set how deep keys press before they actuate.
    Actuation {
        /// Depth in mm, such as 1.20.
        mm: String,
        #[command(flatten)]
        keys: KeyArgs,
        #[arg(long)]
        dry_run: bool,
    },
    /// Turn rapid trigger on or off.
    RapidTrigger {
        state: OnOff,
        /// Press and release sensitivity in mm (0.50 unless the key already has one).
        #[arg(long, conflicts_with_all = ["press", "release"])]
        sensitivity: Option<String>,
        /// Press sensitivity in mm, set apart from release.
        #[arg(long)]
        press: Option<String>,
        /// Release sensitivity in mm, set apart from press.
        #[arg(long)]
        release: Option<String>,
        #[command(flatten)]
        keys: KeyArgs,
        #[arg(long)]
        dry_run: bool,
    },
    /// Change the lighting mode, brightness, speed or colour.
    Lighting {
        /// A mode by name (static, breath, wave, auto-ripple, ...) or number.
        #[arg(long)]
        mode: Option<String>,
        #[arg(long)]
        brightness: Option<u8>,
        #[arg(long)]
        speed: Option<u8>,
        /// Colour as RRGGBB.
        #[arg(long)]
        color: Option<String>,
        /// Cycle through every colour instead of one (not on Static).
        #[arg(long)]
        rainbow: Option<OnOff>,
        #[arg(long)]
        dry_run: bool,
    },
    /// Answer the Omarchy overlay: JSON requests on stdin, one response per line on stdout.
    Serve,
    /// Put back the settings saved in a backup.
    Restore {
        file: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(clap::Args)]
struct KeyArgs {
    /// Every key.
    #[arg(long, conflicts_with = "keys")]
    all: bool,
    /// Keys by label or code: W A S D, Space, LShift, KeyQ.
    keys: Vec<String>,
}

#[derive(Clone, Copy, ValueEnum)]
enum OnOff {
    On,
    Off,
}

fn main() -> ExitCode {
    // A closed pipe (`omakboard triggers | head`) ends the process quietly instead of panicking.
    // Nothing is printed between a write and its read-back, so this never cuts a change short.
    // SAFETY: resetting a signal to its default disposition before any threads start.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    let cli = Cli::parse();
    let found = match discover::discover(&Sysfs::default()) {
        Ok(found) => found,
        Err(e) => return fail(format!("cannot list hidraw devices: {e}")),
    };
    let result = match cli.command {
        Command::Devices { json: true } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&discover::report(&found)).expect("report serialises")
            );
            Ok(())
        }
        Command::Devices { json: false } => {
            print_devices(&found);
            Ok(())
        }
        Command::Doctor => {
            let checks = doctor::checks(&doctor::gather(&found));
            print!("{}", doctor::render(&checks));
            return if checks.iter().any(|c| c.level == Level::Fix) {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            };
        }
        Command::Info { json } => info(&found, json),
        Command::Triggers { json } => triggers(&found, json),
        Command::Backup { output } => {
            read_all(&found)
                .and_then(|r| save_backup(&r, output))
                .map(|(path, keys, frames)| {
                    println!(
                        "Saved {keys} keys and {frames} frames to {}",
                        path.display()
                    );
                })
        }
        Command::Actuation { mm, keys, dry_run } => change(&found, dry_run, |r| {
            let travel = apply::parse_mm(&mm, &r.info.trigger_range)?;
            trigger_plan(r, &keys, TriggerEdit::Actuation(travel))
        }),
        Command::RapidTrigger {
            state,
            sensitivity,
            press,
            release,
            keys,
            dry_run,
        } => change(&found, dry_run, |r| {
            let range = &r.info.trigger_range;
            let mm =
                |v: &Option<String>| v.as_deref().map(|s| apply::parse_mm(s, range)).transpose();
            let edit = match state {
                OnOff::Off => TriggerEdit::RapidOff,
                OnOff::On => match mm(&sensitivity)? {
                    Some(both) => TriggerEdit::RapidOn {
                        press: Some(both),
                        release: Some(both),
                    },
                    None => TriggerEdit::RapidOn {
                        press: mm(&press)?,
                        release: mm(&release)?,
                    },
                },
            };
            trigger_plan(r, &keys, edit)
        }),
        Command::Lighting {
            mode,
            brightness,
            speed,
            color,
            rainbow,
            dry_run,
        } => change(&found, dry_run, |r| {
            let from = r.info.lighting;
            let mut to = from;
            if let Some(mode) = mode {
                // By name, with spaces or hyphens ("auto-ripple"), or by number.
                to.mode = (0..=u8::MAX)
                    .find(|&id| {
                        aula::lighting_mode_name(id).is_some_and(|name| {
                            name.eq_ignore_ascii_case(&mode)
                                || name.replace(' ', "-").eq_ignore_ascii_case(&mode)
                        })
                    })
                    .or_else(|| mode.parse().ok())
                    .ok_or_else(|| format!("{mode:?} is not a lighting mode name or number"))?;
            }
            to.brightness = brightness.unwrap_or(to.brightness);
            to.speed = speed.unwrap_or(to.speed);
            if let Some(color) = color {
                to.foreground = parse_rgb(&color)?;
            }
            if let Some(rainbow) = rainbow {
                to.full_color = matches!(rainbow, OnOff::On);
            }
            // The form the firmware keeps, so the plan shows what the keyboard will have.
            let to = aula::stored_lighting(&to);
            Ok(Plan {
                triggers: Vec::new(),
                lighting: (to != from).then_some((from, to)),
            })
        }),
        Command::Serve => {
            let stdin = std::io::stdin().lock();
            let stdout = std::io::stdout().lock();
            Server::new(SysfsOpener)
                .run(stdin, stdout)
                .map_err(|e| e.to_string())
        }
        Command::Restore { file, dry_run } => change(&found, dry_run, |r| restore_plan(r, &file)),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => fail(message),
    }
}

fn fail(message: impl std::fmt::Display) -> ExitCode {
    eprintln!("omakboard: {message}");
    ExitCode::FAILURE
}

/// Opens the first supported keyboard, holding the device lock until the session is dropped.
fn open(found: &[Found]) -> Result<(&Found, Session<Locked>), String> {
    let f = found.first().ok_or(
        "no supported keyboard is plugged in; `omakboard doctor` lists the models it knows",
    )?;
    let lock = DeviceLock::acquire(Duration::from_secs(5)).map_err(|e| e.to_string())?;
    let hidraw = Hidraw::open(&f.node.path).map_err(|e| e.to_string())?;
    Ok((
        f,
        Session::new(Locked {
            hidraw,
            _lock: lock,
        }),
    ))
}

/// A hidraw node together with the lock that lets this process talk to it.
struct Locked {
    hidraw: Hidraw,
    _lock: DeviceLock,
}

impl Transport for Locked {
    fn send(&mut self, report: &[u8]) -> Result<(), TransportError> {
        self.hidraw.send(report)
    }

    fn recv(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, TransportError> {
        self.hidraw.recv(timeout)
    }
}

/// Finds and opens the keyboard for `omakboard serve`, which locks per request.
struct SysfsOpener;

impl Opener for SysfsOpener {
    type Transport = Hidraw;

    fn open(&mut self) -> Result<(&'static Model, Hidraw), String> {
        let found = discover::discover(&Sysfs::default())
            .map_err(|e| format!("cannot list hidraw devices: {e}"))?;
        let f = found
            .into_iter()
            .next()
            .ok_or("no supported keyboard is plugged in")?;
        let hidraw = Hidraw::open(&f.node.path).map_err(|e| e.to_string())?;
        Ok((f.model, hidraw))
    }
}

fn info(found: &[Found], json: bool) -> Result<(), String> {
    let (f, mut session) = open(found)?;
    let info = snapshot::read_info(&mut session, f.model).map_err(|e| e.to_string())?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&info).expect("info serialises")
        );
        return Ok(());
    }
    let id = &info.identity;
    let r = &info.trigger_range;
    println!(
        "{}  {}  {}",
        f.model.name,
        f.model.usb_id(),
        f.node.path.display()
    );
    println!("  product      {}", id.chip().unwrap_or("?"));
    println!("  platform     {}", id.platform().unwrap_or("?"));
    println!(
        "  firmware     {}{}",
        id.firmware().unwrap_or("?"),
        id.built
            .as_ref()
            .map(|b| format!(" (built {b})"))
            .unwrap_or_default()
    );
    if let Some(hz) = info.poll_rate {
        println!("  polling      {hz} Hz");
    }
    println!(
        "  actuation    {:.2}-{:.2} mm in {:.3} mm steps; rapid trigger 0.01-{:.2} mm",
        r.mm(r.min),
        r.mm(r.max),
        f64::from(r.unit_um) / 1000.0,
        r.mm(r.max_sensitivity())
    );
    println!("  lighting     {}", describe_lighting(&info.lighting));
    let modes: Vec<String> = info
        .lighting_modes
        .modes
        .iter()
        .map(u8::to_string)
        .collect();
    println!("  light modes  {}", modes.join(" "));
    let s = &info.status;
    let features: Vec<&str> = [
        (s.socd, "SOCD"),
        (s.dead_band, "dead band"),
        (s.switch_types, "switch types"),
        (s.calibration, "calibration"),
        (s.custom_lighting, "per-key RGB"),
        (s.music_rhythm, "music rhythm"),
        (s.gamepad, "gamepad"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect();
    println!("  firmware has {}", features.join(", "));
    let caps = f.model.backend.capabilities();
    let tunable: Vec<&str> = [
        (caps.actuation, "actuation"),
        (caps.rapid_trigger, "rapid trigger"),
        (caps.lighting, "lighting"),
    ]
    .into_iter()
    .filter_map(|(on, name)| on.then_some(name))
    .collect();
    println!(
        "  omakboard    {}; {}",
        f.model.support.describe(),
        if tunable.is_empty() {
            "reads only".to_owned()
        } else {
            format!("tunes {}", tunable.join(", "))
        }
    );
    Ok(())
}

/// Everything read before a change, with the session that read it.
struct Reading<'a> {
    found: &'a Found,
    session: Session<Locked>,
    info: Info,
    triggers: Vec<KeyTrigger>,
}

fn read_all(found: &[Found]) -> Result<Reading<'_>, String> {
    let (found, mut session) = open(found)?;
    let info = snapshot::read_info(&mut session, found.model).map_err(|e| e.to_string())?;
    let triggers =
        snapshot::read_triggers(&mut session, found.model.layout).map_err(|e| e.to_string())?;
    Ok(Reading {
        found,
        session,
        info,
        triggers,
    })
}

fn triggers(found: &[Found], json: bool) -> Result<(), String> {
    let Reading {
        found: f,
        info,
        triggers,
        ..
    } = read_all(found)?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&triggers).expect("triggers serialise")
        );
        return Ok(());
    }
    println!(
        "{}: actuation point per key, mm (rt = rapid trigger sensitivity)\n",
        f.model.name
    );
    let range = &info.trigger_range;
    let rows = triggers
        .iter()
        .map(|k| k.index / COLUMNS)
        .max()
        .unwrap_or(0);
    for row in 0..=rows {
        let line: Vec<String> = triggers
            .iter()
            .filter(|k| k.index / COLUMNS == row)
            .map(|k| cell(k, range))
            .collect();
        if !line.is_empty() {
            println!("  {}", line.join("  "));
        }
    }
    let rapid = triggers
        .iter()
        .filter(|k| k.trigger.mode.is_rapid())
        .count();
    println!("\n  {} keys, {} with rapid trigger", triggers.len(), rapid);
    Ok(())
}

fn cell(k: &KeyTrigger, range: &TriggerRange) -> String {
    let t = &k.trigger;
    let rt = match t.mode {
        TriggerMode::Fixed => String::new(),
        TriggerMode::RapidTrigger => format!(" rt {:.2}", range.mm(t.press)),
        TriggerMode::RapidTriggerSeparate => {
            format!(" rt {:.2}/{:.2}", range.mm(t.press), range.mm(t.release))
        }
        TriggerMode::Other(b) => format!(" mode {b:#04x}"),
    };
    format!("{} {:.2}{rt}", k.label, range.mm(t.travel))
}

fn describe_trigger(t: &Trigger, range: &TriggerRange) -> String {
    let depth = format!("{:.2} mm", range.mm(t.travel));
    match t.mode {
        TriggerMode::Fixed => depth,
        TriggerMode::RapidTrigger => format!("{depth}, rapid trigger {:.2} mm", range.mm(t.press)),
        TriggerMode::RapidTriggerSeparate => {
            format!(
                "{depth}, rapid trigger {:.2} mm press / {:.2} mm release",
                range.mm(t.press),
                range.mm(t.release)
            )
        }
        TriggerMode::Other(b) => format!("{depth}, mode {b:#04x}"),
    }
}

fn describe_lighting(l: &Lighting) -> String {
    let mode = l
        .mode_name()
        .map(str::to_owned)
        .unwrap_or_else(|| format!("mode {}", l.mode));
    let colour = if l.full_color {
        "rainbow".to_owned()
    } else {
        format!("colour {}", l.foreground)
    };
    format!(
        "{mode}, brightness {}, speed {}, {colour}",
        l.brightness, l.speed
    )
}

fn parse_rgb(text: &str) -> Result<Rgb, String> {
    let hex = text.trim().trim_start_matches('#');
    let bad = || format!("{text:?} is not a colour like ff8800");
    if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(bad());
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| bad());
    Ok(Rgb {
        r: byte(0)?,
        g: byte(2)?,
        b: byte(4)?,
    })
}

/// What a change command will do.
struct Plan {
    triggers: Vec<KeyChange>,
    lighting: Option<(Lighting, Lighting)>,
}

fn trigger_plan(r: &Reading, args: &KeyArgs, edit: TriggerEdit) -> Result<Plan, String> {
    let layout = r.found.model.layout;
    let selected = if args.all {
        layout.to_vec()
    } else if args.keys.is_empty() {
        return Err("name the keys to change (W A S D, Space, ...) or pass --all".to_owned());
    } else {
        apply::select_keys(layout, &args.keys)?
    };
    let range = &r.info.trigger_range;
    let triggers = apply::trigger_changes(&r.triggers, layout, |k| {
        selected
            .iter()
            .any(|s| s.index == k.index)
            .then(|| apply::edit_trigger(k.trigger, edit, range))
    });
    Ok(Plan {
        triggers,
        lighting: None,
    })
}

fn restore_plan(r: &Reading, file: &Path) -> Result<Plan, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let saved: SavedBackup = serde_json::from_str(&text)
        .map_err(|e| format!("{} is not an Omakboard backup: {e}", file.display()))?;
    if saved.schema != 1 {
        return Err(format!(
            "{} uses backup schema {}, which this version cannot read",
            file.display(),
            saved.schema
        ));
    }
    let (now, then) = (r.info.identity.chip(), saved.info.identity.chip());
    if now != then {
        return Err(format!(
            "{} is from a {} ({}), not this keyboard ({})",
            file.display(),
            saved.model,
            then.unwrap_or("?"),
            now.unwrap_or("?")
        ));
    }
    let layout = r.found.model.layout;
    if let Some(unknown) = saved
        .triggers
        .iter()
        .find(|s| !layout.iter().any(|k| k.code == s.code))
    {
        return Err(format!(
            "{} names a key this layout lacks: {}",
            file.display(),
            unknown.code
        ));
    }
    let triggers = apply::trigger_changes(&r.triggers, layout, |k| {
        saved
            .triggers
            .iter()
            .find(|s| s.code == k.code)
            .map(|s| s.trigger)
    });
    let from = r.info.lighting;
    let to = aula::stored_lighting(&saved.info.lighting);
    Ok(Plan {
        triggers,
        lighting: (from != to).then_some((from, to)),
    })
}

/// Read, plan, show, back up, write, read back.
fn change(
    found: &[Found],
    dry_run: bool,
    plan: impl FnOnce(&Reading) -> Result<Plan, String>,
) -> Result<(), String> {
    let mut r = read_all(found)?;
    let model = r.found.model;
    if model.support != Support::Verified {
        return Err(format!(
            "{} is not verified for changes yet ({})",
            model.name,
            model.support.describe()
        ));
    }
    let plan = plan(&r)?;
    let caps = model.backend.capabilities();
    let range = r.info.trigger_range;

    if plan.triggers.is_empty() && plan.lighting.is_none() {
        println!("Nothing to change: the keyboard already has these settings.");
        return Ok(());
    }
    // Check everything before printing or sending anything.
    for c in &plan.triggers {
        let rapid_changed = c.from.mode != c.to.mode
            || c.from.press != c.to.press
            || c.from.release != c.to.release;
        if (c.from.travel != c.to.travel && !caps.actuation)
            || (rapid_changed && !caps.rapid_trigger)
        {
            return Err(format!(
                "Omakboard cannot change this setting on the {} yet",
                model.name
            ));
        }
    }
    let frames = apply::trigger_frames(&plan.triggers, &range).map_err(|e| e.to_string())?;
    if let Some((_, to)) = &plan.lighting {
        if !caps.lighting {
            return Err(format!(
                "Omakboard cannot change lighting on the {} yet",
                model.name
            ));
        }
        aula::validate_lighting(to, &r.info.lighting_modes, r.info.lighting.mode)
            .map_err(|e| e.to_string())?;
    }

    for c in &plan.triggers {
        let (mut from, mut to) = (
            describe_trigger(&c.from, &range),
            describe_trigger(&c.to, &range),
        );
        if from == to {
            // Only the sensitivities a fixed key keeps for rapid trigger differ.
            let sensitivity = |t: &Trigger| {
                format!(
                    " (stored rapid trigger {:.2}/{:.2} mm)",
                    range.mm(t.press),
                    range.mm(t.release)
                )
            };
            from += &sensitivity(&c.from);
            to += &sensitivity(&c.to);
        }
        println!("  {:<7} {from}  ->  {to}", c.key.label);
    }
    if let Some((from, to)) = &plan.lighting {
        println!(
            "  lighting  {}  ->  {}",
            describe_lighting(from),
            describe_lighting(to)
        );
    }

    if dry_run {
        // A mode change first leaves music rhythm, as apply_lighting does.
        let lighting = plan.lighting.iter().flat_map(|(from, to)| {
            let leave = (from.mode != to.mode).then(aula::leave_music_rhythm_frame);
            leave.into_iter().chain([aula::lighting_frame(to)])
        });
        println!("\nDry run: these frames would be sent, and nothing was.");
        for f in frames.iter().copied().chain(lighting) {
            println!(
                "  {}",
                snapshot::trace_lines(&[(omakboard::session::Direction::Out, f.to_vec())])[0].hex
            );
        }
        return Ok(());
    }

    let (backup, _, _) = save_backup(&r, None)?;
    let undo = format!(
        "Your earlier settings are saved; put them back with:\n  omakboard restore {}",
        backup.display()
    );
    if !plan.triggers.is_empty() {
        apply::apply_triggers(&mut r.session, &plan.triggers, &range)
            .map_err(|e| format!("{e}\n{undo}"))?;
    }
    if let Some((_, to)) = &plan.lighting {
        let from = r.info.lighting;
        apply::apply_lighting(&mut r.session, &r.info.lighting_modes, &from, to)
            .map_err(|e| format!("{e}\n{undo}"))?;
    }
    let what = match (plan.triggers.len(), plan.lighting.is_some()) {
        (0, _) => "Lighting".to_owned(),
        (n, false) => format!("{n} key{}", if n == 1 { "" } else { "s" }),
        (n, true) => format!("{n} key{} and lighting", if n == 1 { "" } else { "s" }),
    };
    println!(
        "\n{what} written and read back. Undo with:\n  omakboard restore {}",
        backup.display()
    );
    Ok(())
}

/// Writes a backup of `r`; returns its path and how many keys and frames it holds.
fn save_backup(r: &Reading, output: Option<PathBuf>) -> Result<(PathBuf, usize, usize), String> {
    let path = match output {
        Some(path) => path,
        None => snapshot::default_backup_path(&r.found.model.usb_id())?,
    };
    let frames =
        snapshot::save_backup(&path, r.found.model, &r.info, &r.triggers, &r.session.trace)?;
    Ok((path, r.triggers.len(), frames))
}

fn print_devices(found: &[Found]) {
    if found.is_empty() {
        println!(
            "No supported keyboard is plugged in. `omakboard doctor` lists the models it knows."
        );
        return;
    }
    for f in found {
        let access = match f.access {
            Access::Ok => "reachable",
            Access::Denied => "permission denied (see `omakboard doctor`)",
            Access::Missing => "gone",
        };
        println!(
            "{}  {}  {}  {}",
            f.model.name,
            f.model.usb_id(),
            f.node.path.display(),
            access
        );
        println!("  {}", f.model.support.describe());
    }
}
