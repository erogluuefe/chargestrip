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
mod compx;
mod fractal_scape;
mod logitech;
mod razer;
mod sonix;
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
    /// Supported product IDs (receiver/cable variants speaking the same protocol). Empty: every
    /// product of `vid` is tried; one that never answers is not shown (see [`Reading::tentative`]).
    pub pids: &'static [u16],
    /// Which HID interface(s) of the receiver to try; the first valid reading wins.
    pub interface: Interface,
    /// Reads the battery from the opened interface. `pid` tells variants of one driver apart.
    /// `None`: device off or asleep, or no valid reply.
    pub read: fn(dev: &HidDevice, pid: u16) -> Option<Found>,
}

/// Which HID interfaces of a receiver a driver talks to.
#[derive(Clone, Copy)]
pub enum Interface {
    /// Every interface.
    Any,
    /// Every interface on this usage page.
    Page(u16),
    /// The interface with this usage page and usage.
    Usage(u16, u16),
    /// Every vendor-specific interface (usage page ≥ 0xFF00), this usage page and usage first.
    Vendor(u16, u16),
}

impl Interface {
    fn matches(self, page: u16, usage: u16) -> bool {
        match self {
            Interface::Any => true,
            Interface::Page(p) => page == p,
            Interface::Usage(p, u) => page == p && usage == u,
            Interface::Vendor(..) => page >= 0xFF00,
        }
    }

    /// Sort key: the preferred interface first.
    fn rank(self, page: u16, usage: u16) -> bool {
        !matches!(self, Interface::Vendor(p, u) if (p, u) == (page, usage))
    }
}

/// A driver's answer: the device is one of its own.
pub struct Found {
    /// `None`: the receiver answered, but the device is off, asleep or out of range.
    pub battery: Option<Battery>,
    /// Kind learned from the device (e.g. when a receiver can pair with a keyboard or a mouse).
    pub kind: Option<Kind>,
    /// Name learned from the device.
    pub name: Option<String>,
}

impl From<Battery> for Found {
    fn from(battery: Battery) -> Self {
        Found { battery: Some(battery), kind: None, name: None }
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
    /// The receiver was only tried (its driver covers every product of a vendor) and gave no valid
    /// reply, so it may not be a supported device at all. Shown only if it answered before.
    pub tentative: bool,
}

/// Supported USB receiver devices; shown in this order, before Bluetooth devices.
pub const DRIVERS: &[Driver] = &[
    arctis_nova7::DRIVER,
    fractal_scape::DRIVER,
    waizowl_ogm::DRIVER,
    razer::MICE,
    razer::KEYBOARDS,
    logitech::RECEIVERS,
    compx::ATK,
    compx::COMPX,
    compx::PULSAR,
    compx::HITSCAN,
    compx::G_WOLVES,
    sonix::AJAZZ_PRO,
    sonix::AJAZZ_MAX,
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
            let mut candidates: Vec<_> = api
                .device_list()
                .filter(|info| {
                    info.vendor_id() == d.vid
                        && (d.pids.is_empty() || d.pids.contains(&info.product_id()))
                        && d.interface.matches(info.usage_page(), info.usage())
                })
                .collect();
            candidates.sort_by_key(|info| d.interface.rank(info.usage_page(), info.usage()));
            let first = candidates.first()?; // receiver not plugged in
            let product = first.product_string().map(product_name).unwrap_or_default();
            let found = candidates.iter().find_map(|info| {
                let dev = info.open_device(api).ok()?;
                (d.read)(&dev, info.product_id())
            });
            let tentative = d.pids.is_empty() && found.is_none();
            let (kind, name, battery) = match found {
                Some(f) => (f.kind.unwrap_or(d.kind), f.name, f.battery),
                None => (d.kind, None, None),
            };
            let name = name.unwrap_or_else(|| if d.name.is_empty() { product } else { d.name.to_string() });
            Some(Reading { key: format!("usb:{i}"), name, kind, battery, tentative })
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
        let tried = if r.tentative { " [tried, not recognized]" } else { "" };
        let _ = writeln!(out, "  {} {:?} \"{}\": {state}{tried}", r.key, r.kind, r.name);
    }
    out
}

/// USB product name without the receiver/cable suffixes vendors add, e.g.
/// "Razer DeathAdder V3 Pro (Wireless)" → "Razer DeathAdder V3 Pro".
fn product_name(name: &str) -> String {
    let mut name = name.trim();
    for suffix in [" HyperSpeed Dongle", " Dongle", " Wireless Receiver", " Receiver", " (Wireless)", " (Wired)", " 2.4G"] {
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
