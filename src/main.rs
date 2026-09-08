//! aditor — Agentic Editor.
//!
//! CLI pensada para ser operada por um agente: comandos previsíveis,
//! `--json` para saída parseável, `--dry-run` para inspecionar o ffmpeg
//! gerado, `--yes` para sobrescrever, e binário único que resolve o
//! ffmpeg sozinho (sistema → sidecar → auto-download → embutido).

mod browser;
mod capture;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Binários embutidos (feature `embed-ffmpeg`, build offline-capable)
// ---------------------------------------------------------------------------

#[cfg(feature = "embed-ffmpeg")]
const EMBEDDED_FFMPEG: &[u8] = include_bytes!(env!("ADITOR_FFMPEG_BIN"));
#[cfg(feature = "embed-ffmpeg")]
const EMBEDDED_FFPROBE: &[u8] = include_bytes!(env!("ADITOR_FFPROBE_BIN"));

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

#[derive(Parser, Debug)]
#[command(
    name = "aditor",
    version,
    about = "Captura de abas/telas, prints e edição de vídeo via CLI",
    after_help = "FLUXO PARA AGENTES:
  aditor screens --json                   Lista monitores (macOS)
  aditor tabs --json                      Lista IDs das abas via CDP
  aditor record --tab <ID> --json          Grava só a aba em background
  aditor stop <ID_DA_GRAVACAO> --json      Finaliza e retorna o vídeo
  aditor record --screen 1 --duration 10   Grava o segundo monitor
  aditor screenshot --tab <ID> -o aba.png  Tira um print da aba
  aditor screenshot --screen 0 --json     Tira um print do monitor

Use aditor <comando> --help para detalhes e exemplos. --dry-run inspeciona sem capturar.
Abas: Chrome/Chromium/Edge com CDP habilitado; veja aditor tabs --help."
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Mostra metadados do vídeo (duração, streams, codec, dimensões)
    Info(InfoArgs),
    /// Acelera/desacelera vídeo + áudio sincronizados (ex.: -x 1.5)
    Speed(SpeedArgs),
    /// Corta um trecho (--from/--to/--duration)
    Cut(CutArgs),
    /// Pipeline único: corte + velocidade + codec numa passada só
    Edit(EditArgs),
    /// Recomprime para um codec melhor sem mudar conteúdo
    Convert(ConvertArgs),
    /// Grava uma aba (--tab) ou monitor (--screen); padrão ~/Documents/Videos
    Record(RecordArgs),
    /// Tira um print PNG de uma aba ou tela
    #[command(visible_alias = "print")]
    Screenshot(capture::ScreenshotArgs),
    /// Lista monitores e seus índices para --screen (macOS)
    Screens(capture::ScreensArgs),
    /// Lista abas do Chrome/Chromium/Edge por ID, título e URL
    Tabs(browser::TabsArgs),
    #[command(hide = true, name = "__record-tab")]
    RecordTabWorker { config: PathBuf },
    /// Para uma gravação iniciada por `aditor record` e devolve o caminho do vídeo
    Stop(StopArgs),
    /// Diagnóstico: onde estão ffmpeg/ffprobe, versão, encoders
    Doctor(DoctorArgs),
    /// Baixa build estática de ffmpeg/ffprobe (p/ embed ou uso local)
    FetchFfmpeg(FetchArgs),
}

#[derive(Args, Debug)]
struct InfoArgs {
    input: PathBuf,
    /// Saída machine-readable
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Args, Debug)]
struct SpeedArgs {
    input: PathBuf,
    /// Fator de velocidade (0.25 – 16). Ex.: 1.5 = 50% mais rápido
    #[arg(short = 'x', long)]
    factor: f64,
    #[command(flatten)]
    out: OutOpts,
    #[command(flatten)]
    enc: EncOpts,
    /// Remove o áudio
    #[arg(long, default_value_t = false)]
    mute: bool,
}

#[derive(Args, Debug)]
struct CutArgs {
    input: PathBuf,
    /// Início: segundos ou HH:MM:SS.mmm (ex.: 90, 1:30, 00:01:30.5)
    #[arg(long)]
    from: Option<String>,
    /// Fim (exclusivo com --duration)
    #[arg(long)]
    to: Option<String>,
    /// Duração a partir de --from
    #[arg(long)]
    duration: Option<String>,
    #[command(flatten)]
    out: OutOpts,
    #[command(flatten)]
    enc: EncOpts,
}

#[derive(Args, Debug)]
struct EditArgs {
    input: PathBuf,
    #[arg(short = 'x', long)]
    speed: Option<f64>,
    #[arg(long)]
    from: Option<String>,
    #[arg(long)]
    to: Option<String>,
    #[arg(long)]
    duration: Option<String>,
    #[arg(long, default_value_t = false)]
    mute: bool,
    #[command(flatten)]
    out: OutOpts,
    #[command(flatten)]
    enc: EncOpts,
}

#[derive(Args, Debug)]
struct ConvertArgs {
    input: PathBuf,
    #[command(flatten)]
    out: OutOpts,
    #[command(flatten)]
    enc: EncOpts,
}

#[derive(Args, Debug)]
#[command(after_help = "EXEMPLOS:
  aditor record --tab <ID> --duration 10 -o aba.mp4 --json
  aditor record --screen 1 --json
  aditor stop <ID_DA_GRAVACAO> --json

Sem --duration: inicia em background e retorna um ID para stop.
Com --duration: aguarda a conclusão. Nunca abre um seletor visual.
--tab captura o conteúdo visível da aba, sem barras do browser e sem áudio.
--selector '#player' recorta um elemento único; o retângulo é recalculado a cada frame.\nPrefira contêiner de posição fixa; movimento rápido pode incluir bordas do fundo.\nO elemento deve manter o tamanho; sumir, ficar oculto ou mudar de tamanho encerra com erro.\nSeletores atuam no documento principal, sem atravessar iframe/shadow DOM.\nListe IDs com tabs --json. Abas exigem CDP: veja tabs --help.
--screen usa índices de screens --json; padrão 0 no macOS.
macOS: captura de tela exige permissão de Gravação de Tela para o terminal.")]
struct RecordArgs {
    /// Diretório onde salvar (quando -o/--output não é dado).
    /// Padrão: ~/Documents/Videos (criado se não existir)
    #[arg(long)]
    dir: Option<PathBuf>,
    /// Duração: segundos ou HH:MM:SS (ex.: 10, 1:30).
    /// Com --duration, grava em foreground e sai; sem, grava em
    /// background, devolve um id e termina com `aditor stop <id>`
    #[arg(long)]
    duration: Option<String>,
    /// Frames por segundo (padrão 30)
    #[arg(long, default_value_t = 30)]
    fps: u32,
    #[command(flatten)]
    source: capture::SourceOpts,
    /// Inclui áudio do microfone padrão junto com o vídeo
    #[arg(long, default_value_t = false)]
    audio: bool,
    /// Dispositivo de áudio (índice ou nome do avfoundation/pulse/dshow).
    /// Ex.: 1, "Microfone (MacBook Pro)". Implica --audio
    #[arg(long)]
    audio_device: Option<String>,
    #[command(flatten)]
    out: OutOpts,
    #[command(flatten)]
    enc: EncOpts,
}

#[derive(Args, Debug)]
struct StopArgs {
    /// Id da gravação (o que `aditor record` devolveu).
    /// Sem id, para a única gravação ativa (erra se houver 0 ou 2+)
    id: Option<String>,
    /// Para todas as gravações ativas
    #[arg(long, default_value_t = false)]
    all: bool,
    /// Imprime resumo JSON (p/ o agente) em vez de texto
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Args, Debug)]
struct DoctorArgs {
    #[arg(long, default_value_t = false)]
    json: bool,
}

#[derive(Args, Debug)]
struct FetchArgs {
    /// Diretório de destino (padrão: ./third_party)
    #[arg(long)]
    dir: Option<PathBuf>,
}

/// Opções de saída compartilhadas.
#[derive(Args, Debug, Clone)]
struct OutOpts {
    /// Arquivo de saída. Padrão: speed/cut/edit/convert usam
    /// <nome>-<op>.mp4 ao lado do input; capturas usam o diretório indicado no help
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Sobrescreve sem perguntar (sem isso, recusa se o arquivo existir)
    #[arg(long, default_value_t = false)]
    yes: bool,
    /// Mostra o plano/comando de captura ou edição, sem executar
    #[arg(long, default_value_t = false)]
    dry_run: bool,
    /// Imprime resumo JSON (p/ o agente) em vez de texto
    #[arg(long, default_value_t = false)]
    json: bool,
}

/// Opções de codificação compartilhadas.
#[derive(Args, Debug, Clone)]
struct EncOpts {
    /// Codec de vídeo: auto (HW no Mac, senão SW) | h264 | h264-sw | hevc | hevc-sw | copy
    #[arg(long, default_value = "auto")]
    codec: String,
    /// CRF para encoders por software (menor = melhor; padrão 20)
    #[arg(long)]
    crf: Option<u8>,
    /// Bitrate de vídeo p/ encoder de hardware (padrão 12M)
    #[arg(long, default_value = "12M")]
    video_bitrate: String,
    /// Preset p/ encoder por software (padrão veryfast)
    #[arg(long, default_value = "veryfast")]
    preset: String,
    /// Bitrate de áudio AAC (padrão 160k)
    #[arg(long, default_value = "160k")]
    audio_bitrate: String,
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Cmd::Info(a) => cmd_info(a),
        Cmd::Speed(a) => cmd_speed(a),
        Cmd::Cut(a) => cmd_cut(a),
        Cmd::Edit(a) => cmd_edit(a),
        Cmd::Convert(a) => cmd_convert(a),
        Cmd::Record(a) => cmd_record(a),
        Cmd::Screenshot(a) => capture::screenshot(a),
        Cmd::Screens(a) => capture::screens(a),
        Cmd::Tabs(a) => browser::tabs(a),
        Cmd::RecordTabWorker { config } => browser::worker_entry(&config),
        Cmd::Stop(a) => cmd_stop(a),
        Cmd::Doctor(a) => cmd_doctor(a),
        Cmd::FetchFfmpeg(a) => cmd_fetch(a),
    }
}

// ---------------------------------------------------------------------------
// comandos
// ---------------------------------------------------------------------------

fn cmd_info(a: InfoArgs) -> Result<()> {
    let ffprobe = resolve_ffprobe()?;
    let value = probe(&ffprobe, &a.input)?;
    if a.json {
        println!("{}", serde_json::to_string_pretty(&value)?);
    } else {
        print_human_info(&a.input, &value);
    }
    Ok(())
}

fn cmd_speed(a: SpeedArgs) -> Result<()> {
    check_factor(a.factor)?;
    let num = format!("{:.2}", a.factor);
    let num = num.trim_end_matches('0').trim_end_matches('.');
    let job = Job {
        input: a.input,
        speed: a.factor,
        from: None,
        duration: None,
        mute: a.mute,
        tag: format!("{num}x"),
        out: a.out,
        enc: a.enc,
    };
    run_job(job)
}

fn cmd_cut(a: CutArgs) -> Result<()> {
    let (from, duration) = cut_window(a.from.as_deref(), a.to.as_deref(), a.duration.as_deref())?;
    let job = Job {
        input: a.input,
        speed: 1.0,
        from,
        duration,
        mute: false,
        tag: "cut".to_string(),
        out: a.out,
        enc: a.enc,
    };
    run_job(job)
}

fn cmd_edit(a: EditArgs) -> Result<()> {
    let speed = a.speed.unwrap_or(1.0);
    check_factor(speed)?;
    let (from, duration) = cut_window(a.from.as_deref(), a.to.as_deref(), a.duration.as_deref())?;
    let mut tag = String::new();
    if from.is_some() || duration.is_some() {
        tag.push_str("cut-");
    }
    if (speed - 1.0).abs() > f64::EPSILON {
        tag.push_str(&format!("{speed}x-"));
    }
    tag.push_str("edit");
    let job = Job {
        input: a.input,
        speed,
        from,
        duration,
        mute: a.mute,
        tag,
        out: a.out,
        enc: a.enc,
    };
    run_job(job)
}

fn cmd_convert(a: ConvertArgs) -> Result<()> {
    let job = Job {
        input: a.input,
        speed: 1.0,
        from: None,
        duration: None,
        mute: false,
        tag: "convert".to_string(),
        out: a.out,
        enc: a.enc,
    };
    run_job(job)
}

fn cmd_record(a: RecordArgs) -> Result<()> {
    let duration = a.duration.as_deref().map(parse_ts).transpose()?;
    if let Some(d) = duration {
        if d <= 0.0 {
            bail!("--duration deve ser maior que zero");
        }
    }
    if a.fps == 0 || a.fps > 120 {
        bail!("--fps deve estar entre 1 e 120 (recebido {})", a.fps);
    }
    a.source.validate()?;
    let with_audio = a.audio || a.audio_device.is_some();
    if a.source.tab.is_some() && with_audio {
        bail!("--tab não suporta áudio; remova --audio/--audio-device");
    }

    let ffmpeg = resolve_ffmpeg()?;
    let encoders = run_capture(&ffmpeg, &["-hide_banner", "-encoders"])?;
    let codec_label = a.enc.codec.to_lowercase();
    if codec_label == "copy" {
        bail!("--codec copy não vale para record (precisa codificar a captura)");
    }
    let (video_codec_name, vopts) = pick_video_codec(&codec_label, &encoders, &a.enc)?;

    let output = capture::output_path(&a.out, a.dir.as_deref(), "mp4")?;

    let mut args: Vec<String> = vec!["-hide_banner".into()];
    if a.out.yes {
        args.push("-y".into());
    } else {
        args.push("-n".into());
    }

    if a.source.tab.is_some() {
        args.extend(
            [
                "-probesize",
                "32",
                "-analyzeduration",
                "0",
                "-f",
                "image2pipe",
                "-framerate",
            ]
            .map(String::from),
        );
        args.push(a.fps.to_string());
        args.extend(["-c:v", "mjpeg", "-i", "pipe:0"].map(String::from));
    } else {
        args.extend(capture::input_args(
            &a.source,
            a.fps,
            with_audio,
            a.audio_device.as_deref(),
        )?);
    }

    // Duração limite (opção de saída; sem --duration grava até Ctrl+C).
    if let Some(d) = duration {
        args.push("-t".into());
        args.push(format!("{d:.3}"));
    }

    args.extend(vopts);
    args.extend(["-r".into(), a.fps.to_string()]);
    args.extend(["-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2"].map(String::from));
    args.push("-pix_fmt".into());
    args.push("yuv420p".into());
    args.push("-movflags".into());
    args.push("+faststart".into());
    if with_audio {
        args.push("-c:a".into());
        args.push("aac".into());
        args.push("-b:a".into());
        args.push(a.enc.audio_bitrate.clone());
    } else {
        args.push("-an".into());
    }
    args.push(output.to_string_lossy().to_string());

    let cmd_str = shell_quote(&ffmpeg, &args);
    if a.source.tab.is_some() {
        return browser::record(&a, &ffmpeg, &args, &output, duration, &video_codec_name);
    }
    let background = duration.is_none();
    if a.out.dry_run {
        if a.out.json {
            let s = serde_json::json!({
                "output": output.to_string_lossy(),
                "dir": output.parent().map(|p| p.to_string_lossy()),
                "fps": a.fps,
                "duration": duration,
                "background": background,
                "stop": "aditor stop <id> (só p/ gravação em background)",
                "audio": with_audio,
                "video_codec": video_codec_name,
                "ffmpeg_cmd": cmd_str,
                "dry_run": true,
            });
            println!("{}", serde_json::to_string_pretty(&s)?);
        } else {
            println!("{cmd_str}");
        }
        return Ok(());
    }

    if background {
        return start_background_recording(
            &ffmpeg,
            &args,
            &cmd_str,
            &output,
            a.fps,
            with_audio,
            &video_codec_name,
            a.out.json,
        );
    }

    eprintln!("gravando: {cmd_str}");
    let status = Command::new(&ffmpeg)
        .args(&args)
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("executar {}", ffmpeg.display()))?;
    if !status.success() {
        bail!("ffmpeg falhou (status {status})");
    }
    if !output.exists() {
        bail!("ffmpeg saiu ok mas não achei {}", output.display());
    }

    if a.out.json {
        let (dur, size) = probe_output(&output);
        let s = serde_json::json!({
            "output": output.to_string_lossy(),
            "fps": a.fps,
            "duration": dur,
            "size_bytes": size,
            "audio": with_audio,
            "video_codec": video_codec_name,
            "ffmpeg_cmd": cmd_str,
            "dry_run": false,
        });
        println!("{}", serde_json::to_string_pretty(&s)?);
    } else {
        println!("ok → {}", output.display());
    }
    Ok(())
}

/// Sonda duração (s) e tamanho (bytes) de um vídeo; (None, size) se o probe falhar.
fn probe_output(output: &Path) -> (Option<f64>, Option<u64>) {
    let size = std::fs::metadata(output).map(|m| m.len()).ok();
    let dur = resolve_ffprobe()
        .and_then(|fp| probe(&fp, output))
        .ok()
        .and_then(|v| {
            v["format"]["duration"]
                .as_str()
                .and_then(|s| s.parse::<f64>().ok())
        });
    (dur, size)
}

/// Diretório padrão de gravação: ~/Documents/Videos.
fn default_record_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Documents").join("Videos")
}

fn now_epoch() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// gravação em background: record (sem --duration) ↔ stop <id>
// ---------------------------------------------------------------------------

/// Sessão de gravação em background (um arquivo por gravação ativa).
#[derive(Serialize, Deserialize)]
struct RecSession {
    id: String,
    pid: u32,
    output: String,
    log: String,
    fps: u32,
    audio: bool,
    video_codec: String,
    ffmpeg_cmd: String,
    started_epoch: u64,
    #[serde(default)]
    worker_config: Option<PathBuf>,
}

/// Diretório das sessões: $XDG_CACHE_HOME/aditor/rec ou ~/.cache/aditor/rec.
fn rec_sessions_dir() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let dir = base.join("aditor").join("rec");
    std::fs::create_dir_all(&dir).with_context(|| format!("criar {}", dir.display()))?;
    Ok(dir)
}

fn rec_id(sessions: &Path) -> String {
    let base = format!("rec-{}-{}", now_epoch(), std::process::id());
    let mut cand = base.clone();
    let mut n = 1;
    while sessions.join(format!("{cand}.json")).exists() {
        n += 1;
        cand = format!("{base}-{n}");
    }
    cand
}

#[allow(clippy::too_many_arguments)]
fn start_background_recording(
    ffmpeg: &Path,
    args: &[String],
    cmd_str: &str,
    output: &Path,
    fps: u32,
    with_audio: bool,
    video_codec: &str,
    json: bool,
) -> Result<()> {
    let sessions = rec_sessions_dir()?;
    let id = rec_id(&sessions);
    let log_path = sessions.join(format!("{id}.log"));
    let log = std::fs::File::create(&log_path)
        .with_context(|| format!("criar {}", log_path.display()))?;
    let mut child = Command::new(ffmpeg)
        .args(args)
        .stdin(Stdio::null())
        .stdout(log.try_clone().context("clonar log")?)
        .stderr(log)
        .spawn()
        .with_context(|| format!("executar {}", ffmpeg.display()))?;
    let pid = child.id();
    // Fail-fast: se o ffmpeg morreu em <1s (dispositivo/permissão), mostra o log.
    std::thread::sleep(std::time::Duration::from_secs(1));
    if let Some(status) = child
        .try_wait()
        .with_context(|| format!("aguardar {}", ffmpeg.display()))?
    {
        let tail = tail_file(&log_path, 15);
        let _ = std::fs::remove_file(&log_path);
        if output.exists() {
            let _ = std::fs::remove_file(output);
        }
        bail!("ffmpeg falhou ao iniciar (status {status}). Log:\n{tail}");
    }
    std::mem::forget(child); // continua gravando após o aditor sair
    let sess = RecSession {
        id: id.clone(),
        pid,
        output: output.to_string_lossy().to_string(),
        log: log_path.to_string_lossy().to_string(),
        fps,
        audio: with_audio,
        video_codec: video_codec.to_string(),
        ffmpeg_cmd: cmd_str.to_string(),
        started_epoch: now_epoch(),
        worker_config: None,
    };
    std::fs::write(
        sessions.join(format!("{id}.json")),
        serde_json::to_string_pretty(&sess)?,
    )?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "id": id,
                "output": sess.output,
                "pid": pid,
                "log": sess.log,
                "fps": fps,
                "audio": with_audio,
                "video_codec": video_codec,
                "ffmpeg_cmd": cmd_str,
                "stop": format!("aditor stop {id}"),
            }))?
        );
    } else {
        println!("gravando em background — id {id}");
        println!("saída: {}", output.display());
        println!("pare com: aditor stop {id}");
    }
    Ok(())
}

fn tail_file(path: &Path, n: usize) -> String {
    let content = std::fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = content.lines().collect();
    let from = lines.len().saturating_sub(n);
    lines[from..].join("\n")
}

fn pid_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(windows)]
    {
        run_capture(
            &PathBuf::from("tasklist"),
            &["/FI", &format!("PID eq {pid}"), "/NH"],
        )
        .map(|o| o.contains(&pid.to_string()))
        .unwrap_or(false)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

/// Pede p/ o ffmpeg finalizar (SIGINT) e espera; escala p/ TERM/KILL se preciso.
fn terminate_pid(pid: u32) {
    #[cfg(unix)]
    {
        let _ = Command::new("kill")
            .args(["-INT", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        for _ in 0..60 {
            if !pid_alive(pid) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let _ = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        std::thread::sleep(std::time::Duration::from_secs(2));
        if pid_alive(pid) {
            let _ = Command::new("kill")
                .args(["-KILL", &pid.to_string()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        for _ in 0..60 {
            if !pid_alive(pid) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let _ = Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn load_sessions() -> Result<Vec<(PathBuf, RecSession)>> {
    let dir = rec_sessions_dir()?;
    let mut out = vec![];
    let entries = std::fs::read_dir(&dir).with_context(|| format!("ler {}", dir.display()))?;
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        if let Ok(raw) = std::fs::read_to_string(&p) {
            if let Ok(s) = serde_json::from_str::<RecSession>(&raw) {
                out.push((p, s));
            }
        }
    }
    out.sort_by(|a, b| a.1.started_epoch.cmp(&b.1.started_epoch));
    Ok(out)
}

fn cmd_stop(a: StopArgs) -> Result<()> {
    if a.id.is_some() && a.all {
        bail!("passe um id OU --all, não ambos");
    }
    let sessions = load_sessions()?;
    if sessions.is_empty() {
        bail!("nenhuma gravação ativa (veja `aditor record`)");
    }
    let targets: Vec<(PathBuf, RecSession)> = if a.all {
        sessions
    } else if let Some(id) = &a.id {
        let hit: Vec<_> = sessions.into_iter().filter(|(_, s)| s.id == *id).collect();
        if hit.is_empty() {
            bail!("gravação `{id}` não encontrada. Ativas: {}", active_ids()?);
        }
        hit
    } else {
        if sessions.len() > 1 {
            bail!(
                "mais de uma gravação ativa — passe o id. Ativas: {}",
                active_ids()?
            );
        }
        sessions
    };

    let mut results = vec![];
    for (path, sess) in targets {
        results.push(stop_one(&path, &sess)?);
    }
    if a.json {
        println!("{}", serde_json::to_string_pretty(&results)?);
    } else {
        for r in &results {
            println!("ok → {}", r["output"].as_str().unwrap_or("?"));
        }
    }
    Ok(())
}

fn active_ids() -> Result<String> {
    Ok(load_sessions()?
        .iter()
        .map(|(_, s)| s.id.clone())
        .collect::<Vec<_>>()
        .join(", "))
}

fn stop_one(path: &Path, sess: &RecSession) -> Result<serde_json::Value> {
    let output = PathBuf::from(&sess.output);
    if let Some(config) = &sess.worker_config {
        if let Err(error) = browser::stop_worker(config, sess.pid) {
            if config.with_extension("done").exists() {
                browser::cleanup_worker(config);
                let _ = std::fs::remove_file(path);
            }
            return Err(error);
        }
    } else if pid_alive(sess.pid) {
        eprintln!("parando {} (pid {})…", sess.id, sess.pid);
        terminate_pid(sess.pid);
        // dá um respiro p/ o moov ser finalizado no arquivo
        std::thread::sleep(std::time::Duration::from_millis(500));
    } else {
        eprintln!("{} já tinha terminado (pid {} morto)", sess.id, sess.pid);
    }
    let _ = std::fs::remove_file(path); // sessão encerrada, viva ou não
    let (dur, size) = probe_output(&output);
    if !output.exists() || size.unwrap_or(0) == 0 {
        let tail = tail_file(Path::new(&sess.log), 15);
        bail!(
            "gravação {} parou mas não gerou vídeo em {}. Log:\n{tail}",
            sess.id,
            output.display()
        );
    }
    Ok(serde_json::json!({
        "id": sess.id,
        "output": sess.output,
        "duration": dur,
        "size_bytes": size,
        "fps": sess.fps,
        "audio": sess.audio,
    }))
}

fn cmd_doctor(a: DoctorArgs) -> Result<()> {
    let ffmpeg = resolve_ffmpeg()?;
    let ffprobe = resolve_ffprobe()?;
    let fv = run_capture(&ffmpeg, &["-hide_banner", "-version"])?;
    let pv = run_capture(&ffprobe, &["-version"])?;
    let encoders = run_capture(&ffmpeg, &["-hide_banner", "-encoders"])?;
    let report = serde_json::json!({
        "aditor": env!("CARGO_PKG_VERSION"),
        "embed_ffmpeg": cfg!(feature = "embed-ffmpeg"),
        "ffmpeg": ffmpeg.to_string_lossy(),
        "ffprobe": ffprobe.to_string_lossy(),
        "ffmpeg_version": fv.lines().next().unwrap_or(""),
        "ffprobe_version": pv.lines().next().unwrap_or(""),
        "h264_videotoolbox": encoders.contains("h264_videotoolbox"),
        "hevc_videotoolbox": encoders.contains("hevc_videotoolbox"),
        "libx264": encoders.contains("libx264"),
        "libx265": encoders.contains("libx265"),
    });
    if a.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        println!("aditor v{}", env!("CARGO_PKG_VERSION"));
        println!("ffmpeg:  {}", ffmpeg.display());
        println!("ffprobe: {}", ffprobe.display());
        println!("{}", report["ffmpeg_version"].as_str().unwrap_or(""));
        println!(
            "HW mac: h264_videotoolbox={} hevc_videotoolbox={} | SW: libx264={} libx265={}",
            report["h264_videotoolbox"],
            report["hevc_videotoolbox"],
            report["libx264"],
            report["libx265"]
        );
    }
    Ok(())
}

fn cmd_fetch(a: FetchArgs) -> Result<()> {
    let dir = a.dir.unwrap_or_else(|| PathBuf::from("third_party"));
    std::fs::create_dir_all(&dir).with_context(|| format!("criar {}", dir.display()))?;
    eprintln!("baixando build estática de ffmpeg/ffprobe…");
    ffmpeg_sidecar::download::auto_download().context("falha no auto-download do ffmpeg")?;
    let src_dir = ffmpeg_sidecar::paths::sidecar_dir()?;
    let mut copied = vec![];
    for bin in ["ffmpeg", "ffprobe"] {
        let exe = if cfg!(windows) {
            format!("{bin}.exe")
        } else {
            bin.to_string()
        };
        let src = src_dir.join(&exe);
        if src.exists() {
            let dst = dir.join(&exe);
            std::fs::copy(&src, &dst)
                .with_context(|| format!("copiar {} → {}", src.display(), dst.display()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dst, std::fs::Permissions::from_mode(0o755))?;
            }
            copied.push(dst);
        }
    }
    if copied.is_empty() {
        bail!(
            "download concluiu mas não achei os binários em {}",
            src_dir.display()
        );
    }
    println!("pronto em {}:", dir.display());
    for p in &copied {
        println!("  {}", p.display());
    }
    println!("Para embutir no binário, rebuild com:");
    println!(
        "  ADITOR_FFMPEG_BIN=$PWD/{}/ffmpeg ADITOR_FFPROBE_BIN=$PWD/{}/ffprobe cargo build --release --features embed-ffmpeg",
        dir.display(),
        dir.display()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// job (pipeline único de ffmpeg)
// ---------------------------------------------------------------------------

struct Job {
    input: PathBuf,
    speed: f64,
    from: Option<f64>,
    duration: Option<f64>,
    mute: bool,
    tag: String,
    out: OutOpts,
    enc: EncOpts,
}

#[derive(Serialize)]
struct Summary {
    input: String,
    output: String,
    speed: f64,
    cut_from: Option<f64>,
    cut_duration: Option<f64>,
    video_codec: String,
    ffmpeg_cmd: String,
    dry_run: bool,
}

fn run_job(job: Job) -> Result<()> {
    if !job.input.exists() {
        bail!("input não existe: {}", job.input.display());
    }
    let ffmpeg = resolve_ffmpeg()?;
    let ffprobe = resolve_ffprobe()?;
    let meta = probe(&ffprobe, &job.input)?;
    let has_audio = meta["streams"]
        .as_array()
        .map(|ss| ss.iter().any(|s| s["codec_type"] == "audio"))
        .unwrap_or(false);

    let output = match &job.out.output {
        Some(o) => o.clone(),
        None => default_output(&job.input, &job.tag),
    };
    if output.exists() && !job.out.yes && !job.out.dry_run {
        bail!(
            "saída já existe (use --yes para sobrescrever): {}",
            output.display()
        );
    }

    let codec_label = job.enc.codec.to_lowercase();
    let stream_copy = codec_label == "copy" && (job.speed - 1.0).abs() < f64::EPSILON && !job.mute;
    // copy só vale sem filtros; com corte por keyframe pode ser impreciso —
    // aqui mantemos re-encode como padrão seguro, copy só se pedido.
    let encoders = run_capture(&ffmpeg, &["-hide_banner", "-encoders"])?;

    let mut args: Vec<String> = vec!["-hide_banner".into()];
    if job.out.yes {
        args.push("-y".into());
    } else {
        args.push("-n".into());
    }
    // NOTA: -ss/-t vão ANTES do -i (opções de input). Como output options,
    // o -t quebra os filtros de áudio (atempo vira no-op silencioso no
    // ffmpeg 8) — verificado empiricamente. Como input, o corte é rápido
    // (seek) e os filtros aplicam sobre o trecho certo.
    if let Some(from) = job.from {
        args.push("-ss".into());
        args.push(format!("{from:.3}"));
    }
    if let Some(d) = job.duration {
        args.push("-t".into());
        args.push(format!("{d:.3}"));
    }
    args.push("-i".into());
    args.push(job.input.to_string_lossy().to_string());

    let video_codec_name: String;
    if stream_copy {
        args.push("-c".into());
        args.push("copy".into());
        video_codec_name = "copy".into();
    } else {
        // vídeo (pick_video_codec já inclui o -c:v do encoder)
        let (vcodec, vopts) = pick_video_codec(&codec_label, &encoders, &job.enc)?;
        video_codec_name = vcodec;
        args.extend(vopts);
        args.push("-pix_fmt".into());
        args.push("yuv420p".into());
        args.push("-movflags".into());
        args.push("+faststart".into());

        // velocidade de vídeo
        let mut vf: Vec<String> = vec![];
        if (job.speed - 1.0).abs() > f64::EPSILON {
            vf.push(format!("setpts=PTS/{:.6}", job.speed));
        }
        if !vf.is_empty() {
            args.push("-filter:v".into());
            args.push(vf.join(","));
        }

        // áudio
        if job.mute || !has_audio {
            args.push("-an".into());
        } else {
            args.push("-c:a".into());
            args.push("aac".into());
            args.push("-b:a".into());
            args.push(job.enc.audio_bitrate.clone());
            if (job.speed - 1.0).abs() > f64::EPSILON {
                args.push("-filter:a".into());
                args.push(atempo_chain(job.speed));
            }
        }
    }
    args.push(output.to_string_lossy().to_string());

    let cmd_str = shell_quote(&ffmpeg, &args);
    if job.out.dry_run {
        if job.out.json {
            let s = Summary {
                input: job.input.to_string_lossy().to_string(),
                output: output.to_string_lossy().to_string(),
                speed: job.speed,
                cut_from: job.from,
                cut_duration: job.duration,
                video_codec: video_codec_name,
                ffmpeg_cmd: cmd_str.clone(),
                dry_run: true,
            };
            println!("{}", serde_json::to_string_pretty(&s)?);
        } else {
            println!("{cmd_str}");
        }
        return Ok(());
    }

    eprintln!("rodando: {cmd_str}");
    let status = Command::new(&ffmpeg)
        .args(&args)
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("executar {}", ffmpeg.display()))?;
    if !status.success() {
        bail!("ffmpeg falhou (status {status})");
    }

    let summary = Summary {
        input: job.input.to_string_lossy().to_string(),
        output: output.to_string_lossy().to_string(),
        speed: job.speed,
        cut_from: job.from,
        cut_duration: job.duration,
        video_codec: video_codec_name,
        ffmpeg_cmd: cmd_str,
        dry_run: false,
    };
    if job.out.json {
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!("ok → {}", summary.output);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// ffmpeg/ffprobe resolution — binário único, self-contained
// ---------------------------------------------------------------------------

fn resolve_ffmpeg() -> Result<PathBuf> {
    // 1. override explícito
    if let Ok(p) = std::env::var("ADITOR_FFMPEG") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Ok(p);
        }
        bail!("ADITOR_FFMPEG={} não existe", p.display());
    }
    // 2. binário embutido (feature embed-ffmpeg) → extrai p/ cache
    #[cfg(feature = "embed-ffmpeg")]
    {
        if let Ok(p) = ensure_embedded("ffmpeg", EMBEDDED_FFMPEG) {
            return Ok(p);
        }
    }
    // 3. sidecar ao lado do executável
    if let Ok(side) = ffmpeg_sidecar::paths::sidecar_path() {
        if side.exists() {
            return Ok(side);
        }
    }
    // 4. PATH do sistema
    if let Ok(p) = which::which("ffmpeg") {
        return Ok(p);
    }
    // 5. auto-download (self-bootstrap, precisa de rede 1x)
    eprintln!("ffmpeg não encontrado — baixando build estática…");
    ffmpeg_sidecar::download::auto_download().context(
        "ffmpeg não encontrado e o download falhou. Instale via `brew install ffmpeg` ou defina ADITOR_FFMPEG=/caminho/ffmpeg",
    )?;
    if let Ok(side) = ffmpeg_sidecar::paths::sidecar_path() {
        if side.exists() {
            return Ok(side);
        }
    }
    if let Ok(p) = which::which("ffmpeg") {
        return Ok(p);
    }
    bail!("ffmpeg indisponível mesmo após download");
}

fn resolve_ffprobe() -> Result<PathBuf> {
    if let Ok(p) = std::env::var("ADITOR_FFPROBE") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Ok(p);
        }
        bail!("ADITOR_FFPROBE={} não existe", p.display());
    }
    #[cfg(feature = "embed-ffmpeg")]
    {
        if let Ok(p) = ensure_embedded("ffprobe", EMBEDDED_FFPROBE) {
            return Ok(p);
        }
    }
    // ffprobe costuma acompanhar o ffmpeg: procura vizinhos + PATH
    if let Ok(p) = which::which("ffprobe") {
        // prefere o vizinho do ffmpeg resolvido (versões casadas)
        if let Ok(ff) = resolve_ffmpeg() {
            if let Some(dir) = ff.parent() {
                let sib = dir.join(if cfg!(windows) {
                    "ffprobe.exe"
                } else {
                    "ffprobe"
                });
                if sib.exists() {
                    return Ok(sib);
                }
            }
        }
        return Ok(p);
    }
    // tenta garantir via download e re-tenta
    let _ = ffmpeg_sidecar::download::auto_download();
    if let Ok(ff) = resolve_ffmpeg() {
        if let Some(dir) = ff.parent() {
            let sib = dir.join(if cfg!(windows) {
                "ffprobe.exe"
            } else {
                "ffprobe"
            });
            if sib.exists() {
                return Ok(sib);
            }
        }
    }
    if let Ok(p) = which::which("ffprobe") {
        return Ok(p);
    }
    bail!("ffprobe não encontrado (instale ffmpeg completo via `brew install ffmpeg`)");
}

/// Extrai binário embutido para o cache do usuário (uma vez) e devolve o path.
#[cfg(feature = "embed-ffmpeg")]
fn ensure_embedded(name: &str, bytes: &[u8]) -> Result<PathBuf> {
    let dir = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("aditor")
        .join("bin");
    std::fs::create_dir_all(&dir).with_context(|| format!("criar {}", dir.display()))?;
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let dest = dir.join(&exe);
    let marker = dir.join(format!("{exe}.len"));
    let want = bytes.len().to_string();
    let have = std::fs::read_to_string(&marker).unwrap_or_default();
    if dest.exists() && have == want {
        return Ok(dest);
    }
    let mut f = std::fs::File::create(&dest)?;
    use std::io::Write as _;
    f.write_all(bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::write(&marker, want)?;
    Ok(dest)
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn run_capture(bin: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("executar {}", bin.display()))?;
    let mut s = String::from_utf8_lossy(&out.stdout).to_string();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    Ok(s)
}

fn probe(ffprobe: &Path, input: &Path) -> Result<serde_json::Value> {
    let out = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-show_streams",
            "-show_format",
            "-of",
            "json",
            &input.to_string_lossy(),
        ])
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("executar {}", ffprobe.display()))?;
    if !out.status.success() {
        bail!("ffprobe falhou em {}", input.display());
    }
    serde_json::from_slice(&out.stdout).context("parse do JSON do ffprobe")
}

fn print_human_info(input: &Path, v: &serde_json::Value) {
    let fmt = &v["format"];
    let dur = fmt["duration"].as_str().unwrap_or("?");
    let size = fmt["size"].as_str().unwrap_or("?");
    println!("{}:", input.display());
    println!("  duração: {dur}s  tamanho: {size} bytes");
    for s in v["streams"].as_array().cloned().unwrap_or_default() {
        let (i, ct, codec, extra) = (
            s["index"].to_string(),
            s["codec_type"].as_str().unwrap_or("?"),
            s["codec_name"].as_str().unwrap_or("?"),
            match s["codec_type"].as_str().unwrap_or("") {
                "video" => format!(
                    " {}x{} {}fps",
                    s["width"],
                    s["height"],
                    s["avg_frame_rate"].as_str().unwrap_or("?")
                ),
                "audio" => format!(
                    " {}Hz {}ch",
                    s["sample_rate"].as_str().unwrap_or("?"),
                    s["channels"]
                ),
                _ => String::new(),
            },
        );
        println!("  stream {i}: {ct} {codec}{extra}");
    }
}

/// Aceita segundos ("90", "90.5") ou [HH:]MM:SS[.mmm].
fn parse_ts(s: &str) -> Result<f64> {
    let s = s.trim();
    if let Ok(f) = s.parse::<f64>() {
        if !f.is_finite() || f < 0.0 {
            bail!("timestamp inválido: {s}");
        }
        return Ok(f);
    }
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() > 3 || parts.is_empty() {
        bail!("timestamp inválido: {s} (use segundos ou HH:MM:SS.mmm)");
    }
    let mut total = 0.0;
    for p in parts {
        let f: f64 = p
            .parse()
            .with_context(|| format!("timestamp inválido: {s}"))?;
        if !f.is_finite() || f < 0.0 {
            bail!("timestamp inválido: {s}");
        }
        total = total * 60.0 + f;
    }
    if !total.is_finite() || total < 0.0 {
        bail!("timestamp inválido: {s}");
    }
    Ok(total)
}

fn cut_window(
    from: Option<&str>,
    to: Option<&str>,
    duration: Option<&str>,
) -> Result<(Option<f64>, Option<f64>)> {
    if to.is_some() && duration.is_some() {
        bail!("use --to OU --duration, não ambos");
    }
    let from = from.map(parse_ts).transpose()?;
    let duration = match (to, duration) {
        (Some(t), None) => {
            let to = parse_ts(t)?;
            let f = from.unwrap_or(0.0);
            if to <= f {
                bail!("--to ({to}s) deve ser maior que --from ({f}s)");
            }
            Some(to - f)
        }
        (None, Some(d)) => Some(parse_ts(d)?),
        (None, None) => None,
        _ => unreachable!(),
    };
    if from.is_none() && duration.is_none() {
        bail!("corte precisa de --from, --to ou --duration");
    }
    Ok((from, duration))
}

fn check_factor(f: f64) -> Result<()> {
    if !(0.25..=16.0).contains(&f) {
        bail!("fator deve estar entre 0.25 e 16 (recebido {f})");
    }
    Ok(())
}

/// atempo só aceita 0.5–2.0 → encadeia para fatores arbitrários.
fn atempo_chain(mut f: f64) -> String {
    let mut parts = vec![];
    while f > 2.0 {
        parts.push(2.0);
        f /= 2.0;
    }
    while f < 0.5 {
        parts.push(0.5);
        f /= 0.5;
    }
    parts.push(f);
    parts
        .iter()
        .map(|x| format!("atempo={x:.6}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Escolhe encoder real + flags. Retorna (nome-do-codec, args extras).
fn pick_video_codec(codec: &str, encoders: &str, enc: &EncOpts) -> Result<(String, Vec<String>)> {
    let vt_h264 = encoders.contains("h264_videotoolbox");
    let vt_hevc = encoders.contains("hevc_videotoolbox");
    match codec {
        "auto" | "h264" => {
            if vt_h264 {
                Ok((
                    "h264".into(),
                    vec![
                        "-c:v".into(),
                        "h264_videotoolbox".into(),
                        "-b:v".into(),
                        enc.video_bitrate.clone(),
                    ],
                ))
            } else if encoders.contains("libx264") {
                Ok((
                    "h264".into(),
                    vec![
                        "-c:v".into(),
                        "libx264".into(),
                        "-crf".into(),
                        enc.crf.unwrap_or(20).to_string(),
                        "-preset".into(),
                        enc.preset.clone(),
                    ],
                ))
            } else {
                bail!("nenhum encoder h264 disponível (nem videotoolbox nem libx264)");
            }
        }
        "h264-sw" => Ok((
            "h264".into(),
            vec![
                "-c:v".into(),
                "libx264".into(),
                "-crf".into(),
                enc.crf.unwrap_or(20).to_string(),
                "-preset".into(),
                enc.preset.clone(),
            ],
        )),
        "hevc" => {
            if vt_hevc {
                Ok((
                    "hevc".into(),
                    vec![
                        "-c:v".into(),
                        "hevc_videotoolbox".into(),
                        "-b:v".into(),
                        enc.video_bitrate.clone(),
                    ],
                ))
            } else {
                Ok((
                    "hevc".into(),
                    vec![
                        "-c:v".into(),
                        "libx265".into(),
                        "-crf".into(),
                        enc.crf.unwrap_or(22).to_string(),
                        "-preset".into(),
                        enc.preset.clone(),
                    ],
                ))
            }
        }
        "hevc-sw" => Ok((
            "hevc".into(),
            vec![
                "-c:v".into(),
                "libx265".into(),
                "-crf".into(),
                enc.crf.unwrap_or(22).to_string(),
                "-preset".into(),
                enc.preset.clone(),
            ],
        )),
        "copy" => Ok(("copy".into(), vec![])),
        other => bail!("codec desconhecido: {other} (auto|h264|h264-sw|hevc|hevc-sw|copy)"),
    }
}

fn default_output(input: &Path, tag: &str) -> PathBuf {
    let stem = input
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "saida".into());
    let safe_tag: String = tag
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let name = format!("{stem}-{safe_tag}.mp4");
    input
        .parent()
        .map(|p| p.join(&name))
        .unwrap_or_else(|| PathBuf::from(&name))
}

fn shell_quote(bin: &Path, args: &[String]) -> String {
    let mut out = shell_escape(&bin.to_string_lossy());
    for a in args {
        out.push(' ');
        out.push_str(&shell_escape(a));
    }
    out
}

fn shell_escape(s: &str) -> String {
    if s.chars()
        .all(|c| c.is_alphanumeric() || "/._-+:=".contains(c))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
mod cli_tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn cli_schema_is_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn ambiguous_capture_sources_are_rejected() {
        for command in ["record", "screenshot", "print"] {
            assert!(
                Cli::try_parse_from(["aditor", command, "--tab", "ABC", "--screen", "1"]).is_err()
            );
            assert!(Cli::try_parse_from([
                "aditor",
                command,
                "--tab",
                "ABC",
                "--video-device",
                "0"
            ])
            .is_err());
            assert!(Cli::try_parse_from([
                "aditor",
                command,
                "--screen",
                "0",
                "--video-device",
                "0"
            ])
            .is_err());
            assert!(Cli::try_parse_from(["aditor", command, "--cdp-port", "9333"]).is_err());
            assert!(Cli::try_parse_from(["aditor", command, "--tab", "ABC", "--json"]).is_ok());
            assert!(Cli::try_parse_from(["aditor", command]).is_ok());
            assert!(Cli::try_parse_from(["aditor", command, "--selector", "#player"]).is_err());
            assert!(Cli::try_parse_from([
                "aditor",
                command,
                "--tab",
                "ABC",
                "--selector",
                "#player"
            ])
            .is_ok());
        }
    }

    #[test]
    fn invalid_capture_durations_are_rejected() {
        for value in ["NaN", "inf", "1:NaN", "1:-2", "-3"] {
            assert!(parse_ts(value).is_err(), "{value}");
        }
        assert_eq!(parse_ts("1:30.5").unwrap(), 90.5);
    }

    #[test]
    fn old_screen_sessions_remain_readable() {
        let session = serde_json::json!({"id":"rec-1", "pid":123, "output":"video.mp4", "log":"rec.log", "fps":30, "audio":false, "video_codec":"h264", "ffmpeg_cmd":"ffmpeg", "started_epoch":1});
        let session: RecSession = serde_json::from_value(session).unwrap();
        assert!(session.worker_config.is_none());
    }
}
