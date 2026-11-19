//! macOS: Bluetooth pil seviyesi, `system_profiler SPBluetoothDataType -json` çıktısından
//! (Bluetooth menüsünde görünen değerler; Magic Mouse/Keyboard, AirPods ve diğer kulaklıklar).
//!
//! Araç yaklaşık bir saniye sürdüğünden en çok `EVERY` aralıkla çalıştırılır; arada son sonuç
//! kullanılır. JSON'dan yalnızca birkaç düz alan gerektiği için ayrıştırıcı kütüphanesi yok.

use crate::devices::Reading;
use crate::model::{Battery, Kind};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const EVERY: Duration = Duration::from_secs(5 * 60);
static CACHE: Mutex<Option<(Instant, Vec<Reading>)>> = Mutex::new(None);

pub fn poll() -> Vec<Reading> {
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, readings)) = &*cache {
        if at.elapsed() < EVERY {
            return readings.clone();
        }
    }
    let readings = Command::new("/usr/sbin/system_profiler")
        .args(["SPBluetoothDataType", "-json"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| parse(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default();
    *cache = Some((Instant::now(), readings.clone()));
    readings
}

/// Çıktının biçimi (yalnızca bağlı cihazlar kullanılır):
/// ```text
/// "device_connected" : [ { "AirPods Max" : { "device_batteryLevelMain" : "100%",
///                                             "device_minorType" : "Headphones", ... } }, ... ],
/// "device_not_connected" : [ ... ]
/// ```
/// Kulaklık tekleri sol/sağ ayrı bildirir; ikisinden düşük olanı gösterilir (kutu yok sayılır).
fn parse(json: &str) -> Vec<Reading> {
    let Some(start) = json.find("\"device_connected\"") else { return Vec::new() };
    let section = &json[start..];
    let section = &section[..section.find("\"device_not_connected\"").unwrap_or(section.len())];

    #[derive(Default)]
    struct Device {
        name: String,
        main: Option<u8>,
        buds: Option<u8>,
        kind: Option<Kind>,
    }
    let mut devices: Vec<Device> = Vec::new();
    for line in section.lines().skip(1).map(str::trim) {
        // `"Ad" : {` yeni bir cihazı başlatır; cihazın alanları düz `"anahtar" : "değer"` satırlarıdır.
        if let Some(name) = line.strip_suffix('{').and_then(|l| quoted(l.trim_end().strip_suffix(':')?.trim_end())) {
            devices.push(Device { name: name.to_string(), ..Default::default() });
            continue;
        }
        let (Some(dev), Some((key, value))) = (devices.last_mut(), field(line)) else { continue };
        match key {
            "device_batteryLevelMain" => dev.main = percent(value),
            "device_batteryLevelLeft" | "device_batteryLevelRight" => {
                if let Some(p) = percent(value) {
                    dev.buds = Some(dev.buds.map_or(p, |b| b.min(p)));
                }
            }
            "device_minorType" => dev.kind = Some(kind(value)),
            _ => {}
        }
    }
    devices
        .into_iter()
        .filter_map(|d| {
            let percent = d.main.or(d.buds)?;
            Some(Reading {
                key: format!("bt:{}", d.name),
                kind: d.kind.unwrap_or(Kind::Other),
                battery: Some(Battery { percent, charging: false }),
            })
        })
        .collect()
}

/// `"x"` → `x`
fn quoted(s: &str) -> Option<&str> {
    s.strip_prefix('"')?.strip_suffix('"')
}

/// `"anahtar" : "değer",` → (anahtar, değer)
fn field(line: &str) -> Option<(&str, &str)> {
    let (k, v) = line.trim_end_matches(',').split_once(" : ")?;
    Some((quoted(k)?, quoted(v)?))
}

/// "85%" → 85
fn percent(v: &str) -> Option<u8> {
    v.strip_suffix('%')?.trim().parse().ok().filter(|&p| p <= 100)
}

fn kind(minor_type: &str) -> Kind {
    match minor_type {
        t if t.contains("Head") => Kind::Headset, // Headphones, Headset
        "Mouse" | "Trackpad" => Kind::Mouse,
        "Keyboard" => Kind::Keyboard,
        "Gamepad" | "Joystick" => Kind::Gamepad,
        _ => Kind::Other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
  "SPBluetoothDataType" : [
    {
      "controller_properties" : {
        "controller_address" : "AA:BB:CC:DD:EE:FF",
        "controller_state" : "attrib_on"
      },
      "device_connected" : [
        {
          "AirPods Max" : {
            "device_address" : "11:22:33:44:55:66",
            "device_batteryLevelMain" : "100%",
            "device_minorType" : "Headphones"
          }
        },
        {
          "AirPods Pro" : {
            "device_address" : "22:33:44:55:66:77",
            "device_batteryLevelCase" : "40%",
            "device_batteryLevelLeft" : "81%",
            "device_batteryLevelRight" : "76%",
            "device_minorType" : "Headphones"
          }
        },
        {
          "Magic Keyboard" : {
            "device_address" : "33:44:55:66:77:88",
            "device_batteryLevelMain" : "9%",
            "device_minorType" : "Keyboard"
          }
        },
        {
          "Speaker" : {
            "device_address" : "44:55:66:77:88:99",
            "device_minorType" : "Loudspeaker"
          }
        }
      ],
      "device_not_connected" : [
        {
          "Magic Mouse" : {
            "device_address" : "55:66:77:88:99:00",
            "device_batteryLevelMain" : "50%",
            "device_minorType" : "Mouse"
          }
        }
      ]
    }
  ]
}"#;

    fn reading(key: &str, kind: Kind, percent: u8) -> Reading {
        Reading { key: key.into(), kind, battery: Some(Battery { percent, charging: false }) }
    }

    #[test]
    fn connected_devices_with_battery() {
        assert_eq!(
            parse(SAMPLE),
            vec![
                reading("bt:AirPods Max", Kind::Headset, 100),
                reading("bt:AirPods Pro", Kind::Headset, 76),
                reading("bt:Magic Keyboard", Kind::Keyboard, 9),
            ]
        );
    }

    #[test]
    fn no_bluetooth_section() {
        assert_eq!(parse("{}"), vec![]);
        assert_eq!(parse(r#"{"SPBluetoothDataType":[{"device_not_connected":[]}]}"#), vec![]);
    }
}
