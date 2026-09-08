//! Timeline insertion, rectangular cropping, and timed text overlays.
use super::*;
use serde_json::{json, Value};

#[derive(Args, Debug)]
pub struct CropArgs {
    input: PathBuf,
    /// Start time (seconds or HH:MM:SS.mmm)
    #[arg(long)]
    from: Option<String>,
    #[arg(long, conflicts_with = "duration")]
    to: Option<String>,
    #[arg(long)]
    duration: Option<String>,
    /// Rectangle width in pixels (requires --height)
    #[arg(long, requires = "height")]
    width: Option<u32>,
    #[arg(long, requires = "width")]
    height: Option<u32>,
    /// Rectangle left edge in pixels
    #[arg(long, default_value_t = 0)]
    x: u32,
    /// Rectangle top edge in pixels
    #[arg(long, default_value_t = 0)]
    y: u32,
    #[command(flatten)]
    out: OutOpts,
    #[command(flatten)]
    enc: EncOpts,
}

#[derive(Args, Debug)]
pub struct AppendArgs {
    input: PathBuf,
    /// Video or still image to insert
    insert: PathBuf,
    /// Insertion time in the original video; 0 prepends, its duration appends
    #[arg(long)]
    at: String,
    /// Treat the inserted file as a still image (requires --duration)
    #[arg(long, requires = "duration")]
    image: bool,
    /// Still-image duration, or maximum duration of the inserted video
    #[arg(long)]
    duration: Option<String>,
    #[command(flatten)]
    out: OutOpts,
    #[command(flatten)]
    enc: EncOpts,
}

#[derive(Args, Debug)]
pub struct WriteArgs {
    input: PathBuf,
    /// Literal UTF-8 text; FFmpeg text expansion is disabled
    #[arg(long)]
    text: String,
    /// Zero-based decoded frame index; display on this frame only
    #[arg(long, conflicts_with_all = ["from", "to", "duration"])]
    frame: Option<u64>,
    #[arg(long)]
    from: Option<String>,
    /// Exclusive end time
    #[arg(long, conflicts_with = "duration")]
    to: Option<String>,
    #[arg(long)]
    duration: Option<String>,
    /// Horizontal position in pixels from the left edge
    #[arg(long, default_value_t = 20)]
    x: u32,
    /// Vertical position in pixels from the top edge
    #[arg(long, default_value_t = 20)]
    y: u32,
    /// Font size in pixels
    #[arg(long, default_value_t = 32)]
    font_size: u32,
    /// Text color: name or hex, optionally with @opacity (e.g. white@0.8)
    #[arg(long, default_value = "white")]
    color: String,
    /// Font file; otherwise FFmpeg selects its default font
    #[arg(long)]
    font_file: Option<PathBuf>,
    #[command(flatten)]
    out: OutOpts,
    #[command(flatten)]
    enc: EncOpts,
}

fn metadata(path: &Path) -> Result<Value> {
    if !path.is_file() {
        bail!("input does not exist: {}", path.display());
    }
    let meta = probe(&resolve_ffprobe()?, path)?;
    video(&meta)?;
    Ok(meta)
}

fn video(meta: &Value) -> Result<&Value> {
    meta["streams"]
        .as_array()
        .and_then(|s| s.iter().find(|s| s["codec_type"] == "video"))
        .context("input has no video stream")
}

fn audio(meta: &Value) -> bool {
    meta["streams"]
        .as_array()
        .is_some_and(|s| s.iter().any(|s| s["codec_type"] == "audio"))
}

fn duration(meta: &Value) -> Result<f64> {
    let value = video(meta)?["duration"]
        .as_str()
        .or_else(|| meta["format"]["duration"].as_str())
        .context("input has no known duration")?;
    positive(value)
}

fn positive(value: &str) -> Result<f64> {
    let value = parse_ts(value)?;
    if value <= 0.0 {
        bail!("duration must be greater than zero");
    }
    Ok(value)
}

fn window(
    from: Option<&str>,
    to: Option<&str>,
    length: Option<&str>,
    total: f64,
) -> Result<(f64, f64)> {
    let start = from.map(parse_ts).transpose()?.unwrap_or(0.0);
    let end = match (to, length) {
        (Some(_), Some(_)) => bail!("use --to OR --duration, not both"),
        (Some(t), None) => parse_ts(t)?,
        (None, Some(d)) => start + positive(d)?,
        _ => total,
    };
    if start >= total || end <= start || end > total + 0.000001 {
        bail!("time interval must be nonempty and within the video duration ({total}s)");
    }
    Ok((start, end))
}

// Escape both the filter-option and filtergraph parsing layers. Command arguments
// are passed directly to FFmpeg, without a shell.
fn filter_value(value: &str) -> String {
    let mut option = String::new();
    for c in value.chars() {
        if "\\':".contains(c) || c.is_whitespace() {
            option.push('\\');
        }
        option.push(c);
    }
    let mut graph = String::new();
    for c in option.chars() {
        if "\\'[],;".contains(c) {
            graph.push('\\');
        }
        graph.push(c);
    }
    graph
}

pub fn crop(a: CropArgs) -> Result<()> {
    let meta = metadata(&a.input)?;
    let timed = a.from.is_some() || a.to.is_some() || a.duration.is_some();
    if !timed && a.width.is_none() {
        bail!("crop requires a time interval or --width and --height");
    }
    if a.width.is_none() && (a.x != 0 || a.y != 0) {
        bail!("--x and --y require a crop rectangle");
    }
    let mut args = vec![];
    if timed {
        let (start, end) = window(
            a.from.as_deref(),
            a.to.as_deref(),
            a.duration.as_deref(),
            duration(&meta)?,
        )?;
        args.extend([
            "-ss".into(),
            start.to_string(),
            "-t".into(),
            (end - start).to_string(),
        ]);
    }
    args.extend(["-i".into(), a.input.to_string_lossy().into_owned()]);
    if let (Some(w), Some(h)) = (a.width, a.height) {
        let v = video(&meta)?;
        if w == 0
            || h == 0
            || w % 2 != 0
            || h % 2 != 0
            || u64::from(a.x) + u64::from(w) > v["width"].as_u64().unwrap_or(0)
            || u64::from(a.y) + u64::from(h) > v["height"].as_u64().unwrap_or(0)
        {
            bail!("crop rectangle must fit within the video and have positive, even dimensions");
        }
        args.extend([
            "-vf".into(),
            format!("crop={w}:{h}:{}:{}:exact=1", a.x, a.y),
        ]);
    }
    args.extend(["-map", "0:v:0", "-map", "0:a:0?"].map(String::from));
    render(&a.input, &[], "crop", &a.out, &a.enc, args)
}

pub fn write(a: WriteArgs) -> Result<()> {
    let meta = metadata(&a.input)?;
    if a.text.is_empty() || a.font_size == 0 {
        bail!("text and font size must be nonempty and positive");
    }
    if let Some(path) = &a.font_file {
        if !path.is_file() {
            bail!("font file does not exist: {}", path.display());
        }
    }
    let enable = if let Some(frame) = a.frame {
        if let Some(count) = video(&meta)?["nb_frames"]
            .as_str()
            .and_then(|s| s.parse::<u64>().ok())
        {
            if frame >= count {
                bail!("--frame must be less than the frame count ({count})");
            }
        }
        format!("eq(n,{frame})")
    } else {
        let (start, end) = window(
            a.from.as_deref(),
            a.to.as_deref(),
            a.duration.as_deref(),
            duration(&meta)?,
        )?;
        format!("gte(t,{start})*lt(t,{end})")
    };
    let ffmpeg = resolve_ffmpeg()?;
    if !run_capture(&ffmpeg, &["-hide_banner", "-filters"])?.contains(" drawtext ") {
        bail!("FFmpeg has no drawtext filter; set ADITOR_FFMPEG to a build with drawtext support");
    }
    let mut filter = format!(
        "drawtext=text={}:expansion=none:fontsize={}:fontcolor={}:x={}:y={}:enable={}",
        filter_value(&a.text),
        a.font_size,
        filter_value(&a.color),
        a.x,
        a.y,
        filter_value(&enable)
    );
    if let Some(path) = a.font_file {
        filter.push_str(&format!(
            ":fontfile={}",
            filter_value(&path.to_string_lossy())
        ));
    }
    render(
        &a.input,
        &[],
        "write",
        &a.out,
        &a.enc,
        vec![
            "-i".into(),
            a.input.to_string_lossy().into_owned(),
            "-vf".into(),
            filter,
            "-map".into(),
            "0:v:0".into(),
            "-map".into(),
            "0:a:0?".into(),
        ],
    )
}

pub fn append(a: AppendArgs) -> Result<()> {
    let base = metadata(&a.input)?;
    let inserted = metadata(&a.insert)?;
    let total = duration(&base)?;
    let at = parse_ts(&a.at)?;
    if at > total {
        bail!("--at must be within the video duration ({total}s)");
    }
    let still = a.image
        || a.insert
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| {
                ["png", "jpg", "jpeg", "bmp", "webp", "tif", "tiff"]
                    .contains(&s.to_ascii_lowercase().as_str())
            });
    let length = if still {
        positive(
            a.duration
                .as_deref()
                .context("inserting an image requires --duration")?,
        )?
    } else {
        let available = duration(&inserted)?;
        a.duration
            .as_deref()
            .map(positive)
            .transpose()?
            .unwrap_or(available)
            .min(available)
    };
    let v = video(&base)?;
    let width = v["width"]
        .as_u64()
        .context("missing video width")?
        .div_ceil(2)
        * 2;
    let height = v["height"]
        .as_u64()
        .context("missing video height")?
        .div_ceil(2)
        * 2;
    let fps = v["avg_frame_rate"]
        .as_str()
        .filter(|s| *s != "0/0")
        .unwrap_or("30");
    let mut args = vec!["-i".into(), a.input.to_string_lossy().into_owned()];
    if still {
        args.extend(["-loop".into(), "1".into(), "-framerate".into(), fps.into()]);
    }
    args.extend(["-i".into(), a.insert.to_string_lossy().into_owned()]);
    let with_audio = audio(&base) || (!still && audio(&inserted));
    let mut segments = vec![];
    if at > 0.0 {
        segments.push((0, 0.0, at, audio(&base)));
    }
    segments.push((1, 0.0, length, !still && audio(&inserted)));
    if at < total {
        segments.push((0, at, total - at, audio(&base)));
    }
    let mut filters = vec![];
    let mut inputs = String::new();
    for (n, (source, start, len, has_audio)) in segments.iter().enumerate() {
        filters.push(format!("[{source}:v:0]setpts=PTS-STARTPTS,trim=start={start}:duration={len},setpts=PTS-STARTPTS,scale={width}:{height}:force_original_aspect_ratio=decrease,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps={fps},format=yuv420p,settb=AVTB[v{n}]"));
        inputs.push_str(&format!("[v{n}]"));
        if with_audio {
            let source = if *has_audio {
                format!("[{source}:a:0]asetpts=PTS-STARTPTS,atrim=start={start}:duration={len},asetpts=PTS-STARTPTS,aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo,apad,atrim=duration={len}")
            } else {
                format!("anullsrc=r=48000:cl=stereo,atrim=duration={len}")
            };
            filters.push(format!("{source}[a{n}]"));
            inputs.push_str(&format!("[a{n}]"));
        }
    }
    filters.push(format!(
        "{inputs}concat=n={}:v=1:a={}[v]{}",
        segments.len(),
        u8::from(with_audio),
        if with_audio { "[a]" } else { "" }
    ));
    args.extend([
        "-filter_complex".into(),
        filters.join(";"),
        "-map".into(),
        "[v]".into(),
    ]);
    if with_audio {
        args.extend(["-map".into(), "[a]".into()]);
    }
    render(&a.input, &[&a.insert], "append", &a.out, &a.enc, args)
}

fn render(
    input: &Path,
    extra_inputs: &[&Path],
    operation: &str,
    out: &OutOpts,
    enc: &EncOpts,
    mut args: Vec<String>,
) -> Result<()> {
    if enc.codec.eq_ignore_ascii_case("copy") {
        bail!("--codec copy is not supported for {operation}; encoding is required");
    }
    let output = out
        .output
        .clone()
        .unwrap_or_else(|| default_output(input, operation));
    if output.exists() {
        let canonical = output.canonicalize()?;
        for source in std::iter::once(input).chain(extra_inputs.iter().copied()) {
            if source.canonicalize()? == canonical {
                bail!("output must not overwrite an input file");
            }
        }
        if !out.yes && !out.dry_run {
            bail!(
                "output already exists (use --yes to overwrite): {}",
                output.display()
            );
        }
    }
    let ffmpeg = resolve_ffmpeg()?;
    let encoders = run_capture(&ffmpeg, &["-hide_banner", "-encoders"])?;
    let (codec, opts) = pick_video_codec(&enc.codec.to_lowercase(), &encoders, enc)?;
    args.splice(
        0..0,
        [
            "-hide_banner".into(),
            if out.yes { "-y" } else { "-n" }.into(),
        ],
    );
    args.extend(opts);
    args.extend(
        [
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-b:a",
            &enc.audio_bitrate,
            "-movflags",
            "+faststart",
        ]
        .map(String::from),
    );
    args.push(output.to_string_lossy().into_owned());
    let command = shell_quote(&ffmpeg, &args);
    if !out.dry_run {
        eprintln!("running: {command}");
        let status = Command::new(&ffmpeg)
            .args(&args)
            .stdin(Stdio::null())
            .status()
            .context("execute FFmpeg")?;
        if !status.success() {
            bail!("ffmpeg failed (status {status})");
        }
    }
    if out.json {
        println!(
            "{}",
            serde_json::to_string_pretty(
                &json!({"input":input,"output":output,"operation":operation,"video_codec":codec,"ffmpeg_cmd":command,"dry_run":out.dry_run})
            )?
        );
    } else if out.dry_run {
        println!("{command}");
    } else {
        println!("ok → {}", output.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn intervals_reject_invalid_and_out_of_bounds_times() {
        for (start, end, len) in [
            ("0", None, Some("0")),
            ("5", None, None),
            ("0", Some("6"), None),
            ("2", Some("1"), None),
            ("NaN", None, None),
        ] {
            assert!(window(Some(start), end, len, 5.0).is_err());
        }
        assert_eq!(window(Some("1"), None, Some("2"), 5.0).unwrap(), (1.0, 3.0));
    }
    #[test]
    fn frame_and_time_options_conflict() {
        assert!(Cli::try_parse_from([
            "aditor", "write", "in.mp4", "--text", "Hi", "--frame", "3", "--from", "1"
        ])
        .is_err());
        assert!(Cli::try_parse_from(["aditor", "crop", "in.mp4", "--width", "100"]).is_err());
        assert!(Cli::try_parse_from([
            "aditor",
            "append",
            "in.mp4",
            "image.png",
            "--at",
            "1",
            "--image"
        ])
        .is_err());
    }
}
