use super::*;

#[derive(Args, Debug, Serialize)]
pub struct SourceOpts {
    /// Índice do monitor no macOS (padrão 0). Liste com screens --json
    #[arg(long, conflicts_with_all = ["tab", "video_device"])]
    pub screen: Option<u32>,
    /// ID exato da aba retornado por tabs --json; captura somente o viewport
    #[arg(long, conflicts_with = "video_device")]
    pub tab: Option<String>,
    /// CSS selector único do elemento a capturar (ex.: '#player'); exige --tab
    #[arg(long, requires = "tab")]
    pub selector: Option<String>,
    /// Porta CDP local do Chrome/Chromium/Edge (use com --tab)
    #[arg(long, default_value_t = 9222, requires = "tab", value_parser = clap::value_parser!(u16).range(1..))]
    pub cdp_port: u16,
    /// Entrada nativa: nome/índice AVFoundation, display X11 ou desktop/title=... no Windows
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
            bail!("--selector não pode ser vazio");
        }
        if self.tab.as_deref() == Some("") {
            bail!("--tab exige um ID; liste com aditor tabs --json");
        }
        if !cfg!(target_os = "macos") && self.screen.is_some() {
            bail!("--screen disponível no macOS; nesta plataforma use --video-device ou --tab");
        }
        Ok(())
    }
}

#[derive(Args, Debug)]
#[command(
    after_help = "EXEMPLOS:\n  aditor screenshot --tab <ID> -o aba.png --json\n  aditor print --screen 1 --json\n\nSalva um único PNG; padrão ~/Documents/Pictures/aditor-<epoch>.png.\n--tab captura o viewport da aba, sem barras do navegador. Exige CDP (tabs --help).\nSem --tab: captura nativa, monitor 0 no macOS. Liste com screens --json.\n--selector '#player' recorta um elemento único no documento principal, inclusive fora do viewport.\nErro se ausente, ambíguo ou oculto. Não atravessa iframe/shadow DOM.\nNão há seletor visual nem gravação em background."
)]
pub struct ScreenshotArgs {
    #[command(flatten)]
    pub source: SourceOpts,
    /// Diretório de saída (padrão ~/Documents/Pictures)
    #[arg(long)]
    pub dir: Option<PathBuf>,
    #[command(flatten)]
    pub out: OutOpts,
}

#[derive(Args, Debug)]
pub struct ScreensArgs {
    /// Lista JSON com screen (índice) e name (nome de captura)
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
        bail!("screenshot produz PNG; use um arquivo com extensão .png");
    }
    if output.exists() && !out.yes {
        bail!(
            "saída já existe (use --yes para sobrescrever): {}",
            output.display()
        );
    }
    if !out.dry_run {
        if let Some(parent) = output.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("criar {}", parent.display()))?;
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
    bail!("captura nativa não suportada nesta plataforma; use --tab");
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
        bail!("screens disponível no macOS. Use --video-device para captura nativa ou tabs --json para abas");
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
        bail!("nenhum monitor encontrado pelo AVFoundation; verifique a permissão de Gravação de Tela.\n{raw}");
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
            bail!("ffmpeg falhou ao tirar print (status {status})");
        }
        if std::fs::metadata(&output)?.len() == 0 {
            bail!("print vazio: {}", output.display());
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
