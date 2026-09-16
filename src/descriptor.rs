//! A HID report descriptor reader, just deep enough to recognise a configuration interface.
//!
//! It walks the short items and reports each top-level application collection: its usage
//! page and usage, and how many bits every report ID carries in each direction. Discovery
//! matches on that shape instead of trusting USB interface numbers, which vendors reshuffle
//! between firmware revisions.

use std::collections::BTreeMap;

/// One top-level application collection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Collection {
    pub usage_page: u16,
    pub usage: u16,
    /// Report ID (0 when the descriptor declares none) to its sizes.
    pub reports: BTreeMap<u8, ReportBits>,
}

/// Bits of data a report ID carries, excluding the report ID byte itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ReportBits {
    pub input: u32,
    pub output: u32,
    pub feature: u32,
}

impl ReportBits {
    pub fn input_bytes(&self) -> usize {
        self.input.div_ceil(8) as usize
    }

    pub fn output_bytes(&self) -> usize {
        self.output.div_ceil(8) as usize
    }
}

#[derive(Clone, Copy, Default)]
struct Globals {
    usage_page: u16,
    report_size: u32,
    report_count: u32,
    report_id: u8,
}

/// Parses `bytes`. A truncated trailing item ends the walk; everything before it is kept.
pub fn parse(bytes: &[u8]) -> Vec<Collection> {
    let mut collections: Vec<Collection> = Vec::new();
    let mut globals = Globals::default();
    let mut stack: Vec<Globals> = Vec::new();
    // Each usage keeps its own page when it was given as a 4-byte extended usage.
    let mut usages: Vec<(Option<u16>, u16)> = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;

    while i < bytes.len() {
        let prefix = bytes[i];
        if prefix == 0xFE {
            // Long item: size byte, tag byte, data. None matter here.
            let Some(&size) = bytes.get(i + 1) else { break };
            i += 3 + size as usize;
            continue;
        }
        let size = match prefix & 0x03 {
            3 => 4,
            n => n as usize,
        };
        let Some(data) = bytes.get(i + 1..i + 1 + size) else {
            break;
        };
        let value = data
            .iter()
            .rev()
            .fold(0u32, |acc, &b| (acc << 8) | b as u32);
        i += 1 + size;

        match (prefix >> 2) & 0x03 {
            // Main items
            0 => {
                let tag = prefix >> 4;
                match tag {
                    0x8 | 0x9 | 0xB => {
                        if let Some(current) = collections.last_mut().filter(|_| depth > 0) {
                            let bits = globals.report_size * globals.report_count;
                            let entry = current.reports.entry(globals.report_id).or_default();
                            match tag {
                                0x8 => entry.input += bits,
                                0x9 => entry.output += bits,
                                _ => entry.feature += bits,
                            }
                        }
                    }
                    0xA => {
                        if depth == 0 {
                            let (page, usage) = usages.first().copied().unwrap_or((None, 0));
                            let usage_page = page.unwrap_or(globals.usage_page);
                            collections.push(Collection {
                                usage_page,
                                usage,
                                reports: BTreeMap::new(),
                            });
                        }
                        depth += 1;
                    }
                    0xC => depth = depth.saturating_sub(1),
                    _ => {}
                }
                usages.clear();
            }
            // Global items
            1 => match prefix >> 4 {
                0x0 => globals.usage_page = value as u16,
                0x7 => globals.report_size = value,
                0x8 => globals.report_id = value as u8,
                0x9 => globals.report_count = value,
                0xA => stack.push(globals),
                0xB => globals = stack.pop().unwrap_or(globals),
                _ => {}
            },
            // Local items: only Usage is needed, to name a collection.
            2 if prefix >> 4 == 0x0 => {
                // A 4-byte usage carries its own page in the high word.
                let page = (size == 4).then_some((value >> 16) as u16);
                usages.push((page, value as u16));
            }
            _ => {}
        }
    }
    collections
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The WIN 60 HE's three descriptors, as read from sysfs (notes/hardware.md).
    pub const WIN60_IFACE0: &[u8] = &[
        0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x05, 0x07, 0x19, 0xe0, 0x29, 0xe7, 0x15, 0x00, 0x25,
        0x01, 0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x01, 0x95, 0x05,
        0x75, 0x01, 0x05, 0x08, 0x19, 0x01, 0x29, 0x05, 0x91, 0x02, 0x95, 0x01, 0x75, 0x03, 0x91,
        0x01, 0x95, 0x06, 0x75, 0x08, 0x15, 0x00, 0x25, 0x65, 0x05, 0x07, 0x19, 0x00, 0x29, 0x65,
        0x81, 0x00, 0xc0,
    ];
    pub const WIN60_IFACE1: &[u8] = &[
        0x05, 0x01, 0x09, 0x02, 0xa1, 0x01, 0x85, 0x02, 0x09, 0x01, 0xa1, 0x00, 0x05, 0x09, 0x19,
        0x01, 0x29, 0x05, 0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x05, 0x81, 0x02, 0x75, 0x03,
        0x95, 0x01, 0x81, 0x01, 0x05, 0x01, 0x09, 0x30, 0x09, 0x31, 0x16, 0x01, 0x80, 0x26, 0xff,
        0x7f, 0x75, 0x10, 0x95, 0x02, 0x81, 0x06, 0x09, 0x38, 0x15, 0x81, 0x25, 0x7f, 0x75, 0x08,
        0x95, 0x01, 0x81, 0x06, 0xc0, 0xc0, 0x05, 0x0c, 0x09, 0x01, 0xa1, 0x01, 0x85, 0x03, 0x19,
        0x00, 0x2a, 0x3c, 0x02, 0x15, 0x00, 0x26, 0x3c, 0x02, 0x95, 0x01, 0x75, 0x10, 0x81, 0x00,
        0xc0, 0x05, 0x01, 0x09, 0x80, 0xa1, 0x01, 0x85, 0x04, 0x19, 0x81, 0x29, 0x83, 0x15, 0x00,
        0x25, 0x01, 0x75, 0x01, 0x95, 0x03, 0x81, 0x02, 0x95, 0x05, 0x81, 0x01, 0xc0, 0x05, 0x01,
        0x09, 0x06, 0xa1, 0x01, 0x85, 0x05, 0x05, 0x07, 0x75, 0x01, 0x95, 0xa0, 0x15, 0x00, 0x25,
        0x01, 0x19, 0x00, 0x29, 0x9f, 0x81, 0x02, 0xc0,
    ];
    pub const WIN60_IFACE2: &[u8] = &[
        0x06, 0x1b, 0xff, 0x09, 0x91, 0xa1, 0x01, 0x85, 0x01, 0x19, 0x00, 0x2a, 0xff, 0x00, 0x15,
        0x00, 0x26, 0xff, 0x00, 0x75, 0x08, 0x95, 0x3f, 0x81, 0x02, 0x19, 0x00, 0x29, 0xff, 0x91,
        0x02, 0xc0,
    ];

    fn bits(input: u32, output: u32) -> ReportBits {
        ReportBits {
            input,
            output,
            feature: 0,
        }
    }

    #[test]
    fn vendor_interface_is_one_63_byte_report_each_way() {
        let c = parse(WIN60_IFACE2);
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].usage_page, c[0].usage), (0xFF1B, 0x91));
        assert_eq!(c[0].reports, BTreeMap::from([(1, bits(504, 504))]));
        assert_eq!(c[0].reports[&1].output_bytes(), 63);
    }

    #[test]
    fn boot_keyboard_has_no_report_id() {
        let c = parse(WIN60_IFACE0);
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].usage_page, c[0].usage), (0x01, 0x06));
        assert_eq!(c[0].reports, BTreeMap::from([(0, bits(64, 8))]));
    }

    #[test]
    fn composite_interface_splits_into_its_collections() {
        let c = parse(WIN60_IFACE1);
        let shape: Vec<_> = c
            .iter()
            .map(|c| (c.usage_page, c.usage, c.reports.clone()))
            .collect();
        assert_eq!(
            shape,
            vec![
                (0x01, 0x02, BTreeMap::from([(2, bits(48, 0))])),
                (0x0C, 0x01, BTreeMap::from([(3, bits(16, 0))])),
                (0x01, 0x80, BTreeMap::from([(4, bits(8, 0))])),
                (0x01, 0x06, BTreeMap::from([(5, bits(160, 0))])),
            ]
        );
    }

    #[test]
    fn truncated_descriptor_keeps_what_parsed() {
        let c = parse(&WIN60_IFACE2[..WIN60_IFACE2.len() - 4]);
        assert_eq!((c[0].usage_page, c[0].usage), (0xFF1B, 0x91));
        assert_eq!(c[0].reports[&1], bits(504, 0));
    }

    #[test]
    fn four_byte_usage_carries_its_own_page() {
        // Usage Page 0x01, then a 4-byte usage naming page 0xFF60 usage 0x61 (the VIA shape).
        let c = parse(&[0x05, 0x01, 0x0B, 0x61, 0x00, 0x60, 0xFF, 0xA1, 0x01, 0xC0]);
        assert_eq!((c[0].usage_page, c[0].usage), (0xFF60, 0x61));
    }
}
