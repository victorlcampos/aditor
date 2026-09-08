use super::*;

#[derive(Args, Debug, Serialize)]
pub struct SourceOpts {
    /// Monitor index on macOS (default: 0). List with screens --json
    #[arg(long, conflicts_with_all = ["tab", "video_device"])]
    pub screen: Option<u32>,
    /// Exact tab ID returned by tabs --json; captures only the viewport
    #[arg(long, conflicts_with = "video_device")]
    pub tab: Option<String>,
    /// Unique CSS selector of the element to capture (e.g. '#player'); requires --tab
    #[arg(long, requires = "tab")]
    pub selector: Option<String>,
    /// Local Chrome/Chromium/Edge CDP port (use with --tab)
    #[arg(long, default_value_t = 9222, requires = "tab", value_parser = clap::value_parser!(u16).range(1..))]
    pub cdp_port: u16,
    /// Native input: AVFoundation name/index, X11 display, or desktop/title=... on Windows
    #[arg(long)]
    pub video_device: Option<String>,
}

impl SourceOpts {
    pub fn validate(&self) -> Result<()> {
        if self
            .selector
            .as_deref()
            .is_some_and(|s| s.trim().is_empty())
        {
            bail!("--selector cannot be empty");
        }
        if self.tab.as_deref() == Some("") {
            bail!("--tab requires an ID; list tabs with aditor tabs --json");
        }
        if !cfg!(target_os = "macos") && self.screen.is_some() {
            bail!("--screen is available on macOS; on this platform use --video-device or --tab");
        }
        Ok(())
    }
}

#[derive(Args, Debug)]
#[command(
    after_help = "EXAMPLES:\n  aditor screenshot --tab <ID> -o tab.png --json\n  aditor print --screen 1 --json\n\nSave a single PNG; default: ~/Documents/Pictures/aditor-<epoch>.png.\n--tab captures the tab viewport, without browser chrome. Requires CDP (tabs --help).\nWithout --tab: native capture, monitor 0 on macOS. List with screens --json.\n--selector '#player' crops a unique element in the main document, even outside the viewport.\nError if missing, ambiguous, or hidden. Does not cross iframe/shadow DOM boundaries.\nNo visual picker or background recording."
)]
pub struct ScreenshotArgs {
    #[command(flatten)]
    pub source: SourceOpts,
    /// Output directory (default: ~/Documents/Pictures)
    #[arg(long)]
    pub dir: Option<PathBuf>,
    #[command(flatten)]
    pub out: OutOpts,
}

#[derive(Args, Debug)]
pub struct ScreensArgs {
    /// JSON list with screen (index) and name (capture name)
    #[arg(long)]
    pub json: bool,
}

pub fn output_path(out: &OutOpts, dir: Option<&Path>, extension: &str) -> Result<PathBuf> {
    let output = match &out.output {
        Some(p) => p.clone(),
        None => {
            let default = if extension == "png" {
                default_record_dir().with_file_name("Pictures")
            } else {
                default_record_dir()
            };
            let dir = dir.unwrap_or(&default);
            let base = format!("aditor-{}", now_epoch());
            let mut candidate = dir.join(format!("{base}.{extension}"));
            let mut n = 1;
            while candidate.exists() {
                candidate = dir.join(format!("{base}-{n}.{extension}"));
                n += 1;
            }
            candidate
        }
    };
    if extension == "png" && output.extension().and_then(|s| s.to_str()) != Some("png") {
        bail!("screenshot produces PNG; use a file with the .png extension");
    }
    if output.exists() && !out.yes {
        bail!(
            "output already exists (use --yes to overwrite): {}",
            output.display()
        );
    }
    if !out.dry_run {
        if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create {}", parent.display()))?;
        }
    }
    Ok(if output.is_absolute() {
        output
    } else {
        std::env::current_dir()?.join(output)
    })
}

pub fn input_args(
    source: &SourceOpts,
    fps: u32,
    audio: bool,
    audio_device: Option<&str>,
) -> Result<Vec<String>> {
    source.validate()?;
    let mut args = vec![];
    #[cfg(target_os = "macos")]
    {
        let video = source
            .video_device
            .clone()
            .unwrap_or_else(|| format!("Capture screen {}", source.screen.unwrap_or(0)));
        args.extend(["-f", "avfoundation", "-framerate"].map(String::from));
        args.push(fps.to_string());
        args.extend(["-capture_cursor", "1", "-i"].map(String::from));
        args.push(format!(
            "{video}:{}",
            if audio {
                audio_device.unwrap_or("default")
            } else {
                "none"
            }
        ));
    }
    #[cfg(target_os = "linux")]
    {
        args.extend(["-f", "x11grab", "-framerate"].map(String::from));
        args.push(fps.to_string());
        args.push("-i".into());
        args.push(
            source
                .video_device
                .clone()
                .unwrap_or_else(|| std::env::var("DISPLAY").unwrap_or_else(|_| ":0.0".into())),
        );
        if audio {
            args.extend(["-f", "pulse", "-i", audio_device.unwrap_or("default")].map(String::from));
        }
    }
    #[cfg(target_os = "windows")]
    {
        args.extend(["-f", "gdigrab", "-framerate"].map(String::from));
        args.push(fps.to_string());
        args.push("-i".into());
        args.push(
            source
                .video_device
                .clone()
                .unwrap_or_else(|| "desktop".into()),
        );
        if audio {
            args.extend(["-f", "dshow", "-i"].map(String::from));
            args.push(format!("audio={}", audio_device.unwrap_or("default")));
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    bail!("native capture is not supported on this platform; use --tab");
    Ok(args)
}

#[derive(Debug, Serialize, PartialEq)]
pub struct Screen {
    screen: u32,
    name: String,
}

fn parse_screens(raw: &str) -> Vec<Screen> {
    let mut screens = vec![];
    for line in raw.lines() {
        if let Some((_, suffix)) = line.split_once("Capture screen ") {
            if let Ok(screen) = suffix.trim().parse::<u32>() {
                if !screens.iter().any(|s: &Screen| s.screen == screen) {
                    screens.push(Screen {
                        screen,
                        name: format!("Capture screen {screen}"),
                    });
                }
            }
        }
    }
    screens.sort_by_key(|s| s.screen);
    screens
}

pub fn screens(a: ScreensArgs) -> Result<()> {
    if !cfg!(target_os = "macos") {
        bail!("screens is available on macOS. Use --video-device for native capture or tabs --json for tabs");
    }
    let ffmpeg = resolve_ffmpeg()?;
    let raw = run_capture(
        &ffmpeg,
        &[
            "-hide_banner",
            "-f",
            "avfoundation",
            "-list_devices",
            "true",
            "-i",
            "",
        ],
    )?;
    let screens = parse_screens(&raw);
    if screens.is_empty() {
        bail!("AVFoundation found no monitors; check Screen Recording permission.\n{raw}");
    }
    if a.json {
        println!("{}", serde_json::to_string_pretty(&screens)?);
    } else {
        for screen in screens {
            println!(
                "{}\t{}\t(use --screen {})",
                screen.screen, screen.name, screen.screen
            );
        }
    }
    Ok(())
}

pub fn screenshot(a: ScreenshotArgs) -> Result<()> {
    a.source.validate()?;
    let output = output_path(&a.out, a.dir.as_deref(), "png")?;
    if a.source.tab.is_some() {
        return browser::screenshot(&a, &output);
    }
    let ffmpeg = resolve_ffmpeg()?;
    let mut args = vec![
        "-hide_banner".into(),
        if a.out.yes { "-y" } else { "-n" }.into(),
    ];
    args.extend(input_args(&a.source, 30, false, None)?);
    args.extend(
        [
            "-frames:v",
            "1",
            "-an",
            "-c:v",
            "png",
            "-f",
            "image2",
            "-update",
            "1",
        ]
        .map(String::from),
    );
    args.push(output.to_string_lossy().into_owned());
    let cmd = shell_quote(&ffmpeg, &args);
    if !a.out.dry_run {
        let status = Command::new(&ffmpeg)
            .args(&args)
            .stdin(Stdio::null())
            .status()?;
        if !status.success() {
            bail!("ffmpeg failed to take a screenshot (status {status})");
        }
        if std::fs::metadata(&output)?.len() == 0 {
            bail!("empty screenshot: {}", output.display());
        }
    }
    if a.out.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "output": output, "format": "png", "dry_run": a.out.dry_run,
                "ffmpeg_cmd": cmd, "size_bytes": std::fs::metadata(&output).ok().map(|m| m.len())
            }))?
        );
    } else if a.out.dry_run {
        println!("{cmd}");
    } else {
        println!("ok → {}", output.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn monitor_indices_are_not_device_indices() {
        let raw = "[AVFoundation @ x] [0] Camera\n[AVFoundation @ x] [1] Capture screen 0\n[AVFoundation @ x] [2] Capture screen 1\n[AVFoundation @ x] [0] Microphone";
        assert_eq!(
            parse_screens(raw),
            vec![
                Screen {
                    screen: 0,
                    name: "Capture screen 0".into()
                },
                Screen {
                    screen: 1,
                    name: "Capture screen 1".into()
                }
            ]
        );
    }
}
