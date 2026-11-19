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
}

impl From<Battery> for Found {
    fn from(battery: Battery) -> Self {
        Found { battery, kind: None }
    }
}

/// One device's reading in one polling round.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reading {
    /// Stable key that identifies the device across rounds (to remember its last value).
    pub key: String,
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
            let found = std::iter::once(first).chain(candidates).find_map(|info| {
                let dev = info.open_device(api).ok()?;
                (d.read)(&dev, info.product_id())
            });
            Some(Reading {
                key: format!("usb:{i}"),
                kind: found.as_ref().and_then(|f| f.kind).unwrap_or(d.kind),
                battery: found.map(|f| f.battery),
            })
        })
        .collect()
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
/// Prints every vendor-specific HID interface (usage page >= 0xFF00) and every reading (Bluetooth
/// included); also the way to find a new device's VID/PID/usage page.
#[cfg(test)]
mod probe {
    use super::*;

    #[test]
    #[ignore]
    fn probe() {
        let mut api = HidApi::new().expect("hidapi");
        for d in api.device_list().filter(|d| d.usage_page() >= 0xFF00) {
            println!(
                "{:04x}:{:04x} interface={} usage_page={:04x} usage={:04x} {}",
                d.vendor_id(), d.product_id(), d.interface_number(), d.usage_page(), d.usage(),
                d.product_string().unwrap_or_default()
            );
        }
        for r in poll(Some(&mut api)) {
            println!("{} {:?}: {:?}", r.key, r.kind, r.battery);
        }
    }
}
