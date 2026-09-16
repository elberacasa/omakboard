//! Reading a keyboard's settings: the identity check, then everything readable.

use serde::Serialize;
use thiserror::Error;

use crate::hidraw::Transport;
use crate::layout::Key;
use crate::proto::aula_ff1b::{
    self as aula, DecodeError, Identity, Lighting, LightingModes, Read, Status, Trigger,
    TriggerRange,
};
use crate::registry::Model;
use crate::session::{Direction, Session, SessionError};

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error(transparent)]
    Session(#[from] SessionError),
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error(
        "this keyboard reports product {found:?} ({raw}), but Omakboard's {model} layout is for \
         {expected}; other AULA boards share this USB id, so nothing more is read"
    )]
    WrongModel {
        model: &'static str,
        expected: String,
        found: String,
        raw: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct Info {
    pub identity: Identity,
    pub status: Status,
    /// Hz, when the keyboard supports changing it.
    pub poll_rate: Option<u32>,
    pub lighting: Lighting,
    pub lighting_modes: LightingModes,
    pub trigger_range: TriggerRange,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct KeyTrigger {
    pub code: &'static str,
    pub label: &'static str,
    pub index: u8,
    pub trigger: Trigger,
}

/// Reads identity first and stops unless it names one of the model's products.
pub fn read_info<T: Transport>(
    session: &mut Session<T>,
    model: &Model,
) -> Result<Info, SnapshotError> {
    let identity = aula::parse_identity(&session.read(Read::Identity)?)?;
    if !identity
        .chip()
        .is_some_and(|chip| model.products.contains(&chip))
    {
        return Err(SnapshotError::WrongModel {
            model: model.name,
            expected: model.products.join(" or "),
            found: identity.chip().unwrap_or_default().to_owned(),
            raw: identity.raw,
        });
    }
    let status = aula::parse_status(&session.read_one(Read::Status)?)?;
    let poll_rate = if status.poll_rate {
        Some(aula::parse_poll_rate(&session.read_one(Read::PollRate)?)?)
    } else {
        None
    };
    let lighting = aula::parse_lighting(&session.read_one(Read::Lighting)?)?;
    let lighting_modes = aula::parse_lighting_modes(&session.read_one(Read::LightingModes)?)?;
    let trigger_range = aula::parse_trigger_range(
        &session.read_one(Read::TriggerRange)?,
        status.high_precision,
    )?;
    Ok(Info {
        identity,
        status,
        poll_rate,
        lighting,
        lighting_modes,
        trigger_range,
    })
}

/// Reads every key's trigger, one request per key as the web driver does.
pub fn read_triggers<T: Transport>(
    session: &mut Session<T>,
    layout: &[Key],
) -> Result<Vec<KeyTrigger>, SnapshotError> {
    layout
        .iter()
        .map(|key| {
            let reply = session.read_one(Read::Trigger {
                row: key.row(),
                column: key.column(),
            })?;
            Ok(KeyTrigger {
                code: key.code,
                label: key.label,
                index: key.index,
                trigger: aula::parse_trigger(&reply)?,
            })
        })
        .collect()
}

/// Everything read, plus the raw conversation: the record a later write is undone from.
#[derive(Serialize)]
pub struct Backup<'a> {
    pub schema: u32,
    pub omakboard: &'static str,
    pub created_unix: u64,
    pub model: &'static str,
    pub usb_id: String,
    pub info: &'a Info,
    pub triggers: &'a [KeyTrigger],
    pub trace: Vec<TraceLine>,
}

#[derive(Serialize)]
pub struct TraceLine {
    pub dir: &'static str,
    pub hex: String,
}

/// Where backups go when no path is given: `$XDG_STATE_HOME/omakboard/backups/`.
pub fn default_backup_path(usb_id: &str) -> Result<std::path::PathBuf, String> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/state"))
        })
        .ok_or("cannot find a state directory")?;
    Ok(state.join("omakboard/backups").join(format!(
        "{}-{}{:03}.json",
        usb_id.replace(':', "-"),
        now.as_secs(),
        now.subsec_millis()
    )))
}

/// Writes a backup; returns how many frames of the conversation it holds.
pub fn save_backup(
    path: &std::path::Path,
    model: &Model,
    info: &Info,
    triggers: &[KeyTrigger],
    trace: &[(Direction, Vec<u8>)],
) -> Result<usize, String> {
    let backup = Backup {
        schema: 1,
        omakboard: env!("CARGO_PKG_VERSION"),
        created_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        model: model.name,
        usb_id: model.usb_id(),
        info,
        triggers,
        trace: trace_lines(trace),
    };
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let json = serde_json::to_string_pretty(&backup).expect("backup serialises");
    std::fs::write(path, json + "\n").map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(backup.trace.len())
}

pub fn trace_lines(trace: &[(Direction, Vec<u8>)]) -> Vec<TraceLine> {
    trace
        .iter()
        .map(|(dir, payload)| {
            // Trailing zeros carry nothing: every payload is 63 bytes.
            let end = payload.iter().rposition(|&b| b != 0).map_or(1, |i| i + 1);
            TraceLine {
                dir: match dir {
                    Direction::Out => "out",
                    Direction::In => "in",
                },
                hex: payload[..end]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" "),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::WIN60_HE;
    use crate::proto::aula_ff1b::tests::*;
    use crate::proto::aula_ff1b::{Rgb, TriggerMode};
    use crate::registry::lookup;
    use crate::session::tests::FakeKeyboard;

    fn win60_keyboard(identity0: &str) -> FakeKeyboard {
        let mut answers = vec![
            (Read::Identity, vec![frame(identity0), frame(IDENTITY_1)]),
            (Read::Status, vec![frame(STATUS)]),
            (Read::PollRate, vec![frame(POLL_RATE)]),
            (Read::Lighting, vec![frame(LIGHTING)]),
            (Read::LightingModes, vec![frame(MODES)]),
            (Read::TriggerRange, vec![frame(RANGE)]),
        ];
        for key in WIN60_HE {
            answers.push((
                Read::Trigger {
                    row: key.row(),
                    column: key.column(),
                },
                vec![frame(TRIGGER_ESC)],
            ));
        }
        FakeKeyboard::new(answers)
    }

    #[test]
    fn reads_the_win60() {
        let model = lookup(0x2E3C, 0xC365).unwrap();
        let mut s = Session::new(win60_keyboard(IDENTITY_0));
        let info = read_info(&mut s, model).unwrap();
        assert_eq!(info.identity.firmware(), Some("3.13.06"));
        assert_eq!(info.poll_rate, Some(8000));
        assert_eq!(info.lighting.foreground, Rgb { r: 255, g: 0, b: 0 });
        assert_eq!(info.trigger_range.max, 340);

        let triggers = read_triggers(&mut s, model.layout).unwrap();
        assert_eq!(triggers.len(), 61);
        assert!(
            triggers
                .iter()
                .all(|k| k.trigger.mode == TriggerMode::Fixed && k.trigger.travel == 170)
        );
    }

    #[test]
    fn another_board_on_the_same_usb_id_is_refused_after_identity() {
        let model = lookup(0x2E3C, 0xC365).unwrap();
        // The WIN 68 HE: same platform and USB id, product SI2828KZHEARGB.
        let other = IDENTITY_0.replacen("32 38 32 35", "32 38 32 38", 1);
        let mut s = Session::new(win60_keyboard(&other));
        let err = read_info(&mut s, model).unwrap_err();
        assert!(
            matches!(err, SnapshotError::WrongModel { ref found, .. } if found == "SI2828KZHEARGB"),
            "{err}"
        );
        // Nothing after identity was requested.
        assert_eq!(
            s.trace.iter().filter(|(d, _)| *d == Direction::Out).count(),
            1
        );
    }

    #[test]
    fn trace_lines_trim_padding() {
        let lines = trace_lines(&[
            (Direction::Out, Read::Lighting.payload().to_vec()),
            (Direction::In, frame(POLL_RATE)),
        ]);
        assert_eq!((lines[0].dir, lines[0].hex.as_str()), ("out", "07 01"));
        assert_eq!(lines[1].hex, "21 00 00 00 02 09 08");
    }
}
