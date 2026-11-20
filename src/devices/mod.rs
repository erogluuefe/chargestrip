//! Device drivers. Two sources, both producing the same list of [`Reading`]s:
//!
//! - **USB receivers:** each model is a [`Driver`]: which receiver to open (VID/PID + usage page)
//!   and how to read the battery from the opened HID interface.
//! - **Bluetooth:** the battery level the operating system already keeps for connected devices
//!   ([`bluetooth`]); no per-model code needed.
//!
//! The UI layers don't depend on the number of devices. To add a USB receiver device:
//!
//! 1. Create `src/devices/<brand_model>.rs` with `pub const DRIVER: Driver` and `fn read(...)`.
//!    Cover the protocol with unit tests built from captured frames.
//! 2. Add the file as a `mod` below and one line to [`DRIVERS`].
//!
//! Drivers only send **read-only** requests; a command that could change a setting is never sent.
//! See README.md for how to work out a protocol and the tools for it.

mod arctis_nova7;
mod bluetooth;
mod fractal_scape;
mod logitech;
mod razer;
mod waizowl_ogm;

use crate::model::{Battery, Kind};
use hidapi::{HidApi, HidDevice};
use std::time::{Duration, Instant};

/// Definition of a device model (or of a family speaking the same protocol).
pub struct Driver {
    /// Device name for the tooltip; empty to use the receiver's USB product name.
    pub name: &'static str,
    /// Icon to show (unless the driver learns the kind from the device).
    pub kind: Kind,
    /// Vendor ID of the USB receiver.
    pub vid: u16,
    /// Supported product IDs (receiver/cable variants speaking the same protocol).
    pub pids: &'static [u16],
    /// Usage page / usage of the HID interface the battery is read from; with `ANY`, every matching
    /// interface is tried in turn and the first valid reading wins.
    pub usage_page: u16,
    pub usage: u16,
    /// Reads the battery from the opened interface. `pid` tells variants of one driver apart.
    /// `None`: device off or asleep, or no valid reply.
    pub read: fn(dev: &HidDevice, pid: u16) -> Option<Found>,
}

/// For `usage_page` / `usage`: any value.
pub const ANY: u16 = 0;

/// A driver's successful reading.
pub struct Found {
    pub battery: Battery,
    /// Kind learned from the device (e.g. when a receiver can pair with a keyboard or a mouse).
    pub kind: Option<Kind>,
    /// Name learned from the device.
    pub name: Option<String>,
}

impl From<Battery> for Found {
    fn from(battery: Battery) -> Self {
        Found { battery, kind: None, name: None }
    }
}

/// One device's reading in one polling round.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reading {
    /// Stable key that identifies the device across rounds (to remember its last value).
    pub key: String,
    /// Human-readable device name, e.g. "SteelSeries Arctis Nova 7P".
    pub name: String,
    pub kind: Kind,
    /// `None`: receiver plugged in, but the device is off or asleep, or gave no valid reply.
    pub battery: Option<Battery>,
}

/// Supported USB receiver devices; shown in this order, before Bluetooth devices.
pub const DRIVERS: &[Driver] = &[
    arctis_nova7::DRIVER,
    fractal_scape::DRIVER,
    waizowl_ogm::DRIVER,
    razer::MICE,
    razer::KEYBOARDS,
    logitech::RECEIVERS,
];

/// Longest wait for a reply.
const TIMEOUT: Duration = Duration::from_millis(1000);

/// Polls plugged-in receivers and connected Bluetooth devices. Drivers without a plugged-in
/// receiver and disconnected Bluetooth devices are left out. Without `api` (HID unavailable),
/// only Bluetooth.
pub fn poll(api: Option<&mut HidApi>) -> Vec<Reading> {
    let mut readings = api.map(poll_receivers).unwrap_or_default();
    readings.extend(bluetooth::poll());
    readings
}

fn poll_receivers(api: &mut HidApi) -> Vec<Reading> {
    // Receivers come and go, so the device list is refreshed every round.
    if api.refresh_devices().is_err() {
        return Vec::new();
    }
    let api = &*api;
    DRIVERS
        .iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let mut candidates = api.device_list().filter(|info| {
                info.vendor_id() == d.vid
                    && d.pids.contains(&info.product_id())
                    && (d.usage_page == ANY || info.usage_page() == d.usage_page)
                    && (d.usage == ANY || info.usage() == d.usage)
            });
            let first = candidates.next()?; // receiver not plugged in
            let product = first.product_string().map(product_name).unwrap_or_default();
            let found = std::iter::once(first).chain(candidates).find_map(|info| {
                let dev = info.open_device(api).ok()?;
                (d.read)(&dev, info.product_id())
            });
            let (kind, name, battery) = match found {
                Some(f) => (f.kind.unwrap_or(d.kind), f.name, Some(f.battery)),
                None => (d.kind, None, None),
            };
            let name = name.unwrap_or_else(|| if d.name.is_empty() { product } else { d.name.to_string() });
            Some(Reading { key: format!("usb:{i}"), name, kind, battery })
        })
        .collect()
}

/// Plain-text report for bug reports ("Copy diagnostics" in the menu): app version and platform,
/// every vendor-specific HID interface (where battery protocols live), the other HID devices by
/// name, and what each driver and the Bluetooth source currently read.
pub fn diagnostics(mut api: Option<&mut HidApi>) -> String {
    use std::fmt::Write;
    let mut out = String::new();
    let _ = writeln!(out, "Chargestrip {} on {} {}", crate::update::CURRENT, std::env::consts::OS, std::env::consts::ARCH);
    if let Some(api) = api.as_mut() {
        let _ = api.refresh_devices();
        let mut vendor = Vec::new();
        let mut other = std::collections::BTreeSet::new();
        for d in api.device_list() {
            let product = d.product_string().unwrap_or_default();
            if d.usage_page() >= 0xFF00 {
                vendor.push(format!(
                    "  {:04x}:{:04x} interface={} usage={:04x}:{:04x} \"{}\"",
                    d.vendor_id(), d.product_id(), d.interface_number(), d.usage_page(), d.usage(), product
                ));
            } else {
                other.insert(format!("  {:04x}:{:04x} \"{}\"", d.vendor_id(), d.product_id(), product));
            }
        }
        let _ = writeln!(out, "
Vendor HID interfaces:
{}", vendor.join("
"));
        let _ = writeln!(out, "
Other HID devices:
{}", other.into_iter().collect::<Vec<_>>().join("
"));
    } else {
        let _ = writeln!(out, "
HID unavailable");
    }
    let _ = writeln!(out, "
Readings:");
    for r in poll(api) {
        let state = match r.battery {
            Some(b) => format!("{}%{}", b.percent, if b.charging { " charging" } else { "" }),
            None => "no reply (off or asleep)".into(),
        };
        let _ = writeln!(out, "  {} {:?} \"{}\": {state}", r.key, r.kind, r.name);
    }
    out
}

/// USB product name without the receiver/cable suffixes vendors add, e.g.
/// "Razer DeathAdder V3 Pro (Wireless)" → "Razer DeathAdder V3 Pro".
fn product_name(name: &str) -> String {
    let mut name = name.trim();
    for suffix in [" HyperSpeed Dongle", " Dongle", " Wireless Receiver", " Receiver", " (Wireless)", " (Wired)"] {
        name = name.strip_suffix(suffix).unwrap_or(name);
    }
    name.to_string()
}

/// Sends the request and reads reports until `accept` returns a value. Unsolicited reports the
/// device sends in between are skipped by `accept` returning `None`.
/// `None` on timeout (`TIMEOUT`) or HID error.
pub(crate) fn exchange<T>(dev: &HidDevice, request: &[u8], accept: impl FnMut(&[u8]) -> Option<T>) -> Option<T> {
    exchange_within(dev, request, TIMEOUT, accept)
}

/// [`exchange`] with a custom timeout.
pub(crate) fn exchange_within<T>(
    dev: &HidDevice,
    request: &[u8],
    timeout: Duration,
    mut accept: impl FnMut(&[u8]) -> Option<T>,
) -> Option<T> {
    dev.write(request).ok()?;
    let deadline = Instant::now() + timeout;
    let mut buf = [0u8; 128];
    loop {
        let left = deadline.checked_duration_since(Instant::now())?;
        let n = dev.read_timeout(&mut buf, left.as_millis().max(1) as i32).ok()?;
        if n == 0 {
            return None;
        }
        if let Some(value) = accept(&buf[..n]) {
            return Some(value);
        }
    }
}

/// Try with real hardware: `cargo test probe -- --ignored --nocapture`
/// Prints the same report as "Copy diagnostics"; also the way to find a new device's
/// VID/PID/usage page.
#[cfg(test)]
mod probe {
    use super::*;

    #[test]
    #[ignore]
    fn probe() {
        let mut api = HidApi::new().ok();
        println!("{}", diagnostics(api.as_mut()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_name_drops_receiver_suffixes() {
        assert_eq!(product_name("Razer DeathAdder V3 Pro (Wireless)"), "Razer DeathAdder V3 Pro");
        assert_eq!(product_name("Razer Viper V2 Pro HyperSpeed Dongle"), "Razer Viper V2 Pro");
        assert_eq!(product_name(" Arctis Nova 7P "), "Arctis Nova 7P");
    }
}
