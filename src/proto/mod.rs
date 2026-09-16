//! Protocol backends. Each vendor protocol family implements [`Backend`].
//!
//! A backend says which HID collection it talks over and what it can change. Until a
//! protocol is decoded from captures, its capabilities stay empty and nothing is written.

pub mod aula_ff1b;

use serde::Serialize;
use thiserror::Error;

use crate::descriptor::Collection;

/// The HID collection a backend configures the keyboard through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigInterface {
    pub usage_page: u16,
    pub usage: u16,
    pub report_id: u8,
    /// Payload bytes per report, excluding the report ID, in both directions.
    pub payload_len: usize,
}

impl ConfigInterface {
    /// Whether a node's descriptor exposes exactly this interface.
    pub fn matches(&self, collections: &[Collection]) -> bool {
        collections.iter().any(|c| {
            c.usage_page == self.usage_page
                && c.usage == self.usage
                && c.reports.get(&self.report_id).is_some_and(|r| {
                    r.input_bytes() == self.payload_len && r.output_bytes() == self.payload_len
                })
        })
    }

    /// Wraps a payload in one output report: report ID, payload, zero padding.
    pub fn frame(&self, payload: &[u8]) -> Result<Vec<u8>, ProtoError> {
        if payload.len() > self.payload_len {
            return Err(ProtoError::PayloadTooLong {
                len: payload.len(),
                max: self.payload_len,
            });
        }
        let mut report = vec![0u8; 1 + self.payload_len];
        report[0] = self.report_id;
        report[1..=payload.len()].copy_from_slice(payload);
        Ok(report)
    }
}

/// What a backend can read and change. Everything starts false and is switched on per
/// feature as its encoding is decoded and verified on hardware.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    pub actuation: bool,
    pub rapid_trigger: bool,
    pub socd: bool,
    pub dks: bool,
    /// Lighting mode, brightness, speed and colours.
    pub lighting: bool,
    pub per_key_rgb: bool,
    pub profiles: bool,
}

impl Capabilities {
    pub fn any(&self) -> bool {
        self.actuation
            || self.rapid_trigger
            || self.socd
            || self.dks
            || self.lighting
            || self.per_key_rgb
            || self.profiles
    }
}

#[derive(Debug, Error)]
pub enum ProtoError {
    #[error("payload is {len} bytes; one report holds {max}")]
    PayloadTooLong { len: usize, max: usize },
}

pub trait Backend: Sync {
    /// Stable id, used in JSON output and issue reports.
    fn id(&self) -> &'static str;
    fn config_interface(&self) -> ConfigInterface;
    fn capabilities(&self) -> Capabilities;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::{parse, tests::*};

    const FF1B: ConfigInterface = ConfigInterface {
        usage_page: 0xFF1B,
        usage: 0x91,
        report_id: 1,
        payload_len: 63,
    };

    #[test]
    fn matches_only_the_vendor_interface() {
        assert!(FF1B.matches(&parse(WIN60_IFACE2)));
        assert!(!FF1B.matches(&parse(WIN60_IFACE0)));
        assert!(!FF1B.matches(&parse(WIN60_IFACE1)));
    }

    #[test]
    fn a_different_report_length_does_not_match() {
        let shorter = ConfigInterface {
            payload_len: 32,
            ..FF1B
        };
        assert!(!shorter.matches(&parse(WIN60_IFACE2)));
    }

    #[test]
    fn frames_pad_to_a_full_report() {
        let report = FF1B.frame(&[0xAA, 0xBB]).unwrap();
        assert_eq!(report.len(), 64);
        assert_eq!(&report[..3], &[0x01, 0xAA, 0xBB]);
        assert!(report[3..].iter().all(|&b| b == 0));
    }

    #[test]
    fn oversized_payload_is_refused() {
        assert!(matches!(
            FF1B.frame(&[0; 64]),
            Err(ProtoError::PayloadTooLong { len: 64, max: 63 })
        ));
    }
}
