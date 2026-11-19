//! Güncelleme: GitHub'daki en son sürümü günde bir kez sorar; daha yeniyse arayüze bildirir.
//! İndirme ve kurma yalnızca kullanıcı menüden "Güncelle"yi seçince yapılır.
//!
//! Ağ için ek kütüphane (TLS, HTTP, JSON) yok: Windows 10+ ve macOS'ta hazır gelen `curl`
//! çalıştırılır, cevaptan gereken iki alan elle okunur. Böylece boyut ve bellek değişmez.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const LATEST: &str = "https://api.github.com/repos/erogluuefe/chargestrip/releases/latest";
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// Bu platformun Release dosyası (kurulum programı / disk kalıbı).
#[cfg(windows)]
const ASSET_SUFFIX: &str = ".exe";
#[cfg(target_os = "macos")]
const ASSET_SUFFIX: &str = ".dmg";

/// Yayımlanmış, çalışandan yeni bir sürüm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Update {
    pub version: String,
    url: String,
}

/// Arka planda günde bir kez denetler; yeni sürüm bulunca `notify` bir kez çağrılır ve biter.
pub fn spawn(notify: impl Fn(Update) + Send + 'static) {
    let _ = std::thread::Builder::new()
        .name("update".into())
        .stack_size(128 * 1024)
        .spawn(move || loop {
            let json = curl(&["-fsSL", "--max-time", "20", LATEST]).unwrap_or_default();
            if let Some(update) = parse(&json, env!("CARGO_PKG_VERSION")) {
                notify(update);
                return;
            }
            std::thread::sleep(CHECK_EVERY);
        });
}

/// Kurulum dosyasını geçici klasöre indirir.
pub fn download(update: &Update) -> Option<PathBuf> {
    let name = update.url.rsplit('/').next()?;
    let path = std::env::temp_dir().join(name);
    curl(&["-fsSL", "--max-time", "300", "-o", path.to_str()?, &update.url])?;
    path.exists().then_some(path)
}

/// İndirilen kurulumu başlatır; çağıran ardından hemen çıkmalıdır (dosya kilitli kalmasın).
/// Kurulum bitince uygulama kendiliğinden yeniden açılır.
pub fn install(file: &Path) -> bool {
    #[cfg(windows)]
    {
        // Inno Setup: sessiz kurulum, önceki seçimler (açılışta başlat) korunur; [Run]'daki
        // `skipifnotsilent` satırı uygulamayı yeniden açar.
        Command::new(file).args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]).spawn().is_ok()
    }
    #[cfg(target_os = "macos")]
    {
        // Çalışan .app paketinin yerine diskteki yenisini koyar ve açar. Bu süreç çıkana dek
        // 1 sn bekler. curl ile indirilen dosyada karantina işareti olmaz, uyarı çıkmaz.
        let Some(app) = std::env::current_exe().ok().and_then(|e| e.ancestors().nth(3).map(Path::to_path_buf))
        else {
            return false;
        };
        if app.extension().is_none_or(|x| x != "app") {
            return false; // paket dışında (geliştirme) çalışıyor
        }
        const SCRIPT: &str = r#"set -e; sleep 1
M=$(mktemp -d); hdiutil attach -nobrowse -quiet -mountpoint "$M" "$1"
rm -rf "$2.new"; ditto "$M/Chargestrip.app" "$2.new"; hdiutil detach -quiet "$M"
rm -rf "$2"; mv "$2.new" "$2"; open "$2""#;
        Command::new("/bin/sh").args(["-c", SCRIPT, "sh"]).arg(file).arg(app).spawn().is_ok()
    }
}

/// `curl`'ü pencere açmadan çalıştırır; başarılıysa standart çıktısı.
fn curl(args: &[&str]) -> Option<String> {
    #[cfg(windows)]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let mut c = Command::new(format!(r"{root}\System32\curl.exe"));
        c.creation_flags(CREATE_NO_WINDOW);
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = Command::new("/usr/bin/curl");
    let out = cmd.args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// GitHub "latest release" cevabından, `current`'tan yeniyse sürüm ve bu platformun dosyası.
fn parse(json: &str, current: &str) -> Option<Update> {
    let version = string_values(json, "tag_name").next()?.trim_start_matches('v').to_string();
    if !newer(&version, current) {
        return None;
    }
    let url = string_values(json, "browser_download_url").find(|u| u.ends_with(ASSET_SUFFIX))?;
    Some(Update { version, url: url.to_string() })
}

/// JSON'da `"key": "değer"` biçimindeki tüm değerler (kaçış karakteri içermeyenler yeterli:
/// sürüm etiketleri ve dosya adresleri).
fn string_values<'a>(json: &'a str, key: &str) -> impl Iterator<Item = &'a str> {
    let needle = format!("\"{key}\"");
    let values: Vec<&'a str> = json
        .match_indices(&needle)
        .filter_map(|(i, _)| {
            let rest = json[i + needle.len()..].trim_start().strip_prefix(':')?.trim_start();
            let rest = rest.strip_prefix('"')?;
            Some(&rest[..rest.find('"')?])
        })
        .collect();
    values.into_iter()
}

/// "0.10.1" > "0.9.3" gibi sayısal karşılaştırma; sayı olmayan sürüm hiçbir zaman yeni sayılmaz.
fn newer(candidate: &str, current: &str) -> bool {
    let parts = |v: &str| v.split('.').map(str::parse::<u32>).collect::<Result<Vec<_>, _>>();
    match (parts(candidate), parts(current)) {
        (Ok(a), Ok(b)) => a > b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JSON: &str = r#"{
      "tag_name": "v0.3.0",
      "assets": [
        {"name": "Chargestrip-macOS-0.3.0.dmg",
         "browser_download_url": "https://github.com/erogluuefe/chargestrip/releases/download/v0.3.0/Chargestrip-macOS-0.3.0.dmg"},
        {"name": "Chargestrip-Setup-0.3.0.exe",
         "browser_download_url":"https://github.com/erogluuefe/chargestrip/releases/download/v0.3.0/Chargestrip-Setup-0.3.0.exe"}
      ]
    }"#;

    #[test]
    fn finds_newer_release_for_this_platform() {
        let u = parse(JSON, "0.2.4").unwrap();
        assert_eq!(u.version, "0.3.0");
        assert!(u.url.ends_with(ASSET_SUFFIX));
        assert!(u.url.starts_with("https://github.com/erogluuefe/chargestrip/releases/download/v0.3.0/"));
    }

    #[test]
    fn ignores_same_or_older_release() {
        assert_eq!(parse(JSON, "0.3.0"), None);
        assert_eq!(parse(JSON, "0.10.0"), None);
    }

    #[test]
    fn version_order_is_numeric() {
        assert!(newer("0.10.0", "0.9.9"));
        assert!(newer("1.0", "0.99.99"));
        assert!(!newer("0.2.4", "0.2.4"));
        assert!(!newer("0.3.0-beta", "0.2.0"));
    }

    #[test]
    fn missing_fields_or_garbage() {
        assert_eq!(parse("", "0.1.0"), None);
        assert_eq!(parse(r#"{"message":"Not Found"}"#, "0.1.0"), None);
        assert_eq!(parse(r#"{"tag_name":"v9.0.0","assets":[]}"#, "0.1.0"), None);
    }
}
