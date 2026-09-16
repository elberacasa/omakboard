//! Linux hidraw: finding nodes through sysfs, checking access, and a blocking transport.
//!
//! Nothing is hardcoded to a hidraw number. Each node is read from sysfs: its HID id, name,
//! report descriptor and USB interface. Opening a node sends nothing to the device.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Serialize;
use thiserror::Error;

/// Where hidraw nodes are listed and where their device files live.
#[derive(Debug, Clone)]
pub struct Sysfs {
    pub class_dir: PathBuf,
    pub dev_dir: PathBuf,
}

impl Default for Sysfs {
    fn default() -> Self {
        Self {
            class_dir: "/sys/class/hidraw".into(),
            dev_dir: "/dev".into(),
        }
    }
}

/// One hidraw node, as sysfs describes it.
#[derive(Debug, Clone)]
pub struct Node {
    /// `hidraw4`
    pub name: String,
    /// `/dev/hidraw4`
    pub path: PathBuf,
    pub vendor: u16,
    pub product: u16,
    pub hid_name: String,
    /// The USB interface number, when the node sits on a USB interface.
    pub interface: Option<u8>,
    pub descriptor: Vec<u8>,
}

impl Node {
    pub fn usb_id(&self) -> String {
        format!("{:04x}:{:04x}", self.vendor, self.product)
    }
}

/// Lists every hidraw node. Nodes whose sysfs entries cannot be read are skipped, since a
/// device unplugged mid-scan is normal.
pub fn enumerate(sysfs: &Sysfs) -> io::Result<Vec<Node>> {
    let mut nodes = Vec::new();
    for entry in fs::read_dir(&sysfs.class_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(node) = read_node(sysfs, &name) {
            nodes.push(node);
        }
    }
    nodes.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(nodes)
}

fn read_node(sysfs: &Sysfs, name: &str) -> Option<Node> {
    let device = sysfs.class_dir.join(name).join("device");
    let uevent = fs::read_to_string(device.join("uevent")).ok()?;
    let (vendor, product) = parse_hid_id(&uevent)?;
    let hid_name = uevent
        .lines()
        .find_map(|l| l.strip_prefix("HID_NAME="))
        .unwrap_or_default()
        .to_owned();
    let descriptor = fs::read(device.join("report_descriptor")).ok()?;
    let interface = fs::canonicalize(&device)
        .ok()
        .and_then(|p| {
            p.parent()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().into_owned())
        })
        .and_then(|n| parse_interface(&n));
    Some(Node {
        name: name.to_owned(),
        path: sysfs.dev_dir.join(name),
        vendor,
        product,
        hid_name,
        interface,
        descriptor,
    })
}

/// Parses `HID_ID=0003:00002E3C:0000C365` from a HID device's uevent.
fn parse_hid_id(uevent: &str) -> Option<(u16, u16)> {
    let id = uevent.lines().find_map(|l| l.strip_prefix("HID_ID="))?;
    let mut parts = id.split(':').skip(1);
    let vendor = u32::from_str_radix(parts.next()?, 16).ok()?;
    let product = u32::from_str_radix(parts.next()?, 16).ok()?;
    Some((u16::try_from(vendor).ok()?, u16::try_from(product).ok()?))
}

/// Parses the interface number from a USB interface directory name, `1-10.3:1.2` -> 2.
fn parse_interface(dir: &str) -> Option<u8> {
    let (port, config_iface) = dir.split_once(':')?;
    let (config, iface) = config_iface.split_once('.')?;
    let port_ok = !port.is_empty()
        && port
            .chars()
            .all(|c| c.is_ascii_digit() || c == '-' || c == '.');
    let config_ok = !config.is_empty() && config.chars().all(|c| c.is_ascii_digit());
    if !port_ok || !config_ok || !iface.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    iface.parse().ok()
}

/// Whether the current user can open a node for reading and writing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Access {
    Ok,
    Denied,
    Missing,
}

pub fn access(path: &Path) -> Access {
    match OpenOptions::new().read(true).write(true).open(path) {
        Ok(_) => Access::Ok,
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => Access::Denied,
        Err(_) => Access::Missing,
    }
}

#[derive(Debug, Error)]
pub enum TransportError {
    #[error(
        "permission denied opening {0}; install Omakboard's udev rule \
         (packaging/udev/70-omakboard.rules) and replug the keyboard"
    )]
    PermissionDenied(PathBuf),
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("the keyboard answered with an empty report")]
    Empty,
}

/// Sends and receives whole HID reports. The first byte is the report ID.
pub trait Transport {
    fn send(&mut self, report: &[u8]) -> Result<(), TransportError>;
    /// Waits up to `timeout` for one report; `None` when nothing arrived.
    fn recv(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, TransportError>;
}

/// A hidraw node opened for reading and writing.
pub struct Hidraw {
    file: File,
    path: PathBuf,
}

impl Hidraw {
    pub fn open(path: &Path) -> Result<Self, TransportError> {
        match OpenOptions::new().read(true).write(true).open(path) {
            Ok(file) => Ok(Self {
                file,
                path: path.to_owned(),
            }),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                Err(TransportError::PermissionDenied(path.to_owned()))
            }
            Err(source) => Err(TransportError::Io {
                path: path.to_owned(),
                source,
            }),
        }
    }

    fn io(&self, source: io::Error) -> TransportError {
        TransportError::Io {
            path: self.path.clone(),
            source,
        }
    }
}

impl Transport for Hidraw {
    fn send(&mut self, report: &[u8]) -> Result<(), TransportError> {
        self.file.write_all(report).map_err(|e| self.io(e))
    }

    fn recv(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, TransportError> {
        let mut pfd = libc::pollfd {
            fd: self.file.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let ms = libc::c_int::try_from(timeout.as_millis()).unwrap_or(libc::c_int::MAX);
        // SAFETY: `pfd` is a valid pollfd for an open descriptor, and the count is 1.
        let ready = unsafe { libc::poll(&mut pfd, 1, ms) };
        if ready < 0 {
            return Err(self.io(io::Error::last_os_error()));
        }
        if ready == 0 {
            return Ok(None);
        }
        let mut buf = [0u8; 1024];
        let n = self.file.read(&mut buf).map_err(|e| self.io(e))?;
        if n == 0 {
            return Err(TransportError::Empty);
        }
        Ok(Some(buf[..n].to_vec()))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::descriptor::tests::{WIN60_IFACE0, WIN60_IFACE2};
    use std::os::unix::fs::symlink;
    use std::sync::atomic::{AtomicU32, Ordering};

    /// A throwaway sysfs tree shaped like the real one: class entries whose `device` links
    /// into a USB interface directory.
    pub struct FakeSysfs {
        pub root: PathBuf,
        pub sysfs: Sysfs,
    }

    impl FakeSysfs {
        pub fn new() -> Self {
            static N: AtomicU32 = AtomicU32::new(0);
            let root = std::env::temp_dir().join(format!(
                "omakboard-test-{}-{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let sysfs = Sysfs {
                class_dir: root.join("class/hidraw"),
                dev_dir: root.join("dev"),
            };
            fs::create_dir_all(&sysfs.class_dir).unwrap();
            fs::create_dir_all(&sysfs.dev_dir).unwrap();
            Self { root, sysfs }
        }

        pub fn add(
            &self,
            name: &str,
            interface_dir: &str,
            hid_id: &str,
            hid_name: &str,
            descriptor: &[u8],
        ) {
            let device = self
                .root
                .join("devices")
                .join(interface_dir)
                .join(format!("{hid_id}.0001"));
            fs::create_dir_all(&device).unwrap();
            fs::write(
                device.join("uevent"),
                format!("DRIVER=hid-generic\nHID_ID={hid_id}\nHID_NAME={hid_name}\n"),
            )
            .unwrap();
            fs::write(device.join("report_descriptor"), descriptor).unwrap();
            let class = self.sysfs.class_dir.join(name);
            fs::create_dir_all(&class).unwrap();
            symlink(&device, class.join("device")).unwrap();
        }
    }

    impl Drop for FakeSysfs {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn reads_ids_names_interfaces_and_descriptors() {
        let fake = FakeSysfs::new();
        fake.add(
            "hidraw4",
            "1-10.3:1.2",
            "0003:00002E3C:0000C365",
            "WIN 60 HE",
            WIN60_IFACE2,
        );
        fake.add(
            "hidraw2",
            "1-10.3:1.0",
            "0003:00002E3C:0000C365",
            "WIN 60 HE",
            WIN60_IFACE0,
        );
        let nodes = enumerate(&fake.sysfs).unwrap();
        assert_eq!(
            nodes.iter().map(|n| n.name.as_str()).collect::<Vec<_>>(),
            ["hidraw2", "hidraw4"]
        );
        let vendor = &nodes[1];
        assert_eq!(vendor.usb_id(), "2e3c:c365");
        assert_eq!(vendor.hid_name, "WIN 60 HE");
        assert_eq!(vendor.interface, Some(2));
        assert_eq!(vendor.descriptor, WIN60_IFACE2);
        assert_eq!(vendor.path, fake.sysfs.dev_dir.join("hidraw4"));
    }

    #[test]
    fn skips_nodes_it_cannot_read() {
        let fake = FakeSysfs::new();
        fs::create_dir_all(fake.sysfs.class_dir.join("hidraw9")).unwrap();
        assert!(enumerate(&fake.sysfs).unwrap().is_empty());
    }

    #[test]
    fn parses_hid_ids_and_interfaces() {
        assert_eq!(
            parse_hid_id("HID_ID=0003:0000046D:0000C099\n"),
            Some((0x046D, 0xC099))
        );
        assert_eq!(parse_hid_id("HID_NAME=x\n"), None);
        assert_eq!(parse_interface("1-10.3:1.2"), Some(2));
        assert_eq!(parse_interface("0003:2E3C:C365.0005"), None);
    }

    #[test]
    fn missing_node_is_missing() {
        assert_eq!(access(Path::new("/nonexistent/hidraw99")), Access::Missing);
    }
}
