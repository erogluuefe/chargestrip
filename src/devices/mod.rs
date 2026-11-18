pub mod nova7;
pub mod ogm;

use crate::model::Reading;
use hidapi::HidApi;

/// Kulaklık ve fareyi sorgular. Takılıp çıkarılan alıcılar için her seferinde listeyi yeniler.
pub fn poll_all(api: &mut HidApi) -> (Reading, Reading) {
    if api.refresh_devices().is_err() {
        return (Reading::Unavailable, Reading::Unavailable);
    }
    (nova7::query(api), ogm::query(api))
}

/// Gerçek donanım denemesi: `cargo test probe -- --ignored --nocapture`
#[cfg(test)]
mod probe {
    use super::*;

    #[test]
    #[ignore]
    fn probe() {
        let mut api = HidApi::new().expect("hidapi");
        for d in api.device_list() {
            if [nova7::VID, ogm::VID].contains(&d.vendor_id()) {
                println!(
                    "{:04x}:{:04x} if={} page={:04x} usage={:04x}",
                    d.vendor_id(), d.product_id(), d.interface_number(), d.usage_page(), d.usage()
                );
            }
        }
        let (h, m) = poll_all(&mut api);
        println!("kulaklık: {h:?}\nfare: {m:?}");
    }
}
