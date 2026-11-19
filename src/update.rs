//! Updates: checks GitHub for the latest release once a day and tells the UI when it is newer.
//! Downloading and installing only happen when the user picks "Update" in the menu.
//!
//! The latest version is read from the tag GitHub's `releases/latest` URL redirects to
//! (`…/releases/tag/v1.2.3`), which is not subject to the GitHub API rate limit. The file name is
//! derived from the version (the release workflow uploads the same names every time).
//!
//! No networking dependency (TLS, HTTP): the `curl` that ships with Windows 10+ and macOS is run
//! instead, so size and memory stay the same.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const RELEASES: &str = "https://github.com/erogluuefe/chargestrip/releases";
const CHECK_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// This platform's release file (installer / disk image): prefix + version + extension.
#[cfg(windows)]
const ASSET: (&str, &str) = ("Chargestrip-Setup-", ".exe");
#[cfg(target_os = "macos")]
const ASSET: (&str, &str) = ("Chargestrip-macOS-", ".dmg");

/// A published version newer than the running one.
#[derive(Debug, PartialEq, Eq)]
pub struct Update {
    pub version: String,
    url: String,
}

/// The running version.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Checks in the background once a day; calls `notify` once when a new version is found, then stops.
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

/// Checks right away ("Check for updates" in the menu; usually well under a second).
/// `Ok(None)`: up to date. `Err`: GitHub could not be reached.
pub fn check() -> Result<Option<Update>, ()> {
    // Headers only (-I), without following the redirect: "location: …/releases/tag/v1.2.3".
    let headers = curl(&["-fsSI", "--max-time", "20", &format!("{RELEASES}/latest")]).ok_or(())?;
    Ok(parse(&headers, CURRENT))
}

/// Deletes installers left in the temp folder by earlier updates. A still-running installer keeps
/// its file locked; that one is deleted on the next check.
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

/// Downloads the installer into the temp folder.
pub fn download(update: &Update) -> Option<PathBuf> {
    let name = update.url.rsplit('/').next()?;
    let path = std::env::temp_dir().join(name);
    curl(&["-fsSL", "--max-time", "300", "-o", path.to_str()?, &update.url])?;
    path.exists().then_some(path)
}

/// Starts the downloaded installer; the caller must exit right after (so no file stays locked).
/// The app is relaunched when the installation finishes.
pub fn install(file: &Path) -> bool {
    #[cfg(windows)]
    {
        // Inno Setup: silent install, previous choices (start with Windows) are kept; the
        // `skipifnotsilent` entry in [Run] relaunches the app.
        Command::new(file).args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART"]).spawn().is_ok()
    }
    #[cfg(target_os = "macos")]
    {
        // Replaces the running .app bundle with the one from the disk image and opens it, after
        // waiting a second for this process to exit. curl downloads carry no quarantine flag, so
        // no Gatekeeper prompt appears.
        let Some(app) = std::env::current_exe().ok().and_then(|e| e.ancestors().nth(3).map(Path::to_path_buf))
        else {
            return false;
        };
        if app.extension().is_none_or(|x| x != "app") {
            return false; // running outside a bundle (development)
        }
        const SCRIPT: &str = r#"set -e; sleep 1
M=$(mktemp -d); hdiutil attach -nobrowse -quiet -mountpoint "$M" "$1"
rm -rf "$2.new"; ditto "$M/Chargestrip.app" "$2.new"; hdiutil detach -quiet "$M"
rm -rf "$2"; mv "$2.new" "$2"; open "$2""#;
        Command::new("/bin/sh").args(["-c", SCRIPT, "sh"]).arg(file).arg(app).spawn().is_ok()
    }
}

/// Runs `curl` without opening a window; its standard output on success.
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

/// From the `releases/latest` response headers: the version and this platform's file, if newer
/// than `current`.
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

/// Numeric comparison, so "0.10.1" > "0.9.3"; a non-numeric version is never considered newer.
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

    const HEADERS: &str = "HTTP/1.1 302 Found\r\nServer: GitHub.com\r\n\
        Location: https://github.com/erogluuefe/chargestrip/releases/tag/v0.3.0\r\n\r\n";

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
        // Without any release, GitHub redirects to the releases page.
        assert_eq!(parse("location: https://github.com/erogluuefe/chargestrip/releases\r\n", "0.1.0"), None);
        assert_eq!(parse("location: https://github.com/x/y/releases/tag/vNEXT\r\n", "0.1.0"), None);
    }
}
