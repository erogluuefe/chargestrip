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
