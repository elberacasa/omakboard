//! Omakboard: tune Hall-effect keyboards on Omarchy.

pub mod apply;
pub mod descriptor;
pub mod discover;
pub mod doctor;
pub mod hidraw;
pub mod layout;
pub mod lock;
pub mod proto;
pub mod registry;
pub mod serve;
pub mod session;
#[cfg(test)]
pub mod sim;
pub mod snapshot;
