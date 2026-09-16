//! Finds supported keyboards: registry models whose hidraw node exposes their backend's
//! configuration interface.

use std::io;

use serde::Serialize;

use crate::descriptor;
use crate::hidraw::{self, Access, Node, Sysfs};
use crate::proto::Capabilities;
use crate::registry::{self, Model, Support};

pub struct Found {
    pub model: &'static Model,
    pub node: Node,
    pub access: Access,
}

pub fn discover(sysfs: &Sysfs) -> io::Result<Vec<Found>> {
    Ok(hidraw::enumerate(sysfs)?
        .into_iter()
        .filter_map(|node| {
            let model = registry::lookup(node.vendor, node.product)?;
            let collections = descriptor::parse(&node.descriptor);
            model
                .backend
                .config_interface()
                .matches(&collections)
                .then(|| {
                    let access = hidraw::access(&node.path);
                    Found {
                        model,
                        node,
                        access,
                    }
                })
        })
        .collect())
}

/// `omakboard devices --json`. The bar widget reads this; bump `schema` on any breaking change.
#[derive(Serialize)]
pub struct Report {
    pub schema: u32,
    pub version: &'static str,
    pub devices: Vec<Device>,
}

#[derive(Serialize)]
pub struct Device {
    pub name: &'static str,
    pub usb_id: String,
    pub support: Support,
    pub backend: &'static str,
    pub node: String,
    pub interface: Option<u8>,
    pub access: Access,
    pub capabilities: Capabilities,
}

pub fn report(found: &[Found]) -> Report {
    Report {
        schema: 1,
        version: env!("CARGO_PKG_VERSION"),
        devices: found
            .iter()
            .map(|f| Device {
                name: f.model.name,
                usb_id: f.model.usb_id(),
                support: f.model.support,
                backend: f.model.backend.id(),
                node: f.node.path.display().to_string(),
                interface: f.node.interface,
                access: f.access,
                capabilities: f.model.backend.capabilities(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::descriptor::tests::{WIN60_IFACE0, WIN60_IFACE1, WIN60_IFACE2};
    use crate::hidraw::tests::FakeSysfs;

    #[test]
    fn picks_the_configuration_node_by_descriptor_not_number() {
        let fake = FakeSysfs::new();
        let id = "0003:00002E3C:0000C365";
        // Numbered so the vendor interface is neither first nor last.
        fake.add("hidraw7", "1-10.3:1.0", id, "WIN 60 HE", WIN60_IFACE0);
        fake.add("hidraw1", "1-10.3:1.2", id, "WIN 60 HE", WIN60_IFACE2);
        fake.add("hidraw3", "1-10.3:1.1", id, "WIN 60 HE", WIN60_IFACE1);
        let found = discover(&fake.sysfs).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].node.name, "hidraw1");
        assert_eq!(found[0].node.interface, Some(2));
        // The fake /dev has no node file.
        assert_eq!(found[0].access, Access::Missing);
    }

    #[test]
    fn unknown_devices_are_ignored_even_with_a_matching_descriptor() {
        let fake = FakeSysfs::new();
        fake.add(
            "hidraw0",
            "1-2:1.2",
            "0003:00001234:00005678",
            "Other",
            WIN60_IFACE2,
        );
        assert!(discover(&fake.sysfs).unwrap().is_empty());
    }

    #[test]
    fn report_json_shape() {
        let fake = FakeSysfs::new();
        fake.add(
            "hidraw4",
            "1-10.3:1.2",
            "0003:00002E3C:0000C365",
            "WIN 60 HE",
            WIN60_IFACE2,
        );
        let json = serde_json::to_value(report(&discover(&fake.sysfs).unwrap())).unwrap();
        assert_eq!(json["schema"], 1);
        let d = &json["devices"][0];
        assert_eq!(d["name"], "AULA WIN 60 HE");
        assert_eq!(d["usb_id"], "2e3c:c365");
        assert_eq!(d["support"], "verified");
        assert_eq!(d["backend"], "aula-ff1b");
        assert_eq!(d["interface"], 2);
        assert_eq!(d["access"], "missing");
        assert_eq!(d["capabilities"]["lighting"], true);
        assert_eq!(d["capabilities"]["socd"], false);
    }
}
