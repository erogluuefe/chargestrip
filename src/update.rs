//! Güncelleme: GitHub'daki en son sürümü günde bir kez sorar; daha yeniyse arayüze bildirir.
//! İndirme ve kurma yalnızca kullanıcı menüden "Güncelle"yi seçince yapılır.
//!
//! Son sürüm, GitHub'ın `releases/latest` adresinin yönlendirdiği etiketten okunur
//! (`…/releases/tag/v1.2.3`); GitHub API'sinin saatlik istek sınırına takılmaz. Dosya adı
//! sürümden türetilir (sürüm akışı her sürümde aynı adlarla yükler).
//!
//! Ağ için ek kütüphane (TLS, HTTP) yok: Windows 10+ ve macOS'ta hazır gelen `curl` çalıştırılır.
//! Böylece boyut ve bellek değişmez.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const RELEASES: &str = "https://github.com/erogluuefe/chargestrip/releases";
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// Bu platformun Release dosyası (kurulum programı / disk kalıbı): ön ek + sürüm + uzantı.
#[cfg(windows)]
const ASSET: (&str, &str) = ("Chargestrip-Setup-", ".exe");
#[cfg(target_os = "macos")]
const ASSET: (&str, &str) = ("Chargestrip-macOS-", ".dmg");

/// Yayımlanmış, çalışandan yeni bir sürüm.
#[derive(Debug, PartialEq, Eq)]
pub struct Update {
    pub version: String,
    url: String,
}

/// Çalışan sürüm.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Arka planda günde bir kez denetler; yeni sürüm bulunca `notify` bir kez çağrılır ve biter.
pub fn spawn(notify: impl Fn(Update) + Send + 'static) {
    let _ = std::thread::Builder::new()
        .name("update".into())
        .stack_size(128 * 1024)
        .spawn(move || loop {
            remove_old_downloads();
            if let Ok(Some(update)) = check() {
                notify(update);
                return;
            }
            std::thread::sleep(CHECK_EVERY);
        });
}

/// Hemen denetler (menüden "Güncellemeleri denetle"; genelde bir saniyenin altında).
/// `Ok(None)`: güncel. `Err`: GitHub'a ulaşılamadı.
pub fn check() -> Result<Option<Update>, ()> {
    // Yalnızca başlıklar (-I), yönlendirme izlenmeden: "location: …/releases/tag/v1.2.3".
    let headers = curl(&["-fsSI", "--max-time", "20", &format!("{RELEASES}/latest")]).ok_or(())?;
    Ok(parse(&headers, CURRENT))
}

/// Önceki güncellemelerden geçici klasörde kalan kurulum dosyalarını siler. Kurulum hâlâ
/// çalışıyorsa dosyası kilitlidir, silinemez; bir sonraki denetimde silinir.
fn remove_old_downloads() {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(ASSET.0) && name.ends_with(ASSET.1) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
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

/// `releases/latest` cevabının başlıklarından, `current`'tan yeniyse sürüm ve bu platformun dosyası.
fn parse(headers: &str, current: &str) -> Option<Update> {
    let location = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        name.trim().eq_ignore_ascii_case("location").then(|| value.trim())
    })?;
    let version = location.rsplit_once("/tag/v")?.1.trim_end_matches('/');
    if !newer(version, current) {
        return None;
    }
    let (prefix, ext) = ASSET;
    let url = format!("{RELEASES}/download/v{version}/{prefix}{version}{ext}");
    Some(Update { version: version.to_string(), url })
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

    const HEADERS: &str = "HTTP/1.1 302 Found
Server: GitHub.com
        Location: https://github.com/erogluuefe/chargestrip/releases/tag/v0.3.0

";

    #[test]
    fn finds_newer_release_for_this_platform() {
        let u = parse(HEADERS, "0.2.4").unwrap();
        assert_eq!(u.version, "0.3.0");
        let (prefix, ext) = ASSET;
        assert_eq!(u.url, format!("{RELEASES}/download/v0.3.0/{prefix}0.3.0{ext}"));
    }

    #[test]
    fn ignores_same_or_older_release() {
        assert_eq!(parse(HEADERS, "0.3.0"), None);
        assert_eq!(parse(HEADERS, "0.10.0"), None);
    }

    #[test]
    fn version_order_is_numeric() {
        assert!(newer("0.10.0", "0.9.9"));
        assert!(newer("1.0", "0.99.99"));
        assert!(!newer("0.2.4", "0.2.4"));
        assert!(!newer("0.3.0-beta", "0.2.0"));
    }

    #[test]
    fn no_release_or_garbage() {
        assert_eq!(parse("", "0.1.0"), None);
        // Hiç sürüm yoksa GitHub sürümler sayfasına yönlendirir.
        assert_eq!(parse("location: https://github.com/erogluuefe/chargestrip/releases
", "0.1.0"), None);
        assert_eq!(parse("location: https://github.com/x/y/releases/tag/vNEXT
", "0.1.0"), None);
    }
}
