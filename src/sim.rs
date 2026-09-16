//! A simulated WIN 60 HE for tests: it answers every read Omakboard makes with the replies
//! captured from the real keyboard, and applies trigger and lighting writes to its state.

use std::collections::{HashMap, VecDeque};
use std::time::Duration;

use crate::hidraw::{Transport, TransportError};
use crate::layout::WIN60_HE;
use crate::proto::aula_ff1b::tests::*;
use crate::proto::aula_ff1b::{PAYLOAD_LEN, REPORT_ID, Trigger, TriggerMode};

pub const FACTORY: Trigger = Trigger {
    mode: TriggerMode::Fixed,
    travel: 170,
    press: 1,
    release: 1,
};

pub struct SimKeyboard {
    pub triggers: HashMap<(u8, u8), Trigger>,
    /// The 12 lighting payload bytes.
    pub lighting: [u8; 12],
    pub identity: String,
    /// Read triggers back one step deeper than written.
    pub lie: bool,
    /// Every payload received.
    pub received: Vec<Vec<u8>>,
    pub pending: VecDeque<Vec<u8>>,
}

impl Default for SimKeyboard {
    fn default() -> Self {
        let lighting_reply = frame(LIGHTING);
        let mut lighting = [0u8; 12];
        lighting.copy_from_slice(&lighting_reply[5..17]);
        Self {
            triggers: WIN60_HE
                .iter()
                .map(|k| ((k.row(), k.column()), FACTORY))
                .collect(),
            lighting,
            identity: IDENTITY_0.to_owned(),
            lie: false,
            received: Vec::new(),
            pending: VecDeque::new(),
        }
    }
}

impl SimKeyboard {
    pub fn trigger(&self, label: &str) -> Trigger {
        let k = crate::layout::find(WIN60_HE, label).unwrap();
        self.triggers[&(k.row(), k.column())]
    }

    /// How many set-trigger or lighting writes arrived.
    pub fn writes(&self) -> usize {
        self.received
            .iter()
            .filter(|p| is_trigger_write(p) || is_lighting_write(p))
            .count()
    }

    fn reply(&mut self, payload: Vec<u8>) {
        let mut report = vec![REPORT_ID];
        report.extend(payload);
        report.resize(1 + PAYLOAD_LEN, 0);
        self.pending.push_back(report);
    }
}

fn is_trigger_write(p: &[u8]) -> bool {
    p[0] == 0x21
        && p[4] == 0x18
        && [0x00, 0x0C, 0x0D].contains(&p[5])
        && p[6..28].iter().any(|&b| b != 0)
}

fn is_lighting_write(p: &[u8]) -> bool {
    p[0] == 0x07 && p[1] == 0 && p[4] == 0x0E
}

impl Transport for SimKeyboard {
    fn send(&mut self, report: &[u8]) -> Result<(), TransportError> {
        assert_eq!(report.len(), 1 + PAYLOAD_LEN);
        let p = report[1..].to_vec();
        self.received.push(p.clone());
        match (p[0], p[1], p[4], p[5]) {
            (0x0D, ..) => {
                self.reply(frame(&self.identity.clone()));
                self.reply(frame(IDENTITY_1));
            }
            (0x01, ..) => self.reply(frame(STATUS)),
            (0x0A, ..) => self.reply(frame(MODES)),
            (0x07, 0x01, ..) => {
                let mut r = frame("07 00 00 00 0c");
                r[5..17].copy_from_slice(&self.lighting);
                self.reply(r);
            }
            (0x07, 0x00, 0x0E, _) => self.lighting.copy_from_slice(&p[5..17]),
            (0x21, _, 0x18, 0x04) => self.reply(frame(RANGE)),
            (0x21, _, 0x01, 0x09) => self.reply(frame(POLL_RATE)),
            (0x21, _, 0x18, 0x05) => {
                let t = self.triggers.get(&(p[6], p[7])).copied().unwrap_or(FACTORY);
                let [th, tl] = (if self.lie { t.travel + 1 } else { t.travel }).to_be_bytes();
                let [ph, pl] = t.press.to_be_bytes();
                let [rh, rl] = t.release.to_be_bytes();
                let mode = match t.mode {
                    TriggerMode::Fixed => 0x00,
                    TriggerMode::RapidTrigger => 0x0C,
                    TriggerMode::RapidTriggerSeparate => 0x0D,
                    TriggerMode::Other(b) => b,
                };
                let mut r = frame("21 00 00 00 04 05");
                r[6..15].copy_from_slice(&[mode, tl, tl, pl, rl, th, th, ph, rh]);
                self.reply(r);
            }
            _ if is_trigger_write(&p) => {
                let t = Trigger {
                    mode: TriggerMode::from_byte(p[5]),
                    travel: u16::from_be_bytes([p[32], p[28]]),
                    press: u16::from_be_bytes([p[34], p[30]]),
                    release: u16::from_be_bytes([p[35], p[31]]),
                };
                for column in 0..22u8 {
                    for row in 0..8u8 {
                        if p[6 + usize::from(column)] & (1 << row) != 0 {
                            self.triggers.insert((row, column), t);
                        }
                    }
                }
                self.reply(frame("21 00 00 00 01"));
            }
            _ => {}
        }
        Ok(())
    }

    fn recv(&mut self, _timeout: Duration) -> Result<Option<Vec<u8>>, TransportError> {
        Ok(self.pending.pop_front())
    }
}
