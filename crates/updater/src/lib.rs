//! Self-update from GitHub Releases, following KlikCore2's `launcher/update.rs`.
//!
//! The app asks GitHub for the latest release of its repository. If that version is
//! newer than this build and has the AppImage asset, the app offers to install it: the new
//! file is downloaded next to the running AppImage, its SHA-256 is checked against the
//! `.sha256` asset, the running file is renamed out of the way and the new one takes its
//! place. The old copy is deleted on the next start ([`cleanup`]).
//!
//! In a Flatpak the release's `.flatpak` bundle is downloaded instead (checked the same
//! way) and installed over this one with `flatpak install --reinstall --bundle` on the
//! host; the app's data in `~/.var/app` stays.
//!
//! Only AppImage and Flatpak builds update (`$APPIMAGE` is set by the AppImage runtime,
//! `/.flatpak-info` exists in a Flatpak sandbox). Builds run from a Cargo `target`
//! folder never do. Pre-releases (tags with a `-`, such as `v0.3.0-test1`) are never
//! returned by GitHub's `releases/latest`, so they are ignored.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncWriteExt;

/// Release asset the AppImage updates from.
pub const ASSET_NAME: &str = "Romulus-x86_64.AppImage";
/// Release asset the Flatpak updates from.
pub const FLATPAK_ASSET: &str = "Romulus-x86_64.flatpak";
/// Smallest plausible AppImage; anything smaller is an error page or a cut-off download.
const MIN_SIZE: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("unexpected response from GitHub: {0}")]
    Decode(String),
    #[error("this copy can't update itself ({0})")]
    NotUpdatable(&'static str),
    #[error("the download looks incomplete ({0} bytes)")]
    TooSmall(u64),
    #[error("the download is damaged (checksum mismatch)")]
    Checksum,
    #[error("flatpak couldn't install the update: {0}")]
    Flatpak(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub sha256_url: Option<String>,
    pub notes: String,
}

#[derive(Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// Where releases come from. `api_base` is `https://api.github.com` except in tests.
#[derive(Debug, Clone)]
pub struct Source {
    pub api_base: String,
    /// GitHub `owner/name`.
    pub repo: String,
}

impl Source {
    pub fn github(repo: &str) -> Self {
        Self {
            api_base: "https://api.github.com".into(),
            repo: repo.into(),
        }
    }
}

/// The file to replace: the AppImage itself when running from one (the program then runs
/// from a temporary mount), otherwise this executable.
pub fn program_path() -> std::io::Result<PathBuf> {
    match std::env::var_os("APPIMAGE") {
        Some(p) if !p.is_empty() => Ok(PathBuf::from(p)),
        _ => std::env::current_exe(),
    }
}

pub fn is_appimage() -> bool {
    std::env::var_os("APPIMAGE").is_some_and(|p| !p.is_empty())
}

pub fn is_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// The release asset this copy updates from.
pub fn asset_name() -> &'static str {
    if is_flatpak() {
        FLATPAK_ASSET
    } else {
        ASSET_NAME
    }
}

/// A developer build (run from Cargo's `target` folder) is never replaced.
pub fn is_dev_build() -> bool {
    program_path()
        .map(|p| p.components().any(|c| c.as_os_str() == "target"))
        .unwrap_or(true)
}

/// Can this copy update itself? `Err` says why not.
pub fn updatable() -> std::result::Result<(), &'static str> {
    if is_flatpak() {
        Ok(())
    } else if is_dev_build() {
        Err("development build")
    } else if !is_appimage() {
        Err("not running as an AppImage or Flatpak")
    } else {
        Ok(())
    }
}

/// Is version `a` newer than `b`? Numeric `major.minor.patch`; a leading "v" is ignored.
pub fn newer(a: &str, b: &str) -> bool {
    let parts = |s: &str| -> Vec<u64> {
        s.trim_start_matches(['v', 'V'])
            .split(['.', '-'])
            .map(|p| p.parse().unwrap_or(0))
            .take(3)
            .collect()
    };
    parts(a) > parts(b)
}

async fn get_release(http: &reqwest::Client, url: &str) -> Result<GhRelease> {
    let resp = http
        .get(url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?;
    resp.json().await.map_err(|e| Error::Decode(e.to_string()))
}

/// The latest release, if it is newer than `current` and has this copy's asset.
pub async fn check(http: &reqwest::Client, src: &Source, current: &str) -> Result<Option<Release>> {
    check_for(http, src, current, asset_name()).await
}

/// [`check`] for a given release asset.
pub async fn check_for(
    http: &reqwest::Client,
    src: &Source,
    current: &str,
    asset: &str,
) -> Result<Option<Release>> {
    let r = get_release(
        http,
        &format!("{}/repos/{}/releases/latest", src.api_base, src.repo),
    )
    .await?;
    if r.tag_name.is_empty() || !newer(&r.tag_name, current) {
        return Ok(None);
    }
    let find = |name: &str| {
        r.assets
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.browser_download_url.clone())
    };
    let Some(url) = find(asset) else {
        return Ok(None);
    };
    Ok(Some(Release {
        version: r.tag_name.trim_start_matches(['v', 'V']).to_string(),
        url,
        sha256_url: find(&format!("{asset}.sha256")),
        notes: r.body.clone().unwrap_or_default(),
    }))
}

/// Notes for `version`, for a "what's new" message after an update.
pub async fn release_notes(http: &reqwest::Client, src: &Source, version: &str) -> Result<String> {
    let r = get_release(
        http,
        &format!(
            "{}/repos/{}/releases/tags/v{version}",
            src.api_base, src.repo
        ),
    )
    .await?;
    Ok(r.body.unwrap_or_default())
}

fn old_copy(exe: &Path) -> PathBuf {
    let name = exe
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Romulus".into());
    let ext = exe
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    exe.with_file_name(format!("{name}.old{ext}"))
}

/// Delete the copy left behind by the last update.
pub fn cleanup() {
    if let Ok(exe) = program_path() {
        let _ = std::fs::remove_file(old_copy(&exe));
        let _ = std::fs::remove_file(exe.with_extension("download"));
    }
}

/// Download and swap in the new AppImage at `target`. `progress(done, total)` is called
/// as bytes arrive. Returns the installed path; start it to finish updating.
pub async fn install_to(
    http: &reqwest::Client,
    r: &Release,
    target: &Path,
    progress: impl Fn(u64, Option<u64>),
) -> Result<PathBuf> {
    let expected = match &r.sha256_url {
        Some(u) => {
            let text = http.get(u).send().await?.error_for_status()?.text().await?;
            // `sha256sum` format: "<hex>  <file name>".
            let hex = text
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase();
            if hex.len() != 64 {
                return Err(Error::Decode("bad .sha256 file".into()));
            }
            Some(hex)
        }
        None => {
            tracing::warn!(
                "release {} has no .sha256 asset; installing without a checksum",
                r.version
            );
            None
        }
    };

    let tmp = target.with_extension("download");
    let resp = http.get(&r.url).send().await?.error_for_status()?;
    let total = resp.content_length();
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut hasher = Sha256::new();
    let mut done = 0u64;
    let mut stream = resp.bytes_stream();
    let result: Result<()> = async {
        while let Some(chunk) = stream.next().await {
            let chunk = chunk?;
            hasher.update(&chunk);
            file.write_all(&chunk).await?;
            done += chunk.len() as u64;
            progress(done, total);
        }
        file.flush().await?;
        file.sync_all().await?;
        if done < MIN_SIZE {
            return Err(Error::TooSmall(done));
        }
        if let Some(want) = &expected
            && hex::encode(hasher.finalize()) != *want
        {
            return Err(Error::Checksum);
        }
        Ok(())
    }
    .await;
    drop(file);
    if let Err(e) = result {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
    }
    let old = old_copy(target);
    let _ = std::fs::remove_file(&old);
    if target.exists() {
        std::fs::rename(target, &old)?;
    }
    if let Err(e) = std::fs::rename(&tmp, target) {
        let _ = std::fs::rename(&old, target);
        return Err(e.into());
    }
    tracing::info!("installed Romulus {} at {}", r.version, target.display());
    Ok(target.to_path_buf())
}

/// [`install_to`] the running AppImage, or download and install the Flatpak bundle.
pub async fn install(
    http: &reqwest::Client,
    r: &Release,
    progress: impl Fn(u64, Option<u64>),
) -> Result<PathBuf> {
    updatable().map_err(Error::NotUpdatable)?;
    if is_flatpak() {
        return install_flatpak(http, r, progress).await;
    }
    install_to(http, r, &program_path()?, progress).await
}

/// Was this Flatpak installed for the user (`~/.local/share/flatpak`), not system-wide?
fn flatpak_user_install(info: &str) -> bool {
    info.lines()
        .find_map(|l| l.strip_prefix("app-path="))
        .is_none_or(|p| p.contains("/.local/share/flatpak/"))
}

/// The host command that installs `bundle` over this Flatpak.
fn flatpak_install_args(info: &str, bundle: &Path) -> Vec<String> {
    let scope = if flatpak_user_install(info) {
        "--user"
    } else {
        "--system"
    };
    [
        "--host",
        "flatpak",
        "install",
        scope,
        "--noninteractive",
        "--reinstall",
        "--bundle",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([bundle.to_string_lossy().into_owned()])
    .collect()
}

async fn install_flatpak(
    http: &reqwest::Client,
    r: &Release,
    progress: impl Fn(u64, Option<u64>),
) -> Result<PathBuf> {
    // The cache folder is under ~/.var/app, the same path inside and outside the sandbox.
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&dir)?;
    let bundle = dir.join(FLATPAK_ASSET);
    install_to(http, r, &bundle, progress).await?;
    let info = std::fs::read_to_string("/.flatpak-info").unwrap_or_default();
    let args = flatpak_install_args(&info, &bundle);
    let out = tokio::process::Command::new("flatpak-spawn")
        .args(&args)
        .output()
        .await?;
    let _ = std::fs::remove_file(&bundle);
    let _ = std::fs::remove_file(old_copy(&bundle));
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let last = err
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("");
        return Err(Error::Flatpak(last.trim().to_string()));
    }
    tracing::info!("installed Romulus {} (Flatpak)", r.version);
    Ok(bundle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn version_order() {
        assert!(newer("v0.2.0", "0.1.0"));
        assert!(newer("0.10.0", "0.9.9"));
        assert!(!newer("v0.1.0", "0.1.0"));
        assert!(!newer("0.1.0", "0.1.1"));
    }

    async fn server(body: Vec<u8>, sha: Option<String>) -> (MockServer, Source) {
        let s = MockServer::start().await;
        let mut assets = vec![
            json!({"name": ASSET_NAME, "browser_download_url": format!("{}/dl/app", s.uri())}),
        ];
        if sha.is_some() {
            assets.push(json!({"name": format!("{ASSET_NAME}.sha256"), "browser_download_url": format!("{}/dl/sha", s.uri())}));
        }
        Mock::given(path("/repos/o/r/releases/latest"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"tag_name": "v0.2.0", "body": "notes", "assets": assets}),
                ),
            )
            .mount(&s)
            .await;
        Mock::given(path("/dl/app"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
            .mount(&s)
            .await;
        if let Some(sha) = sha {
            Mock::given(path("/dl/sha"))
                .respond_with(ResponseTemplate::new(200).set_body_string(sha))
                .mount(&s)
                .await;
        }
        let src = Source {
            api_base: s.uri(),
            repo: "o/r".into(),
        };
        (s, src)
    }

    #[test]
    fn flatpak_updates_install_where_the_app_is() {
        let user = "[Application]\nname=x\n\n[Instance]\napp-path=/home/deck/.local/share/flatpak/app/x/x86_64/master/abc/files\n";
        let system = "[Instance]\napp-path=/var/lib/flatpak/app/x/x86_64/master/abc/files\n";
        let b = Path::new("/home/deck/.var/app/x/cache/Romulus-x86_64.flatpak");
        let args = flatpak_install_args(user, b);
        assert_eq!(args[..4], ["--host", "flatpak", "install", "--user"]);
        assert_eq!(
            args.last().unwrap(),
            "/home/deck/.var/app/x/cache/Romulus-x86_64.flatpak"
        );
        assert_eq!(flatpak_install_args(system, b)[3], "--system");
    }

    #[tokio::test]
    async fn check_finds_the_flatpak_bundle_only_when_published() {
        let (_s, src) = server(vec![], Some("x".into())).await;
        let http = reqwest::Client::builder().user_agent("t").build().unwrap();
        assert_eq!(
            check_for(&http, &src, "0.1.0", FLATPAK_ASSET)
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn check_finds_newer_release() {
        let (_s, src) = server(vec![], Some("x".into())).await;
        let http = reqwest::Client::builder().user_agent("t").build().unwrap();
        let r = check(&http, &src, "0.1.0").await.unwrap().unwrap();
        assert_eq!(r.version, "0.2.0");
        assert!(r.sha256_url.is_some());
        assert_eq!(check(&http, &src, "0.2.0").await.unwrap(), None);
    }

    #[tokio::test]
    async fn install_swaps_file_and_keeps_old_copy() {
        let body = vec![42u8; MIN_SIZE as usize + 10];
        let sha = format!("{}  {ASSET_NAME}\n", hex::encode(Sha256::digest(&body)));
        let (_s, src) = server(body.clone(), Some(sha)).await;
        let http = reqwest::Client::new();
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("Romulus.AppImage");
        std::fs::write(&target, b"old version").unwrap();

        let r = check(&http, &src, "0.1.0").await.unwrap().unwrap();
        install_to(&http, &r, &target, |_, _| {}).await.unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), body);
        assert_eq!(
            std::fs::read(dir.path().join("Romulus.old.AppImage")).unwrap(),
            b"old version"
        );
    }

    #[tokio::test]
    async fn bad_checksum_leaves_old_version() {
        let body = vec![1u8; MIN_SIZE as usize + 10];
        let (_s, src) = server(body, Some("0".repeat(64))).await;
        let http = reqwest::Client::new();
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("Romulus.AppImage");
        std::fs::write(&target, b"old version").unwrap();
        let r = check(&http, &src, "0.1.0").await.unwrap().unwrap();
        assert!(matches!(
            install_to(&http, &r, &target, |_, _| {}).await,
            Err(Error::Checksum)
        ));
        assert_eq!(std::fs::read(&target).unwrap(), b"old version");
        assert!(!target.with_extension("download").exists());
    }

    #[tokio::test]
    async fn tiny_download_is_rejected() {
        let (_s, src) = server(b"<html>error</html>".to_vec(), None).await;
        let http = reqwest::Client::new();
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("Romulus.AppImage");
        std::fs::write(&target, b"old").unwrap();
        let r = check(&http, &src, "0.1.0").await.unwrap().unwrap();
        assert!(matches!(
            install_to(&http, &r, &target, |_, _| {}).await,
            Err(Error::TooSmall(_))
        ));
        assert_eq!(std::fs::read(&target).unwrap(), b"old");
    }
}
