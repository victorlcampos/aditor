//! Per-user installation and checksum-verified updates from official releases.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const RELEASE_API: &str = "https://api.github.com/repos/victorlcampos/aditor/releases/latest";
const DOWNLOAD_PREFIX: &str = "https://github.com/victorlcampos/aditor/releases/download/";
const MAX_ARCHIVE: u64 = 128 * 1024 * 1024;
const MAX_BINARY: u64 = 256 * 1024 * 1024;

fn executable_name() -> &'static str {
    if cfg!(windows) {
        "aditor.exe"
    } else {
        "aditor"
    }
}

fn default_install_dir() -> Result<PathBuf> {
    #[cfg(windows)]
    let directory =
        PathBuf::from(std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is not set")?)
            .join("Aditor")
            .join("bin");
    #[cfg(not(windows))]
    let directory = home()?.join(".local/bin");
    Ok(directory)
}

#[cfg(not(windows))]
fn home() -> Result<PathBuf> {
    Ok(PathBuf::from(
        std::env::var_os("HOME").context("HOME is not set")?,
    ))
}

fn copy_binary(source: &Path, destination: &Path) -> Result<bool> {
    if destination.exists() && fs::canonicalize(source)? == fs::canonicalize(destination)? {
        return Ok(false);
    }
    if fs::symlink_metadata(destination).is_ok_and(|meta| meta.file_type().is_symlink()) {
        bail!(
            "installation destination is a symlink: {}; choose --install-dir",
            destination.display()
        );
    }
    let parent = destination
        .parent()
        .context("installation path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    std::io::copy(&mut fs::File::open(source)?, &mut staged)?;
    staged
        .as_file()
        .set_permissions(fs::metadata(source)?.permissions())?;
    staged.as_file().sync_all()?;
    staged.persist(destination).with_context(|| {
        format!(
            "install to {}; check write permissions and close other running copies",
            destination.display()
        )
    })?;
    Ok(true)
}

#[cfg(not(windows))]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(not(windows))]
fn profile_paths(home: &Path, shell: &str) -> Vec<PathBuf> {
    match Path::new(shell).file_name().and_then(|s| s.to_str()) {
        Some("zsh") => vec![home.join(".zshrc")],
        Some("bash") => {
            // Do not create .bash_profile and thereby hide an existing .profile.
            let login = [".bash_profile", ".bash_login", ".profile"]
                .into_iter()
                .map(|name| home.join(name))
                .find(|path| path.exists())
                .unwrap_or_else(|| home.join(".profile"));
            vec![home.join(".bashrc"), login]
        }
        Some("fish") => vec![home.join(".config/fish/conf.d/aditor.fish")],
        _ => vec![home.join(".profile")],
    }
}

#[cfg(not(windows))]
fn append_path_entry(profile: &Path, directory: &Path, fish: bool) -> Result<bool> {
    let path = directory
        .to_str()
        .context("install path is not valid UTF-8")?;
    if path.contains(['\n', '\r']) {
        bail!("install path must not contain line breaks");
    }
    let quoted = shell_quote(path);
    let entry = if fish {
        // Fish single-quoted strings escape backslashes and single quotes.
        let quoted = format!("'{}'", path.replace('\\', "\\\\").replace('\'', "\\'"));
        format!("contains -- {quoted} $PATH; or set -gx PATH {quoted} $PATH")
    } else {
        format!("export PATH={quoted}:\"$PATH\"")
    };
    let existing = match fs::read_to_string(profile) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    if existing.lines().any(|line| line == entry) {
        return Ok(false);
    }
    if let Some(parent) = profile.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(profile)
        .with_context(|| format!("update PATH in {}", profile.display()))?;
    write!(file, "\n# Added by aditor --install\n{entry}\n")?;
    Ok(true)
}

#[cfg(not(windows))]
fn configure_path(directory: &Path) -> Result<Vec<PathBuf>> {
    let home = home()?;
    let shell = std::env::var("SHELL").unwrap_or_default();
    let mut profiles = profile_paths(&home, &shell);
    if Path::new(&shell).file_name().is_some_and(|s| s == "zsh") {
        if let Some(zdotdir) = std::env::var_os("ZDOTDIR").filter(|s| !s.is_empty()) {
            profiles = vec![PathBuf::from(zdotdir).join(".zshrc")];
        }
    }
    if Path::new(&shell).file_name().is_some_and(|s| s == "fish") {
        if let Some(config) = std::env::var_os("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
            profiles = vec![PathBuf::from(config).join("fish/conf.d/aditor.fish")];
        }
    }
    let fish = Path::new(&shell).file_name().is_some_and(|s| s == "fish");
    let mut changed = Vec::new();
    for profile in profiles {
        if append_path_entry(&profile, directory, fish)? {
            changed.push(profile);
        }
    }
    Ok(changed)
}

#[cfg(windows)]
fn configure_path(directory: &Path) -> Result<Vec<PathBuf>> {
    // Pass the path as data rather than interpolating it into PowerShell code.
    let script = r#"$ErrorActionPreference = 'Stop'
$dir = $env:ADITOR_INSTALL_PATH
$old = [Environment]::GetEnvironmentVariable('Path', 'User')
$parts = @($old -split ';' | Where-Object { $_ -and $_.TrimEnd('\') -ine $dir.TrimEnd('\') })
$new = (@($dir) + $parts) -join ';'
if ($new -cne $old) { [Environment]::SetEnvironmentVariable('Path', $new, 'User') }
"#;
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("ADITOR_INSTALL_PATH", directory)
        .status()
        .context("configure the Windows user PATH with PowerShell")?;
    if !status.success() {
        bail!(
            "could not update the Windows user PATH; add {} manually",
            directory.display()
        );
    }
    Ok(Vec::new())
}

pub fn install(directory: Option<PathBuf>, no_modify_path: bool, json_output: bool) -> Result<()> {
    let source = std::env::current_exe()?;
    let directory = directory.map(Ok).unwrap_or_else(default_install_dir)?;
    fs::create_dir_all(&directory).with_context(|| format!("create {}", directory.display()))?;
    let directory = fs::canonicalize(directory)?;
    // Avoid Windows extended-length prefixes in PATH entries used by other programs.
    #[cfg(windows)]
    let directory = {
        let text = directory.to_string_lossy();
        if let Some(unc) = text.strip_prefix(r"\\?\UNC\") {
            PathBuf::from(format!(r"\\{unc}"))
        } else {
            PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text))
        }
    };
    let destination = directory.join(executable_name());
    let copied = copy_binary(&source, &destination)?;
    let profiles = if no_modify_path {
        Vec::new()
    } else {
        configure_path(&directory)?
    };
    let hint = if no_modify_path {
        format!("Add {} to PATH if needed.", directory.display())
    } else {
        "Open a new terminal, then run aditor --version. An existing terminal cannot inherit PATH changes from this process.".into()
    };
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "install", "status": if copied { "installed" } else { "already_installed" },
                "version": env!("ADITOR_VERSION"), "output": destination,
                "path_configured": !no_modify_path, "profiles_updated": profiles, "next_step": hint
            }))?
        );
    } else {
        println!(
            "{} → {}",
            if copied {
                "Installed"
            } else {
                "Already installed"
            },
            destination.display()
        );
        println!("{hint}");
    }
    Ok(())
}

fn release_target(os: &str, arch: &str) -> Result<&'static str> {
    match (os, arch) {
        ("macos", "aarch64") => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64") => Ok("x86_64-apple-darwin"),
        ("linux", "x86_64") => Ok("x86_64-unknown-linux-gnu"),
        ("windows", "x86_64") => Ok("x86_64-pc-windows-msvc"),
        _ => bail!("no prebuilt release for {os}/{arch}; update from source instead"),
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

impl Release {
    fn asset_url(&self, name: &str) -> Result<&str> {
        let asset = self
            .assets
            .iter()
            .find(|asset| asset.name == name)
            .with_context(|| {
                format!(
                    "release {} has no {name}; current executable was not changed",
                    self.tag_name
                )
            })?;
        if !asset.browser_download_url.starts_with(DOWNLOAD_PREFIX) {
            bail!("release asset URL is outside the official repository");
        }
        Ok(&asset.browser_download_url)
    }
}

fn download(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>> {
    agent
        .get(url)
        .header("User-Agent", concat!("aditor/", env!("ADITOR_VERSION")))
        .call()
        .with_context(|| format!("download {url}; check network access and GitHub rate limits"))?
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()
        .context("read release download")
}

fn verify_checksum(bytes: &[u8], checksums: &str, filename: &str) -> Result<()> {
    let matches: Vec<_> = checksums
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let digest = fields.next()?;
            let name = fields.next()?.trim_start_matches('*');
            (name == filename && fields.next().is_none()).then_some(digest)
        })
        .collect();
    if matches.len() != 1
        || matches[0].len() != 64
        || !matches[0].bytes().all(|b| b.is_ascii_hexdigit())
    {
        bail!("missing, invalid, or duplicate SHA-256 checksum for {filename}");
    }
    if !format!("{:x}", Sha256::digest(bytes)).eq_ignore_ascii_case(matches[0]) {
        bail!("SHA-256 mismatch for {filename}; current executable was not changed");
    }
    Ok(())
}

fn read_binary(reader: impl Read) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(MAX_BINARY + 1).read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_BINARY {
        bail!("release executable is empty or exceeds the size limit");
    }
    Ok(bytes)
}

fn extract_binary(bytes: &[u8], target: &str) -> Result<Vec<u8>> {
    let windows = target.ends_with("windows-msvc");
    let expected = format!(
        "aditor-{target}/{}",
        if windows { "aditor.exe" } else { "aditor" }
    );
    // Read only the expected regular file into memory. Never unpack archive paths.
    if windows {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
        let mut binary = archive
            .by_name(&expected)
            .context("release ZIP has no expected executable")?;
        if binary.is_dir()
            || binary
                .unix_mode()
                .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            bail!("release executable must be a regular file");
        }
        return read_binary(&mut binary);
    }
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    let mut binary = None;
    for entry in archive.entries()? {
        let entry = entry?;
        if entry.path()?.as_ref() == Path::new(&expected) {
            if binary.is_some() || !entry.header().entry_type().is_file() {
                bail!("release executable must be a unique regular file");
            }
            binary = Some(read_binary(entry)?);
        }
    }
    binary.context("release archive has no expected executable")
}

fn verify_version(binary: &Path, expected: &str) -> Result<()> {
    let output = Command::new(binary)
        .arg("--version")
        .output()
        .context("verify downloaded CLI version")?;
    if !output.status.success()
        || String::from_utf8_lossy(&output.stdout).trim() != format!("aditor {expected}")
    {
        bail!(
            "downloaded CLI version does not match the release; current executable was not changed"
        );
    }
    Ok(())
}

pub fn update(json_output: bool) -> Result<()> {
    let target = release_target(std::env::consts::OS, std::env::consts::ARCH)?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .https_only(true)
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .into();
    if !json_output {
        eprintln!("Checking the latest Aditor release…");
    }
    let release: Release =
        serde_json::from_slice(&download(&agent, RELEASE_API, 2 * 1024 * 1024)?)?;
    let version = release
        .tag_name
        .strip_prefix('v')
        .filter(|v| !v.is_empty())
        .context("invalid release tag")?;
    let current = env!("ADITOR_VERSION");
    let executable = std::env::current_exe()?;
    if version == current {
        report_update("up_to_date", current, version, &executable, json_output)?;
        return Ok(());
    }
    let filename = format!(
        "aditor-{target}.{}",
        if cfg!(windows) { "zip" } else { "tar.gz" }
    );
    let archive_url = release.asset_url(&filename)?;
    let checksum_url = release.asset_url("SHA256SUMS")?;
    let checksums = String::from_utf8(download(&agent, checksum_url, 64 * 1024)?)?;
    if !json_output {
        eprintln!("Downloading {} ({target})…", release.tag_name);
    }
    let archive = download(&agent, archive_url, MAX_ARCHIVE)?;
    verify_checksum(&archive, &checksums, &filename)?;
    let bytes = extract_binary(&archive, target)?;
    let parent = executable
        .parent()
        .context("executable has no parent directory")?;
    let mut staged = tempfile::Builder::new()
        .prefix(".aditor-update-")
        .suffix(std::env::consts::EXE_SUFFIX)
        .tempfile_in(parent)
        .with_context(|| {
            format!(
                "cannot write to {}; use aditor --install for a user-owned copy",
                parent.display()
            )
        })?;
    staged.write_all(&bytes)?;
    staged
        .as_file()
        .set_permissions(fs::metadata(&executable)?.permissions())?;
    staged.as_file().sync_all()?;
    let path = staged.into_temp_path();
    verify_version(&path, version)?;
    self_replace::self_replace(&path).context("replace the executable; check write permissions")?;
    report_update("updated", current, version, &executable, json_output)
}

fn report_update(
    status: &str,
    previous: &str,
    version: &str,
    output: &Path,
    json_output: bool,
) -> Result<()> {
    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "action": "update", "status": status, "previous_version": previous,
                "version": version, "output": output
            }))?
        );
    } else if status == "up_to_date" {
        println!("Already up to date: aditor {version}");
    } else {
        println!("Updated {previous} → {version}\n{}", output.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksums_reject_tampering_missing_and_duplicate_entries() {
        let bytes = b"verified release";
        let hash = format!("{:x}", Sha256::digest(bytes));
        let line = format!("{hash}  aditor.zip\r\n");
        verify_checksum(bytes, &line, "aditor.zip").unwrap();
        assert!(verify_checksum(b"modified", &line, "aditor.zip").is_err());
        assert!(verify_checksum(bytes, &line, "another.zip").is_err());
        assert!(verify_checksum(bytes, &(line.clone() + &line), "aditor.zip").is_err());
        assert!(verify_checksum(bytes, "invalid aditor.zip", "aditor.zip").is_err());
    }

    #[test]
    fn only_supported_release_targets_are_selected() {
        for (os, arch, target) in [
            ("macos", "aarch64", "aarch64-apple-darwin"),
            ("macos", "x86_64", "x86_64-apple-darwin"),
            ("linux", "x86_64", "x86_64-unknown-linux-gnu"),
            ("windows", "x86_64", "x86_64-pc-windows-msvc"),
        ] {
            assert_eq!(release_target(os, arch).unwrap(), target);
        }
        assert!(release_target("linux", "aarch64").is_err());
    }

    #[test]
    fn install_replaces_existing_copy_and_is_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source");
        let destination = directory.path().join("bin/aditor");
        fs::write(&source, b"first").unwrap();
        assert!(copy_binary(&source, &destination).unwrap());
        fs::write(&source, b"second").unwrap();
        assert!(copy_binary(&source, &destination).unwrap());
        assert_eq!(fs::read(&destination).unwrap(), b"second");
        assert!(!copy_binary(&destination, &destination).unwrap());
    }

    #[test]
    fn archive_extraction_reads_only_the_expected_binary() {
        let target = "x86_64-unknown-linux-gnu";
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        {
            let mut archive = tar::Builder::new(&mut gzip);
            for (name, contents) in [
                (
                    "aditor-x86_64-unknown-linux-gnu/aditor",
                    b"binary".as_slice(),
                ),
                ("unrelated/file", b"ignored".as_slice()),
            ] {
                let mut header = tar::Header::new_gnu();
                header.set_size(contents.len() as u64);
                header.set_mode(0o755);
                header.set_cksum();
                archive.append_data(&mut header, name, contents).unwrap();
            }
            archive.finish().unwrap();
        }
        let bytes = gzip.finish().unwrap();
        assert_eq!(extract_binary(&bytes, target).unwrap(), b"binary");
        assert!(extract_binary(&bytes, "aarch64-apple-darwin").is_err());
    }

    #[test]
    fn windows_zip_extracts_the_release_executable() {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        archive
            .start_file(
                "aditor-x86_64-pc-windows-msvc/aditor.exe",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(b"windows binary").unwrap();
        let bytes = archive.finish().unwrap().into_inner();
        assert_eq!(
            extract_binary(&bytes, "x86_64-pc-windows-msvc").unwrap(),
            b"windows binary"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn path_configuration_preserves_profiles_quotes_paths_and_does_not_duplicate() {
        let directory = tempfile::tempdir().unwrap();
        let profile = directory.path().join(".zshrc");
        let install = directory.path().join("user's tools/bin");
        fs::write(&profile, "# user settings\n").unwrap();
        assert!(append_path_entry(&profile, &install, false).unwrap());
        assert!(!append_path_entry(&profile, &install, false).unwrap());
        let contents = fs::read_to_string(&profile).unwrap();
        assert!(contents.starts_with("# user settings\n"));
        assert!(contents.contains("user'\\''s tools/bin"));
        let result = Command::new("sh")
            .args(["-c", ". \"$1\"; printf '%s' \"$PATH\"", "sh"])
            .arg(&profile)
            .output()
            .unwrap();
        assert!(result.status.success());
        assert!(String::from_utf8(result.stdout)
            .unwrap()
            .starts_with(install.to_str().unwrap()));
    }

    #[cfg(not(windows))]
    #[test]
    fn bash_install_preserves_existing_login_profile_precedence() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path();
        fs::write(home.join(".profile"), "# existing login settings\n").unwrap();
        assert_eq!(
            profile_paths(home, "/bin/bash"),
            vec![home.join(".bashrc"), home.join(".profile")]
        );
        fs::write(home.join(".bash_login"), "# preferred login settings\n").unwrap();
        assert_eq!(
            profile_paths(home, "/bin/bash"),
            vec![home.join(".bashrc"), home.join(".bash_login")]
        );
        assert!(!home.join(".bash_profile").exists());
    }
}
