//! Background polling loop. Runs on its own thread so HID reads (up to 1 s each) never block the
//! UI: every `INTERVAL`, or right away when a refresh is requested.

use crate::devices::{self, Reading};
use crate::model::{present, Shown};
use hidapi::HidApi;
use std::collections::HashMap;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::Duration;

const INTERVAL: Duration = Duration::from_secs(60);

/// Starts the loop; each round's result goes to `notify` (devices whose receiver is plugged in,
/// in `DRIVERS` order, then connected Bluetooth devices). Sending `()` to the returned sender
/// polls again right away; dropping the sender ends the loop.
pub fn spawn(notify: impl Fn(Vec<Shown>) + Send + 'static) -> Sender<()> {
    let (tx, rx) = mpsc::channel::<()>();
    std::thread::Builder::new()
        .name("poller".into())
        .stack_size(256 * 1024) // the default 1 MB isn't needed; HID buffers are small
        .spawn(move || {
            let mut api: Option<HidApi> = None;
            let mut last = HashMap::new();
            loop {
                if api.is_none() {
                    api = HidApi::new().ok();
                }
                let readings = devices::poll(api.as_mut());
                notify(snapshot(&readings, &mut last));
                match rx.recv_timeout(INTERVAL) {
                    Ok(()) => while rx.try_recv().is_ok() {}, // coalesce back-to-back requests
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        })
        .expect("failed to start the polling thread");
    tx
}

/// Turns readings into what to show; successful readings are stored in `last` by device key so
/// the last value can be shown while the device sleeps.
fn snapshot(readings: &[Reading], last: &mut HashMap<String, u8>) -> Vec<Shown> {
    readings
        .iter()
        .map(|r| {
            if let Some(b) = r.battery {
                last.insert(r.key.clone(), b.percent);
            }
            present(r.kind, &r.name, r.battery, last.get(&r.key).copied())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Battery, Kind, Tone};

    fn r(key: &str, percent: Option<u8>) -> Reading {
        let battery = percent.map(|percent| Battery { percent, charging: false });
        Reading { key: key.into(), name: key.into(), kind: Kind::Mouse, battery }
    }

    #[test]
    fn keeps_last_value_while_asleep() {
        let mut last = HashMap::new();
        snapshot(&[r("usb:0", Some(70)), r("usb:1", Some(40))], &mut last);
        let s = snapshot(&[r("usb:0", Some(69)), r("usb:1", None)], &mut last);
        assert_eq!((s[0].text.as_str(), s[0].tone), ("69%", Tone::Normal));
        assert_eq!((s[1].text.as_str(), s[1].tone), ("40%", Tone::Inactive));
    }

    #[test]
    fn last_value_follows_the_device_not_the_position() {
        let mut last = HashMap::new();
        snapshot(&[r("usb:0", Some(70)), r("bt:a", Some(30))], &mut last);
        // usb:0 was unplugged; bt:a is now first but keeps its own value.
        let s = snapshot(&[r("bt:a", None)], &mut last);
        assert_eq!(s[0].text, "30%");
    }
}
