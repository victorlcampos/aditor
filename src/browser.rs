//! Captura CDP local: seleciona o target pelo ID, sem foco nem seletor visual.
use super::*;
use base64::Engine;
use serde_json::{json, Value};
use std::io::Write;
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};
use tungstenite::{Message, WebSocket};

#[derive(Args, Debug)]
#[command(
    after_help = "CONFIGURAÇÃO (Chrome/Chromium/Edge):\n  Inicie o browser com --remote-debugging-port=9222 e um perfil separado.\n  macOS:\n    \"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome\" \\\n      --remote-debugging-port=9222 --user-data-dir=/tmp/aditor-chrome\n  Linux:\n    google-chrome --remote-debugging-port=9222 --user-data-dir=/tmp/aditor-chrome\n  Windows (PowerShell):\n    & \"$env:ProgramFiles/Google/Chrome/Application/chrome.exe\" --remote-debugging-port=9222 --user-data-dir=\"$env:TEMP/aditor-chrome\"\n\nAbra a página nesse browser e use o ID exato de tabs --json:\n  aditor tabs --cdp-port 9222 --json\n  aditor record --tab <ID> --duration 10 --json\n  aditor screenshot --tab <ID> -o aba.png --json\n\nA conexão é local (127.0.0.1); não abre nem troca abas. Chrome exige perfil\nseparado do perfil padrão para CDP. Safari/Firefox não são suportados.\nA captura inclui só o viewport, sem barras do navegador e sem áudio.\nNão requer que a aba esteja em primeiro plano; o browser precisa estar em execução."
)]
pub struct TabsArgs {
    /// Porta CDP local do navegador
    #[arg(long, default_value_t = 9222, value_parser = clap::value_parser!(u16).range(1..))]
    pub cdp_port: u16,
    /// Array JSON com id, title e url de cada aba
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Deserialize, Serialize)]
struct Tab {
    id: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(rename = "type", skip_serializing)]
    kind: String,
    #[serde(rename = "webSocketDebuggerUrl", default, skip_serializing)]
    websocket: Option<String>,
}

fn list_tabs(port: u16) -> Result<Vec<Tab>> {
    let config = ureq::Agent::config_builder()
        .proxy(None)
        .timeout_global(Some(Duration::from_secs(5)))
        .build();
    let agent: ureq::Agent = config.into();
    let raw = agent.get(format!("http://127.0.0.1:{port}/json/list"))
        .call()
        .with_context(|| format!("não foi possível conectar ao CDP em 127.0.0.1:{port}; inicie Chrome/Chromium/Edge com --remote-debugging-port={port} e --user-data-dir de um perfil separado (aditor tabs --help)"))?
        .body_mut().read_to_string()?;
    let tabs: Vec<Tab> =
        serde_json::from_str(&raw).context("resposta inválida de /json/list do CDP")?;
    Ok(tabs.into_iter().filter(|t| t.kind == "page").collect())
}

pub fn tabs(a: TabsArgs) -> Result<()> {
    let tabs = list_tabs(a.cdp_port)?;
    if a.json {
        println!("{}", serde_json::to_string_pretty(&tabs)?);
    } else if tabs.is_empty() {
        println!("nenhuma aba disponível; abra uma página no browser com CDP");
    } else {
        for tab in tabs {
            println!("{}\t{}\t{}", tab.id, tab.title, tab.url);
        }
    }
    Ok(())
}

struct Cdp {
    socket: WebSocket<TcpStream>,
    next_id: u64,
    element_size: Option<(f64, f64)>,
}

impl Cdp {
    fn connect(port: u16, tab_id: &str) -> Result<Self> {
        let tab = list_tabs(port)?.into_iter().find(|t| t.id == tab_id)
            .with_context(|| format!("aba `{tab_id}` não encontrada; liste IDs com aditor tabs --cdp-port {port} --json"))?;
        let url = tab
            .websocket
            .context("aba sem webSocketDebuggerUrl; não permite captura via CDP")?;
        // Mesmo que /json/list contenha outro host, nunca conectamos fora do loopback.
        let address = SocketAddr::from(([127, 0, 0, 1], port));
        let stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
        stream.set_read_timeout(Some(Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(Duration::from_secs(10)))?;
        let (socket, _) =
            tungstenite::client(url.as_str(), stream).context("conectar ao WebSocket da aba")?;
        Ok(Self {
            socket,
            next_id: 0,
            element_size: None,
        })
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let id = self.next_id;
        self.socket.send(Message::Text(
            json!({"id": id, "method": method, "params": params})
                .to_string()
                .into(),
        ))?;
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if Instant::now() > deadline {
                bail!("CDP não respondeu a {method} em 15s");
            }
            match self
                .socket
                .read()
                .with_context(|| format!("ler {method}; a aba pode ter sido fechada"))?
            {
                Message::Text(text) => {
                    let value: Value = serde_json::from_str(&text)?;
                    if value["id"].as_u64() == Some(id) {
                        if let Some(error) = value.get("error") {
                            bail!("CDP {method}: {error}");
                        }
                        return value
                            .get("result")
                            .cloned()
                            .context("CDP retornou resposta sem result");
                    }
                }
                Message::Close(_) => bail!("conexão CDP encerrada; a aba pode ter sido fechada"),
                Message::Ping(_) => self.socket.flush()?,
                _ => {}
            }
        }
    }

    fn element_clip(&mut self, selector: &str) -> Result<Value> {
        // JSON encoding keeps quotes, backslashes and JS-looking selectors as data.
        let expression = format!(
            "({})({})",
            include_str!("element_bounds.js"),
            serde_json::to_string(selector)?
        );
        let result = self.call(
            "Runtime.evaluate",
            json!({"expression": expression, "returnByValue": true}),
        )?;
        if let Some(error) = result.get("exceptionDetails") {
            bail!("não foi possível localizar --selector {selector:?}: {error}");
        }
        let clip = &result["result"]["value"];
        if let Some(error) = clip["error"].as_str() {
            bail!("--selector {selector:?}: {error}");
        }
        for key in ["x", "y", "width", "height"] {
            let value = clip[key]
                .as_f64()
                .with_context(|| format!("limites inválidos para --selector {selector:?}"))?;
            if !value.is_finite() || (matches!(key, "width" | "height") && value <= 0.0) {
                bail!("elemento sem área capturável: {selector:?}");
            }
        }
        Ok(clip.clone())
    }

    fn frame(&mut self, format: &str, selector: Option<&str>) -> Result<Vec<u8>> {
        let mut params =
            json!({"format": format, "fromSurface": true, "captureBeyondViewport": false});
        if let Some(selector) = selector {
            let clip = self.element_clip(selector)?;
            if format == "jpeg" {
                let size = (
                    clip["width"].as_f64().unwrap(),
                    clip["height"].as_f64().unwrap(),
                );
                if self.element_size.is_some_and(|initial| initial != size) {
                    bail!("o elemento {selector:?} mudou de tamanho durante a gravação; mantenha dimensões fixas e reinicie a captura");
                }
                self.element_size = Some(size);
            }
            params["clip"] = clip;
            params["captureBeyondViewport"] = json!(true);
        }
        if format == "jpeg" {
            params["quality"] = json!(90);
        }
        let result = self.call("Page.captureScreenshot", params)?;
        let data = result["data"]
            .as_str()
            .context("CDP não retornou a imagem da aba")?;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data)
            .context("imagem CDP em base64 inválida")?;
        if bytes.is_empty() {
            bail!("CDP retornou imagem vazia");
        }
        Ok(bytes)
    }
}

pub fn screenshot(a: &capture::ScreenshotArgs, output: &Path) -> Result<()> {
    let tab = a.source.tab.as_deref().context("--tab ausente")?;
    if !a.out.dry_run {
        let bytes =
            Cdp::connect(a.source.cdp_port, tab)?.frame("png", a.source.selector.as_deref())?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .create_new(!a.out.yes)
            .truncate(a.out.yes)
            .open(output)?;
        file.write_all(&bytes)?;
    }
    let result = json!({"output": output, "format": "png", "tab": tab, "selector": a.source.selector, "cdp_port": a.source.cdp_port,
        "method": "Page.captureScreenshot", "dry_run": a.out.dry_run,
        "size_bytes": std::fs::metadata(output).ok().map(|m| m.len())});
    if a.out.json || a.out.dry_run {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("ok → {}", output.display());
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct WorkerConfig {
    port: u16,
    tab: String,
    #[serde(default)]
    selector: Option<String>,
    ffmpeg: PathBuf,
    args: Vec<String>,
    fps: u32,
    duration: Option<f64>,
}

pub fn record(
    a: &RecordArgs,
    ffmpeg: &Path,
    args: &[String],
    output: &Path,
    duration: Option<f64>,
    codec: &str,
) -> Result<()> {
    let tab = a.source.tab.as_deref().context("--tab ausente")?;
    let cmd = shell_quote(ffmpeg, args);
    if a.out.dry_run {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "output": output, "tab": tab, "selector": a.source.selector, "cdp_port": a.source.cdp_port, "fps": a.fps,
                "duration": duration, "background": duration.is_none(), "audio": false,
                "method": "Page.captureScreenshot", "ffmpeg_cmd": cmd,
                "input": "JPEG frames da aba via CDP em pipe:0", "dry_run": true
            }))?
        );
        return Ok(());
    }
    let config = WorkerConfig {
        port: a.source.cdp_port,
        tab: tab.into(),
        selector: a.source.selector.clone(),
        ffmpeg: ffmpeg.into(),
        args: args.into(),
        fps: a.fps,
        duration,
    };
    if duration.is_some() {
        run_worker(&config, None)?;
        if a.out.json {
            let (duration, size) = probe_output(output);
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "output": output, "tab": tab, "selector": a.source.selector, "duration": duration, "size_bytes": size,
                    "fps": a.fps, "audio": false, "video_codec": codec, "dry_run": false
                }))?
            );
        } else {
            println!("ok → {}", output.display());
        }
        return Ok(());
    }

    let sessions = std::fs::canonicalize(rec_sessions_dir()?)?;
    let id = rec_id(&sessions);
    let config_path = sessions.join(format!("{id}.worker"));
    let log_path = sessions.join(format!("{id}.log"));
    std::fs::write(&config_path, serde_json::to_vec(&config)?)?;
    let log = std::fs::File::create(&log_path)?;
    let mut child = Command::new(std::env::current_exe()?)
        .arg("__record-tab")
        .arg(&config_path)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        if let Some(status) = child.try_wait()? {
            let tail = tail_file(&log_path, 20);
            cleanup_worker(&config_path);
            bail!("captura da aba falhou ao iniciar ({status}):\n{tail}");
        }
        if config_path.with_extension("ready").exists() {
            break;
        }
        if Instant::now() > deadline {
            let _ = std::fs::write(config_path.with_extension("stop"), b"");
            let _ = child.kill();
            let _ = child.wait();
            cleanup_worker(&config_path);
            bail!(
                "captura da aba não iniciou em 30s; veja {}",
                log_path.display()
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let sess = RecSession {
        id: id.clone(),
        pid: child.id(),
        output: output.to_string_lossy().into_owned(),
        log: log_path.to_string_lossy().into_owned(),
        fps: a.fps,
        audio: false,
        video_codec: codec.into(),
        ffmpeg_cmd: cmd,
        started_epoch: now_epoch(),
        worker_config: Some(config_path.clone()),
    };
    let session_path = sessions.join(format!("{id}.json"));
    if let Err(error) = std::fs::write(&session_path, serde_json::to_vec_pretty(&sess)?) {
        let _ = stop_worker(&config_path, child.id());
        return Err(error.into());
    }
    if a.out.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({"id": id, "pid": sess.pid,
            "output": output, "tab": tab, "selector": a.source.selector, "fps": a.fps, "audio": false,
            "log": log_path, "stop": format!("aditor stop {id}"), "background": true}))?
        );
    } else {
        println!(
            "gravando aba {tab} em background — id {id}\nsaída: {}\npare com: aditor stop {id}",
            output.display()
        );
    }
    Ok(())
}

// Fecha stdin antes de wait: FFmpeg recebe EOF e finaliza o MP4 sem sinais.
fn run_worker(config: &WorkerConfig, state: Option<&Path>) -> Result<()> {
    let mut cdp = Cdp::connect(config.port, &config.tab)?;
    let mut frame = cdp.frame("jpeg", config.selector.as_deref())?;
    let mut encoder = Command::new(&config.ffmpeg)
        .args(&config.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()?;
    let mut input = encoder.stdin.take().context("abrir stdin do ffmpeg")?;
    let result = (|| -> Result<()> {
        let start = Instant::now();
        let mut written = 0u64;
        let max_frames = config
            .duration
            .map(|d| (d * config.fps as f64).ceil() as u64);
        loop {
            // Se CDP demora, repete o último frame para preservar o tempo real.
            let wanted = frames_due(start.elapsed(), config.fps, max_frames);
            while written < wanted {
                input
                    .write_all(&frame)
                    .context("enviar frame para ffmpeg")?;
                written += 1;
            }
            if let Some(status) = encoder.try_wait()? {
                bail!("ffmpeg encerrou durante a captura: {status}");
            }
            if let Some(path) = state {
                if start.elapsed() >= Duration::from_secs(1)
                    && !path.with_extension("ready").exists()
                {
                    std::fs::write(path.with_extension("ready"), b"")?;
                }
                if path.with_extension("stop").exists() {
                    break;
                }
            }
            if max_frames.is_some_and(|max| written >= max) {
                break;
            }
            let next = Duration::from_secs_f64(written as f64 / config.fps as f64);
            if let Some(delay) = next.checked_sub(start.elapsed()) {
                std::thread::sleep(delay);
            }
            frame = cdp.frame("jpeg", config.selector.as_deref())?;
        }
        Ok(())
    })();
    drop(input);
    let status = encoder.wait().context("finalizar ffmpeg")?;
    result?;
    if !status.success() {
        bail!("ffmpeg falhou ao finalizar captura da aba: {status}");
    }
    Ok(())
}

fn frames_due(elapsed: Duration, fps: u32, limit: Option<u64>) -> u64 {
    let count = (elapsed.as_secs_f64() * fps as f64).floor() as u64 + 1;
    limit.map_or(count, |max| count.min(max))
}

pub fn worker_entry(path: &Path) -> Result<()> {
    let config: WorkerConfig = serde_json::from_slice(&std::fs::read(path)?)?;
    let result = run_worker(&config, Some(path));
    let done = match &result {
        Ok(()) => json!({"ok": true}),
        Err(e) => json!({"ok": false, "error": format!("{e:#}")}),
    };
    // Publica o resultado de forma atômica, somente depois de finalizar o encoder.
    let tmp = path.with_extension("done.tmp");
    std::fs::write(&tmp, serde_json::to_vec(&done)?)?;
    std::fs::rename(tmp, path.with_extension("done"))?;
    result
}

pub fn stop_worker(path: &Path, pid: u32) -> Result<()> {
    std::fs::write(path.with_extension("stop"), b"")?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let done_path = path.with_extension("done");
    while !done_path.exists() {
        // O worker pode publicar done e sair entre as duas verificações.
        if !pid_alive(pid) && !done_path.exists() {
            bail!("processo da captura terminou sem confirmar o vídeo; confira o log da sessão");
        }
        if Instant::now() > deadline {
            bail!("captura ainda está finalizando; tente aditor stop novamente. Sessão preservada");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let done: Value = serde_json::from_slice(&std::fs::read(done_path)?)?;
    if done["ok"] != true {
        bail!(
            "captura da aba falhou: {}",
            done["error"].as_str().unwrap_or("erro desconhecido")
        );
    }
    cleanup_worker(path);
    Ok(())
}

pub(super) fn cleanup_worker(path: &Path) {
    for ext in ["worker", "ready", "stop", "done", "done.tmp"] {
        let _ = std::fs::remove_file(path.with_extension(ext));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slow_captures_preserve_duration_and_respect_limit() {
        assert_eq!(frames_due(Duration::ZERO, 30, None), 1);
        assert_eq!(frames_due(Duration::from_millis(500), 30, None), 16);
        assert_eq!(frames_due(Duration::from_secs(3), 30, Some(60)), 60);
    }
    #[test]
    fn tab_json_exposes_only_agent_selection_fields() {
        let tab: Tab = serde_json::from_value(json!({"id":"ABC", "type":"page", "title":"Teste", "url":"about:blank", "webSocketDebuggerUrl":"ws://127.0.0.1:9222/devtools/page/ABC"})).unwrap();
        assert_eq!(
            serde_json::to_value(tab).unwrap(),
            json!({"id":"ABC", "title":"Teste", "url":"about:blank"})
        );
    }
}
