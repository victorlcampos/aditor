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
pub struct CombineArgs {
    /// First video (left in horizontal, top in vertical, base in pip)
    input: PathBuf,
    /// Second video (right in horizontal, bottom in vertical, overlay in pip)
    second: PathBuf,
    /// Layout: horizontal (side-by-side) | vertical (stacked) | pip (overlay)
    #[arg(long, default_value = "horizontal")]
    layout: String,
    /// Output duration: longest | shortest | first | second
    #[arg(long, default_value = "longest")]
    duration: String,
    /// Audio: auto (first with audio, else second) | first | second | mix | none
    #[arg(long, default_value = "auto")]
    audio: String,
    /// Picture-in-picture corner: tl | tr | bl | br
    #[arg(long, default_value = "br")]
    pip_position: String,
    /// Picture-in-picture width as a fraction of the base width (0.05-0.95)
    #[arg(long, default_value_t = 0.3)]
    pip_scale: f64,
    /// Picture-in-picture margin in pixels from the base edges
    #[arg(long, default_value_t = 20)]
    pip_margin: u32,
    #[command(flatten)]
    out: OutOpts,
    #[command(flatten)]
    enc: EncOpts,
}

#[derive(Args, Debug)]
pub struct TimerArgs {
    input: PathBuf,
    /// Timer direction: stopwatch (count up) | countdown (count down)
    #[arg(long, default_value = "stopwatch")]
    mode: String,
    /// Display format: hms (HH:MM:SS) | mmss (MM:SS) | seconds
    #[arg(long, default_value = "mmss")]
    format: String,
    /// Timer start value in seconds (stopwatch counts up from here,
    /// countdown counts down from here; default: 0 for stopwatch,
    /// visible duration for countdown)
    #[arg(long)]
    start: Option<String>,
    /// Show the timer from this video timestamp
    #[arg(long)]
    from: Option<String>,
    /// Show the timer until this video timestamp (exclusive)
    #[arg(long, conflicts_with = "duration")]
    to: Option<String>,
    /// Show the timer for this long starting at --from
    #[arg(long)]
    duration: Option<String>,
    /// Horizontal position (pixels or FFmpeg expression, e.g. w-tw-20)
    #[arg(long, default_value = "w-tw-20")]
    x: String,
    /// Vertical position (pixels or FFmpeg expression, e.g. 20)
    #[arg(long, default_value = "20")]
    y: String,
    /// Font size in pixels
    #[arg(long, default_value_t = 36)]
    font_size: u32,
    /// Text color: name or hex, optionally with @opacity (e.g. white@0.8)
    #[arg(long, default_value = "white")]
    color: String,
    /// Font file; otherwise FFmpeg selects its default font
    #[arg(long)]
    font_file: Option<PathBuf>,
    /// Disable the background box drawn behind the timer for readability
    #[arg(long, default_value_t = false)]
    no_box: bool,
    /// Box color (the background box is drawn unless --no-box is passed)
    #[arg(long, default_value = "black@0.6")]
    box_color: String,
    /// Box border width in pixels (padding around the timer text)
    #[arg(long, default_value_t = 12)]
    box_margin: u32,
    /// Append a tenths-of-a-second digit (MM:SS.d, HH:MM:SS.d, S.d)
    #[arg(long, default_value_t = false)]
    tenths: bool,
    #[command(flatten)]
    out: OutOpts,
    #[command(flatten)]
    enc: EncOpts,
}

#[derive(Args, Debug)]
pub struct OverlayArgs {
    input: PathBuf,
    /// Still image to burn into the video (PNG with transparency works best)
    image: PathBuf,
    /// Horizontal position (pixels or FFmpeg expression, e.g. 20 or W-w-20)
    #[arg(long, default_value = "(W-w)/2")]
    x: String,
    /// Vertical position (pixels or FFmpeg expression, e.g. 20 or H-h-20)
    #[arg(long, default_value = "(H-h)/2")]
    y: String,
    /// Show the overlay from this video timestamp
    #[arg(long)]
    from: Option<String>,
    /// Show the overlay until this video timestamp (exclusive)
    #[arg(long, conflicts_with = "duration")]
    to: Option<String>,
    /// Show the overlay for this long starting at --from
    #[arg(long)]
    duration: Option<String>,
    /// Overlay width in pixels (keeps aspect when used without --height)
    #[arg(long)]
    width: Option<u32>,
    /// Overlay height in pixels (keeps aspect when used without --width)
    #[arg(long)]
    height: Option<u32>,
    /// Opacity from 0 (invisible) to 1 (opaque)
    #[arg(long, default_value_t = 1.0)]
    opacity: f64,
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

// ---------------------------------------------------------------------------
// overlay: burn a still image (e.g. a Canva drawing) into the video
// ---------------------------------------------------------------------------

/// Scale filter for the overlay leg: keeps the aspect ratio when a single
/// side is given, stretches to the exact size with both, and keeps the
/// native (even) size with neither.
fn overlay_scale_filter(width: Option<u32>, height: Option<u32>) -> String {
    match (width, height) {
        (None, None) => "scale=ceil(iw/2)*2:ceil(ih/2)*2".to_string(),
        (Some(w), None) => format!("scale={}:-2", even_dim(u64::from(w))),
        (None, Some(h)) => format!("scale=-2:{}", even_dim(u64::from(h))),
        (Some(w), Some(h)) => format!(
            "scale={}:{}",
            even_dim(u64::from(w)),
            even_dim(u64::from(h))
        ),
    }
}

fn is_still_image(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        ["png", "jpg", "jpeg", "bmp", "webp", "tif", "tiff"]
            .contains(&s.to_ascii_lowercase().as_str())
    })
}

pub fn overlay(a: OverlayArgs) -> Result<()> {
    let base_meta = metadata(&a.input)?;
    if !a.image.is_file() {
        bail!("image does not exist: {}", a.image.display());
    }
    if !a.opacity.is_finite() || !(0.0..=1.0).contains(&a.opacity) {
        bail!("--opacity must be between 0 and 1");
    }
    if a.x.trim().is_empty() || a.y.trim().is_empty() {
        bail!("--x and --y must not be empty");
    }
    if a.width.is_some_and(|w| w == 0) || a.height.is_some_and(|h| h == 0) {
        bail!("--width and --height must be positive");
    }
    let total = duration(&base_meta)?;
    let timed = a.from.is_some() || a.to.is_some() || a.duration.is_some();
    let (visible_start, visible_end) = if timed {
        window(
            a.from.as_deref(),
            a.to.as_deref(),
            a.duration.as_deref(),
            total,
        )?
    } else {
        (0.0, total)
    };
    let fps_s = fmt_sec(stream_fps(video(&base_meta)?));
    let still = is_still_image(&a.image);

    let mut args: Vec<String> = vec!["-i".into(), a.input.to_string_lossy().into_owned()];
    if still {
        args.extend([
            "-loop".into(),
            "1".into(),
            "-framerate".into(),
            fps_s.clone(),
        ]);
    }
    args.extend(["-i".into(), a.image.to_string_lossy().into_owned()]);

    let mut leg = format!(
        "[1:v:0]{},setsar=1,fps={fps_s},format=rgba",
        overlay_scale_filter(a.width, a.height)
    );
    if a.opacity < 1.0 {
        leg.push_str(&format!(",colorchannelmixer=aa={}", a.opacity));
    }
    leg.push_str(",settb=AVTB[ov]");
    // -t caps the output at the base duration (the looped image is infinite).
    args.extend(
        [
            "-filter_complex",
            &[
                format!("[0:v:0]scale=ceil(iw/2)*2:ceil(ih/2)*2,setsar=1,fps={fps_s},format=yuv420p,settb=AVTB[base]"),
                leg,
                format!(
                    "[base][ov]overlay=x={}:y={}:enable=between(t\\,{}\\,{}):eof_action=pass[vout]",
                    filter_value(&a.x),
                    filter_value(&a.y),
                    fmt_sec(visible_start),
                    fmt_sec(visible_end)
                ),
            ]
            .join(";"),
            "-map",
            "[vout]",
            "-map",
            "0:a:0?",
            "-t",
            &fmt_sec(total),
        ]
        .map(String::from),
    );
    render(&a.input, &[&a.image], "overlay", &a.out, &a.enc, args)
}

// ---------------------------------------------------------------------------
// combine: two videos side-by-side, stacked, or picture-in-picture
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Layout {
    Horizontal,
    Vertical,
    Pip,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CombineDuration {
    Longest,
    Shortest,
    First,
    Second,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum CombineAudio {
    Auto,
    First,
    Second,
    Mix,
    None,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PipCorner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Differences below this (seconds) are treated as equal durations.
const COMBINE_EPS: f64 = 0.05;

fn parse_layout(value: &str) -> Result<Layout> {
    match value.trim().to_ascii_lowercase().as_str() {
        "horizontal" | "h" | "side" | "side-by-side" | "sidebyside" | "sbs" => {
            Ok(Layout::Horizontal)
        }
        "vertical" | "v" | "stack" | "stacked" | "top-bottom" | "topbottom" => Ok(Layout::Vertical),
        "pip" | "overlay" | "overlaid" | "picture-in-picture" | "pictureinpicture" => {
            Ok(Layout::Pip)
        }
        other => bail!("unknown layout: {other} (horizontal|vertical|pip)"),
    }
}

fn parse_combine_duration(value: &str) -> Result<CombineDuration> {
    match value.trim().to_ascii_lowercase().as_str() {
        "longest" | "long" | "max" => Ok(CombineDuration::Longest),
        "shortest" | "short" | "min" => Ok(CombineDuration::Shortest),
        "first" | "1" => Ok(CombineDuration::First),
        "second" | "2" => Ok(CombineDuration::Second),
        other => bail!("unknown duration: {other} (longest|shortest|first|second)"),
    }
}

fn parse_combine_audio(value: &str) -> Result<CombineAudio> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(CombineAudio::Auto),
        "first" | "1" => Ok(CombineAudio::First),
        "second" | "2" => Ok(CombineAudio::Second),
        "mix" | "both" | "merge" => Ok(CombineAudio::Mix),
        "none" | "mute" | "silent" => Ok(CombineAudio::None),
        other => bail!("unknown audio: {other} (auto|first|second|mix|none)"),
    }
}

fn parse_pip_corner(value: &str) -> Result<PipCorner> {
    match value
        .trim()
        .to_ascii_lowercase()
        .replace(['-', '_', ' '], "")
        .as_str()
    {
        "tl" | "topleft" => Ok(PipCorner::TopLeft),
        "tr" | "topright" => Ok(PipCorner::TopRight),
        "bl" | "bottomleft" => Ok(PipCorner::BottomLeft),
        "br" | "bottomright" => Ok(PipCorner::BottomRight),
        other => bail!("unknown pip position: {other} (tl|tr|bl|br)"),
    }
}

fn stream_dims(v: &Value) -> Result<(u64, u64)> {
    let w = v["width"].as_u64().context("missing video width")?;
    let h = v["height"].as_u64().context("missing video height")?;
    if w == 0 || h == 0 {
        bail!("video dimensions must be positive");
    }
    Ok((w, h))
}

fn even_dim(value: u64) -> u64 {
    value.div_ceil(2) * 2
}

fn parse_rate(value: &str) -> Option<f64> {
    let value = value.trim();
    if value.is_empty() || value == "0/0" {
        return None;
    }
    if let Some((num, den)) = value.split_once('/') {
        let num: f64 = num.trim().parse().ok()?;
        let den: f64 = den.trim().parse().ok()?;
        if !num.is_finite() || !den.is_finite() || den == 0.0 || num <= 0.0 {
            return None;
        }
        Some(num / den)
    } else {
        let parsed: f64 = value.parse().ok()?;
        if parsed.is_finite() && parsed > 0.0 {
            Some(parsed)
        } else {
            None
        }
    }
}

fn stream_fps(v: &Value) -> f64 {
    for key in ["avg_frame_rate", "r_frame_rate"] {
        if let Some(text) = v[key].as_str() {
            if let Some(rate) = parse_rate(text) {
                if (1.0..=240.0).contains(&rate) {
                    return rate;
                }
            }
        }
    }
    30.0
}

fn fmt_sec(value: f64) -> String {
    format!("{value:.3}")
}

/// Extend or trim a normalized video leg to exactly `out` seconds.
fn fit_video_leg(label: &str, dur: f64, out: f64) -> String {
    if dur > out + COMBINE_EPS {
        format!(
            ",trim=duration={},setpts=PTS-STARTPTS,settb=AVTB[{label}]",
            fmt_sec(out)
        )
    } else if out > dur + COMBINE_EPS {
        format!(
            ",tpad=stop_mode=clone:stop_duration={},settb=AVTB[{label}]",
            fmt_sec(out - dur)
        )
    } else {
        format!(",settb=AVTB[{label}]")
    }
}

/// Normalize one audio source to exactly `out` seconds, or emit silence.
fn audio_leg(input: u32, label: &str, has_audio: bool, dur: f64, out: f64) -> String {
    if has_audio {
        let base =
            format!("[{input}:a:0]aresample=48000,aformat=sample_fmts=fltp:channel_layouts=stereo");
        if dur > out + COMBINE_EPS {
            format!(
                "{base},atrim=duration={},asetpts=PTS-STARTPTS[{label}]",
                fmt_sec(out)
            )
        } else if out > dur + COMBINE_EPS {
            format!(
                "{base},apad=whole_dur={},atrim=duration={}[{label}]",
                fmt_sec(out),
                fmt_sec(out)
            )
        } else {
            format!("{base}[{label}]")
        }
    } else {
        format!("anullsrc=r=48000:cl=stereo:d={}[{label}]", fmt_sec(out))
    }
}

pub fn combine(a: CombineArgs) -> Result<()> {
    let first = metadata(&a.input)?;
    let second = metadata(&a.second)?;
    let layout = parse_layout(&a.layout)?;
    let dur_mode = parse_combine_duration(&a.duration)?;
    let audio_mode = parse_combine_audio(&a.audio)?;
    if !a.pip_scale.is_finite() || !(0.05..=0.95).contains(&a.pip_scale) {
        bail!("--pip-scale must be between 0.05 and 0.95");
    }
    let corner = parse_pip_corner(&a.pip_position)?;
    let v1 = video(&first)?;
    let v2 = video(&second)?;
    let (w1, h1) = stream_dims(v1)?;
    let (w2, h2) = stream_dims(v2)?;
    let d1 = duration(&first)?;
    let d2 = duration(&second)?;
    let out_dur = match dur_mode {
        CombineDuration::Longest => d1.max(d2),
        CombineDuration::Shortest => d1.min(d2),
        CombineDuration::First => d1,
        CombineDuration::Second => d2,
    };
    if out_dur <= 0.0 {
        bail!("output duration must be greater than zero");
    }
    let fps = stream_fps(v1).max(stream_fps(v2));
    let fps_s = fmt_sec(fps);
    let out_s = fmt_sec(out_dur);
    let has1 = audio(&first);
    let has2 = audio(&second);

    let mut filters: Vec<String> = vec![];
    match layout {
        Layout::Horizontal => {
            let target_h = even_dim(h1.max(h2));
            for (i, dur) in [(0, d1), (1, d2)] {
                let base = format!(
                    "[{i}:v:0]scale=-2:{target_h}:flags=lanczos,setsar=1,fps={fps_s},format=yuv420p"
                );
                filters.push(format!(
                    "{base}{}",
                    fit_video_leg(&format!("v{i}"), dur, out_dur)
                ));
            }
            filters.push("[v0][v1]hstack=inputs=2[vout]".to_string());
        }
        Layout::Vertical => {
            let target_w = even_dim(w1.max(w2));
            for (i, dur) in [(0, d1), (1, d2)] {
                let base = format!(
                    "[{i}:v:0]scale={target_w}:-2:flags=lanczos,setsar=1,fps={fps_s},format=yuv420p"
                );
                filters.push(format!(
                    "{base}{}",
                    fit_video_leg(&format!("v{i}"), dur, out_dur)
                ));
            }
            filters.push("[v0][v1]vstack=inputs=2[vout]".to_string());
        }
        Layout::Pip => {
            let base_w = even_dim(w1);
            let base_h = even_dim(h1);
            let base = format!(
                "[0:v:0]scale={base_w}:{base_h}:flags=lanczos,setsar=1,fps={fps_s},format=yuv420p"
            );
            // For odd source dimensions an explicit scale keeps the base even.
            let base = if base_w == w1 && base_h == h1 {
                format!(
                    "[0:v:0]scale=ceil(iw/2)*2:ceil(ih/2)*2,setsar=1,fps={fps_s},format=yuv420p"
                )
            } else {
                base
            };
            filters.push(format!("{base}{}", fit_video_leg("base", d1, out_dur)));
            let overlay_w =
                even_dim(((base_w as f64 * a.pip_scale).round() as u64).clamp(16, base_w));
            let overlay = format!(
                "[1:v:0]scale={overlay_w}:-2:flags=lanczos,setsar=1,fps={fps_s},format=yuv420p"
            );
            // The overlay repeats its last frame past EOF; only trim when longer.
            if d2 > out_dur + COMBINE_EPS {
                filters.push(format!(
                    "{overlay},trim=duration={out_s},setpts=PTS-STARTPTS,settb=AVTB[ov]"
                ));
            } else {
                filters.push(format!("{overlay},settb=AVTB[ov]"));
            }
            let margin = a.pip_margin;
            let (x, y) = match corner {
                PipCorner::TopLeft => (format!("{margin}"), format!("{margin}")),
                PipCorner::TopRight => (format!("W-w-{margin}"), format!("{margin}")),
                PipCorner::BottomLeft => (format!("{margin}"), format!("H-h-{margin}")),
                PipCorner::BottomRight => (format!("W-w-{margin}"), format!("H-h-{margin}")),
            };
            filters.push(format!(
                "[base][ov]overlay=x={x}:y={y}:eof_action=repeat[vout]"
            ));
            let _ = (w2, h2);
        }
    }

    let sources: Vec<(u32, bool)> = match audio_mode {
        CombineAudio::Auto => {
            if has1 {
                vec![(0, true)]
            } else if has2 {
                vec![(1, true)]
            } else {
                vec![]
            }
        }
        CombineAudio::First => vec![(0, has1)],
        CombineAudio::Second => vec![(1, has2)],
        CombineAudio::Mix => vec![(0, has1), (1, has2)],
        CombineAudio::None => vec![],
    };
    // Drop silent legs only when mixing nothing audible at all.
    let audible = sources.iter().any(|(_, has)| *has);
    let mut args = vec![
        "-i".into(),
        a.input.to_string_lossy().into_owned(),
        "-i".into(),
        a.second.to_string_lossy().into_owned(),
        "-filter_complex".into(),
        String::new(), // filled below once audio legs are known
        "-map".into(),
        "[vout]".into(),
    ];
    if sources.is_empty() || (sources.len() == 2 && !audible) {
        // No audio track: rely on the video-only map.
    } else if sources.len() == 1 {
        let (input, has) = sources[0];
        let dur = if input == 0 { d1 } else { d2 };
        filters.push(audio_leg(input, "aout", has, dur, out_dur));
        args.extend(["-map".into(), "[aout]".into()]);
    } else {
        let mut legs = vec![];
        for (k, (input, has)) in sources.iter().enumerate() {
            let dur = if *input == 0 { d1 } else { d2 };
            filters.push(audio_leg(*input, &format!("a{k}"), *has, dur, out_dur));
            legs.push(format!("[a{k}]"));
        }
        filters.push(format!(
            "{}amix=inputs=2:duration=longest:dropout_transition=0:normalize=0,atrim=duration={out_s},aformat=sample_fmts=fltp:channel_layouts=stereo[aout]",
            legs.join("")
        ));
        args.extend(["-map".into(), "[aout]".into()]);
    }
    if let Some(slot) = args.iter().position(|s| s.is_empty()) {
        args[slot] = filters.join(";");
    }
    render(&a.input, &[&a.second], "combine", &a.out, &a.enc, args)
}

// ---------------------------------------------------------------------------
// timer: burn a stopwatch or countdown into the video
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TimerMode {
    Stopwatch,
    Countdown,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TimerFormat {
    Hms,
    Mmss,
    Seconds,
}

fn parse_timer_mode(value: &str) -> Result<TimerMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "stopwatch" | "stop-watch" | "up" | "count-up" | "countup" => Ok(TimerMode::Stopwatch),
        "countdown" | "count-down" | "down" => Ok(TimerMode::Countdown),
        other => bail!("unknown mode: {other} (stopwatch|countdown)"),
    }
}

fn parse_timer_format(value: &str) -> Result<TimerFormat> {
    match value.trim().to_ascii_lowercase().as_str() {
        "hms" | "hhmmss" | "hh:mm:ss" => Ok(TimerFormat::Hms),
        "mmss" | "mm:ss" | "ms" | "mss" => Ok(TimerFormat::Mmss),
        "seconds" | "second" | "s" | "sec" | "secs" => Ok(TimerFormat::Seconds),
        other => bail!("unknown format: {other} (hms|mmss|seconds)"),
    }
}

/// Build the drawtext `text` value for `value` (a raw FFmpeg expression in
/// seconds). Colons and commas stay escaped for the filtergraph; callers must
/// not pass the result through `filter_value` again.
fn timer_text(value: &str, format: TimerFormat, tenths: bool) -> String {
    let tenth = format!(".%{{eif\\:mod(floor(({value})*10)\\,10)}}");
    match format {
        TimerFormat::Mmss => {
            let base = format!(
                "%{{eif\\:{value}/60\\:d\\:2}}\\:%{{eif\\:mod(floor({value})\\,60)\\:d\\:2}}"
            );
            if tenths {
                format!("{base}{tenth}")
            } else {
                base
            }
        }
        TimerFormat::Hms => {
            let base = format!(
                "%{{eif\\:{value}/3600\\:d\\:2}}\\:%{{eif\\:mod(floor(({value})/60)\\,60)\\:d\\:2}}\\:%{{eif\\:mod(floor({value})\\,60)\\:d\\:2}}"
            );
            if tenths {
                format!("{base}{tenth}")
            } else {
                base
            }
        }
        TimerFormat::Seconds => {
            if tenths {
                format!("%{{eif\\:floor({value})\\:d}}{tenth}")
            } else {
                format!("%{{eif\\:floor({value})\\:d}}")
            }
        }
    }
}

/// Assemble the drawtext filter for a pre-escaped timer `text` value.
/// The value is single-quoted because the filtergraph splitter only honors
/// backslash escapes inside quotes; callers must not quote or re-escape it.
fn timer_filter(text: &str, enable: &str, a: &TimerArgs) -> String {
    let mut filter = format!(
        "drawtext=text='{text}':expansion=normal:fontsize={}:fontcolor={}:x={}:y={}:enable={enable}",
        a.font_size,
        filter_value(&a.color),
        filter_value(&a.x),
        filter_value(&a.y),
    );
    if let Some(path) = &a.font_file {
        filter.push_str(&format!(
            ":fontfile={}",
            filter_value(&path.to_string_lossy())
        ));
    }
    // Subtle drop shadow for depth; background box (on by default) for contrast.
    filter.push_str(":shadowcolor=black@0.7:shadowx=2:shadowy=2");
    if !a.no_box {
        filter.push_str(&format!(
            ":box=1:boxcolor={}:boxborderw={}",
            filter_value(&a.box_color),
            a.box_margin
        ));
    }
    filter
}

pub fn timer(a: TimerArgs) -> Result<()> {
    let meta = metadata(&a.input)?;
    let total = duration(&meta)?;
    let timed = a.from.is_some() || a.to.is_some() || a.duration.is_some();
    let (visible_start, visible_end) = if timed {
        window(
            a.from.as_deref(),
            a.to.as_deref(),
            a.duration.as_deref(),
            total,
        )?
    } else {
        (0.0, total)
    };
    let mode = parse_timer_mode(&a.mode)?;
    let format = parse_timer_format(&a.format)?;
    let start_value = match &a.start {
        Some(raw) => {
            let parsed = parse_ts(raw)?;
            if !parsed.is_finite() || parsed < 0.0 {
                bail!("--start must be zero or greater");
            }
            parsed
        }
        None => {
            if mode == TimerMode::Countdown {
                visible_end - visible_start
            } else {
                0.0
            }
        }
    };
    if a.font_size == 0 {
        bail!("font size must be positive");
    }
    if a.x.trim().is_empty() || a.y.trim().is_empty() {
        bail!("--x and --y must not be empty");
    }
    if let Some(path) = &a.font_file {
        if !path.is_file() {
            bail!("font file does not exist: {}", path.display());
        }
    }
    // --dry-run previews the command without requiring a drawtext build.
    if !a.out.dry_run {
        let ffmpeg = resolve_ffmpeg()?;
        if !run_capture(&ffmpeg, &["-hide_banner", "-filters"])?.contains(" drawtext ") {
            bail!(
                "FFmpeg has no drawtext filter; set ADITOR_FFMPEG to a build with drawtext support"
            );
        }
    }
    let value = match mode {
        TimerMode::Stopwatch => {
            format!("({}+t-{})", fmt_sec(start_value), fmt_sec(visible_start))
        }
        TimerMode::Countdown => {
            format!(
                "max({}-t+{}\\,0)",
                fmt_sec(start_value),
                fmt_sec(visible_start)
            )
        }
    };
    let text = timer_text(&value, format, a.tenths);
    let enable = format!(
        "gte(t\\,{})*lt(t\\,{})",
        fmt_sec(visible_start),
        fmt_sec(visible_end)
    );
    let filter = timer_filter(&text, &enable, &a);
    render(
        &a.input,
        &[],
        "timer",
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
    fn sample_timer_args(no_box: bool) -> TimerArgs {
        TimerArgs {
            input: PathBuf::from("in.mp4"),
            mode: "stopwatch".to_string(),
            format: "mmss".to_string(),
            start: None,
            from: None,
            to: None,
            duration: None,
            x: "w-tw-20".to_string(),
            y: "20".to_string(),
            font_size: 32,
            color: "white".to_string(),
            font_file: None,
            no_box,
            box_color: "black@0.6".to_string(),
            box_margin: 12,
            tenths: false,
            out: OutOpts {
                output: None,
                yes: false,
                dry_run: false,
                json: false,
            },
            enc: EncOpts {
                codec: "auto".to_string(),
                crf: None,
                video_bitrate: "12M".to_string(),
                preset: "veryfast".to_string(),
                audio_bitrate: "160k".to_string(),
            },
        }
    }
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
    #[test]
    fn combine_options_accept_layouts_and_reject_bad_values() {
        assert!(parse_layout("horizontal").is_ok());
        assert!(parse_layout("VERTICAL").is_ok());
        assert!(parse_layout("pip").is_ok());
        assert!(parse_layout("diagonal").is_err());
        assert!(parse_combine_duration("longest").is_ok());
        assert!(parse_combine_duration("shortest").is_ok());
        assert!(parse_combine_duration("forever").is_err());
        assert!(parse_combine_audio("auto").is_ok());
        assert!(parse_combine_audio("mix").is_ok());
        assert!(parse_combine_audio("loud").is_err());
        assert!(parse_pip_corner("br").is_ok());
        assert!(parse_pip_corner("top-left").is_ok());
        assert!(parse_pip_corner("center").is_err());
        assert!(Cli::try_parse_from(["aditor", "combine", "a.mp4", "b.mp4"]).is_ok());
        assert!(
            Cli::try_parse_from(["aditor", "combine", "a.mp4", "b.mp4", "--layout", "pip"]).is_ok()
        );
        assert!(Cli::try_parse_from(["aditor", "combine", "a.mp4"]).is_err());
    }
    #[test]
    fn timer_options_and_expressions_are_well_formed() {
        assert!(parse_timer_mode("stopwatch").is_ok());
        assert!(parse_timer_mode("countdown").is_ok());
        assert!(parse_timer_mode("alarm").is_err());
        assert!(parse_timer_format("hms").is_ok());
        assert!(parse_timer_format("mmss").is_ok());
        assert!(parse_timer_format("seconds").is_ok());
        assert!(parse_timer_format("binary").is_err());
        let up = "(0.000+t-0.000)".to_string();
        let down = "max(60.000-t+0.000\\,0)".to_string();
        let mmss = timer_text(&up, TimerFormat::Mmss, false);
        assert!(mmss.contains("%{eif\\:") && mmss.contains("\\:d\\:2}"));
        assert!(mmss.contains("\\,60"), "{mmss}");
        assert!(timer_text(&up, TimerFormat::Mmss, true).contains("10)\\,10)"));
        let hms = timer_text(&up, TimerFormat::Hms, false);
        assert_eq!(hms.matches("\\:").count(), 11, "{hms}");
        let secs = timer_text(&up, TimerFormat::Seconds, false);
        assert!(
            secs.contains("floor(") && secs.contains("%{eif\\:"),
            "{secs}"
        );
        let down_text = timer_text(&down, TimerFormat::Mmss, false);
        assert!(down_text.contains("max("), "{down_text}");
        assert!(Cli::try_parse_from(["aditor", "timer", "in.mp4"]).is_ok());
        assert!(Cli::try_parse_from([
            "aditor",
            "timer",
            "in.mp4",
            "--mode",
            "countdown",
            "--tenths",
            "--no-box"
        ])
        .is_ok());
        // The filtergraph splitter only honors escapes inside quotes.
        let plain = sample_timer_args(true);
        let filter = timer_filter(
            &timer_text(&up, TimerFormat::Mmss, false),
            "gte(t\\,0.000)*lt(t\\,2.000)",
            &plain,
        );
        assert!(filter.starts_with("drawtext=text='%{eif"), "{filter}");
        assert!(filter.contains(":expansion=normal:"), "{filter}");
        assert!(
            filter.contains(":shadowcolor=black@0.7:shadowx=2:shadowy=2"),
            "{filter}"
        );
        assert!(!filter.contains(":box=1"), "{filter}");
        let boxed = timer_filter(
            &timer_text(&up, TimerFormat::Seconds, true),
            "gte(t\\,0.000)*lt(t\\,2.000)",
            &sample_timer_args(false),
        );
        assert!(
            boxed.contains(":box=1:boxcolor=black@0.6:boxborderw=12"),
            "{boxed}"
        );
    }
    #[test]
    fn overlay_options_scale_and_reject_bad_values() {
        assert_eq!(
            overlay_scale_filter(None, None),
            "scale=ceil(iw/2)*2:ceil(ih/2)*2"
        );
        assert_eq!(overlay_scale_filter(Some(100), None), "scale=100:-2");
        assert_eq!(overlay_scale_filter(None, Some(50)), "scale=-2:50");
        assert_eq!(overlay_scale_filter(Some(101), Some(51)), "scale=102:52");
        assert!(is_still_image(Path::new("drawing.PNG")));
        assert!(!is_still_image(Path::new("clip.mp4")));
        assert!(Cli::try_parse_from(["aditor", "overlay", "in.mp4", "img.png"]).is_ok());
        assert!(Cli::try_parse_from([
            "aditor",
            "overlay",
            "in.mp4",
            "img.png",
            "--opacity",
            "0.5",
            "--from",
            "1"
        ])
        .is_ok());
        assert!(Cli::try_parse_from(["aditor", "overlay", "in.mp4"]).is_err());
        assert!(Cli::try_parse_from([
            "aditor",
            "overlay",
            "in.mp4",
            "img.png",
            "--to",
            "1",
            "--duration",
            "1"
        ])
        .is_err());
    }
    #[test]
    fn frame_rates_and_dimensions_are_sanitized() {
        assert_eq!(parse_rate("30000/1001"), Some(30000.0 / 1001.0));
        assert_eq!(parse_rate("0/0"), None);
        assert_eq!(parse_rate("bogus"), None);
        assert_eq!(even_dim(160), 160);
        assert_eq!(even_dim(161), 162);
        assert_eq!(fmt_sec(2.5), "2.500");
    }
}
