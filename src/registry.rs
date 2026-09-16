//! Keyboards Omakboard knows, and how far each is supported.

use serde::Serialize;

use crate::layout::{self, Key};
use crate::proto::{Backend, aula_ff1b::AulaFf1b};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Support {
    /// Every capability the backend claims was checked on this model's hardware.
    Verified,
    /// Shares a verified backend but nobody has checked this model; writes need consent.
    Untested,
    /// Detected and readable; its protocol is being decoded, so nothing is written.
    Research,
}

impl Support {
    pub fn describe(self) -> &'static str {
        match self {
            Support::Verified => "verified on real hardware",
            Support::Untested => "untested: shares a verified protocol, not checked on this model",
            Support::Research => {
                "protocol being decoded: settings can be read, nothing is written yet"
            }
        }
    }
}

pub struct Model {
    pub vendor: u16,
    pub product: u16,
    pub name: &'static str,
    pub backend: &'static dyn Backend,
    pub support: Support,
    /// Product strings the keyboard may report in its identity (`SI2825KZHEARGB`). AULA ships
    /// several models on one USB id, so this, not the id, confirms the layout below applies.
    pub products: &'static [&'static str],
    pub layout: &'static [Key],
}

impl Model {
    pub fn usb_id(&self) -> String {
        format!("{:04x}:{:04x}", self.vendor, self.product)
    }
}

pub static MODELS: &[Model] = &[Model {
    vendor: 0x2E3C,
    product: 0xC365,
    name: "AULA WIN 60 HE",
    backend: &AulaFf1b,
    support: Support::Verified,
    products: &["SI2825KZHEARGB"],
    layout: layout::WIN60_HE,
}];

pub fn lookup(vendor: u16, product: u16) -> Option<&'static Model> {
    MODELS
        .iter()
        .find(|m| m.vendor == vendor && m.product == product)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn usb_ids_are_unique() {
        let ids: HashSet<_> = MODELS.iter().map(|m| (m.vendor, m.product)).collect();
        assert_eq!(ids.len(), MODELS.len());
    }

    #[test]
    fn a_model_claims_capabilities_only_once_out_of_research() {
        for m in MODELS {
            if m.support == Support::Research {
                assert!(
                    !m.backend.capabilities().any(),
                    "{} is in research but claims capabilities",
                    m.name
                );
            }
        }
    }

    #[test]
    fn finds_the_win60() {
        assert_eq!(
            lookup(0x2E3C, 0xC365).map(|m| m.name),
            Some("AULA WIN 60 HE")
        );
        assert!(lookup(0x046D, 0xC099).is_none());
    }
}
