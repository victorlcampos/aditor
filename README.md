<p align="center">
  <img src="assets/aditor-logo.png" alt="Aditor — Agentic Editor" width="900">
</p>

<h1 align="center">Da ação do seu agente ao vídeo pronto.</h1>

<p align="center">
  <strong>Capture abas. Grave demonstrações. Corte, acelere e entregue.</strong><br>
  Uma CLI em Rust que coloca captura de tela e edição de vídeo no fluxo da sua automação.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/licen%C3%A7a-MIT-orange" alt="Licença MIT"></a>
  <img src="https://img.shields.io/badge/feito_em-Rust-orange" alt="Feito em Rust">
  <img src="https://img.shields.io/badge/interface-CLI_%2B_JSON-222222" alt="CLI e JSON">
</p>

<p align="center">
  <a href="#comece-aqui">Comece aqui</a> ·
  <a href="#abas-do-navegador">Capture uma aba</a> ·
  <a href="#edição">Edite vídeos</a> ·
  <a href="#contrato-para-agentes">Integre ao seu agente</a>
</p>

## Seu agente já executa. Agora ele também mostra.

Uma tarefa concluída fica muito mais fácil de entender quando vem acompanhada de um print, uma demonstração ou um trecho de vídeo. O **Aditor** transforma comandos de terminal nesses arquivos: do registro de uma aba específica ao corte final de uma gravação.

Feito para agentes que executam comandos, scripts e desenvolvedores que querem **automatizar a captura e a edição** com IDs explícitos, saída JSON e controle pelo terminal.

| O que você quer entregar | Como o Aditor ajuda |
| --- | --- |
| Uma demo do seu produto | Grave só a aba escolhida, sem as barras do navegador. |
| Evidência visual de uma tarefa | Salve um PNG de uma aba, monitor ou elemento CSS. |
| Um vídeo direto ao ponto | Corte, ajuste a velocidade e remova áudio em um único comando. |
| Captura durante uma automação | Inicie em background e finalize pelo ID da sessão. |
| Uma integração previsível | Use `--json`, `--dry-run` e códigos de saída para controlar o fluxo. |

```sh
# Com o navegador configurado para CDP (instruções abaixo):
aditor tabs --json

# Substitua ABC123 pelo ID retornado.
aditor record --tab ABC123 --duration 15 -o demo.mp4 --json

# Transforme a captura em uma demo mais curta.
aditor edit demo.mp4 --from 2 --duration 10 --speed 2 --mute -o demo-final.mp4 --json
```

## Comece aqui

Requer Rust e Cargo para compilar. Instale a partir do código:

```sh
git clone https://github.com/victorlcampos/aditor.git
cd aditor
cargo install --path . --locked
aditor --help
```

Ou compile sem instalar:

```sh
cargo build --release --locked
./target/release/aditor --help
```

O projeto está na versão **0.1.0**. Captura de abas usa Chrome, Chromium ou Edge com CDP habilitado; os requisitos de captura de tela variam por sistema, conforme abaixo.

O FFmpeg é resolvido automaticamente (embutido, sidecar, PATH ou download). `ADITOR_FFMPEG` e `ADITOR_FFPROBE` permitem indicar os binários. Prints de abas e a listagem de abas usam CDP diretamente, sem FFmpeg.

## Abas do navegador

Inicie Chrome, Chromium ou Edge com CDP e um perfil separado. No macOS:

```sh
"/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
  --remote-debugging-port=9222 --user-data-dir=/tmp/aditor-chrome
```

Abra a página nesse navegador. O Chrome exige um diretório diferente do perfil padrão para habilitar depuração remota; veja a [documentação do Chrome](https://developer.chrome.com/blog/remote-debugging-port). `aditor tabs --help` também mostra a configuração em Linux e Windows.

```sh
aditor tabs --json
# [{"id":"ABC123", "title":"Minha página", "url":"https://..."}]

aditor screenshot --tab ABC123 -o aba.png --json
aditor record --tab ABC123 --duration 10 -o aba.mp4 --json

# Sem duração: devolve um ID e continua em background.
aditor record --tab ABC123 -o demonstracao.mp4 --json
aditor stop rec-... --json
```

Use o ID exato retornado por `tabs`. Para outra porta, passe `--cdp-port 9333` em `tabs`, `record --tab` e `screenshot --tab`. A conexão é com `127.0.0.1`; não há seleção por título ambíguo nem mudança de foco.

A captura usa [Page.captureScreenshot do CDP](https://chromedevtools.github.io/devtools-protocol/tot/Page/#method-captureScreenshot): somente a área visível da página, sem barras do navegador ou outras abas. Funciona com outra aba em primeiro plano. Não captura a página inteira rolável e não inclui áudio; `--tab --audio` é rejeitado. Safari e Firefox não são suportados por este backend. Se o navegador renderizar mais lentamente que `--fps`, frames são repetidos para preservar a duração do vídeo.

## Capturar um elemento por CSS selector

Use `--selector` junto com o ID da aba:

```sh
aditor screenshot --tab ABC123 --selector '#player' -o player.png --json
aditor print --tab ABC123 --selector '[data-testid="chart"]' -o grafico.png --json
aditor record --tab ABC123 --selector '.preview > canvas' --duration 10 -o canvas.mp4 --json

# Também funciona em background, finalizando com stop.
aditor record --tab ABC123 --selector '#player' -o player.mp4 --json
aditor stop rec-... --json
```

O seletor deve encontrar **exatamente um elemento renderizado** no documento principal. Seletores inválidos, vazios, ausentes, ambíguos ou elementos ocultos retornam erro; não há fallback para capturar a aba inteira. Não atravessa iframes nem shadow DOM. Use aspas no shell para preservar espaços e caracteres do seletor.

A captura recorta o retângulo do elemento, incluindo sua borda, mesmo fora do viewport. Não rola a página nem muda o foco. Trata-se de um recorte da página renderizada: sobreposições e recortes de ancestrais continuam aparecendo; não extrai uma camada isolada do DOM.

Durante o vídeo, o seletor e o retângulo são reavaliados a cada frame. Para recorte preciso, prefira um contêiner de posição fixa com conteúdo animado: movimentos rápidos do próprio contêiner entre a medição e a captura podem incluir bordas do fundo. Mantenha as dimensões do elemento fixas: se ele mudar de tamanho, desaparecer ou ficar oculto, a gravação termina com erro e finaliza o trecho já capturado. Vídeos podem receber até um pixel de preenchimento para manter dimensões pares exigidas pelo codec. `--dry-run --json` inclui o seletor no plano sem conectar ao navegador.

### Capture só o elemento que importa

Use um seletor CSS para isolar um player, gráfico ou componente:

```sh
aditor screenshot --tab ABC123 --selector '#player' -o player.png --json
aditor record --tab ABC123 --selector '#player' --duration 10 -o player.mp4 --json
```

O seletor deve corresponder a um único elemento visível no documento principal; não atravessa iframes ou shadow DOM. A gravação acompanha a posição do elemento, mas exige tamanho constante. Se ele desaparecer, ficar oculto ou mudar de tamanho, a captura termina com erro.

## Monitores e prints

No macOS, liste os índices de captura antes de escolher o monitor:

```sh
aditor screens --json
# [{"screen":0,"name":"Capture screen 0"},{"screen":1,"name":"Capture screen 1"}]

aditor record --screen 1 --duration 10 -o monitor.mp4 --json
aditor screenshot --screen 1 -o monitor.png --json
aditor print --screen 0 --json  # alias de screenshot
```

`screen` é o índice do monitor, não o índice de câmera do AVFoundation. Sem `--screen` nem `--tab`, o macOS captura o monitor 0. O terminal precisa ter permissão de Gravação de Tela no macOS. `--audio` inclui o microfone na gravação nativa; `--audio-device` escolhe o dispositivo.

`--screen`, `--tab` e `--video-device` são mutuamente exclusivos. Em Linux, a captura nativa usa X11 (`--video-device :0.0`, padrão `$DISPLAY`); em Windows, usa GDI (`--video-device desktop` ou `--video-device 'title=Nome da janela'`). A enumeração e seleção de monitores por `--screen` estão disponíveis no macOS. Nas outras plataformas, essa opção retorna erro em vez de ignorar o monitor solicitado.

## Contrato para agentes

- `--json`: sucesso em JSON no stdout; logs e erros no stderr. Falhas têm código de saída diferente de zero.
- `--dry-run`: mostra o plano/comando, sem capturar nem criar diretórios de saída. Com `--tab`, não conecta ao navegador. O plano de vídeo mostra que `pipe:0` recebe frames CDP; o comando FFmpeg sozinho não produz essa entrada.
- `record --duration`: aguarda o término e retorna o arquivo finalizado.
- `record` sem duração: retorna `id`, `pid`, `output` e o comando `stop`. Vale para abas e captura nativa.
- `stop ID`: finaliza o MP4 antes de retornar; sem ID, exige uma única sessão. `stop --all` encerra as sessões.
- `screenshot`/`print`: salva um único PNG e termina, sem sessão em background.
- `-o`: caminho do arquivo; sem ele, vídeos vão para `~/Documents/Videos` e prints para `~/Documents/Pictures`. `--dir` troca o diretório. Capturas retornam caminhos absolutos.
- Arquivos existentes são recusados; `--yes` autoriza sobrescrever. Prints exigem extensão `.png`.

## Edição

```sh
aditor info entrada.mp4 --json
aditor speed entrada.mp4 -x 1.5 -o rapido.mp4 --json
aditor cut entrada.mp4 --from 00:01:30 --duration 10 -o trecho.mp4
aditor edit entrada.mp4 --from 5 --duration 20 --speed 2 --mute -o editado.mp4
aditor convert entrada.mp4 --codec hevc -o menor.mp4
aditor doctor --json
```

## Validação

```sh
cargo test
cargo clippy --all-targets -- -D warnings
python3 tests/browser_cli.py \
  --browser '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' \
  --aditor target/debug/aditor
```

O teste de integração inicia um Chrome headless com perfil temporário e verifica seleção de aba, recorte por CSS selector (dimensões e conteúdo), PNG, sobrescrita, duração de vídeo, background/stop, erros e dry-run. Não usa o perfil pessoal do navegador. Requer Chrome/Chromium, FFmpeg e ffprobe no PATH.


## Contribua

Tem um fluxo de captura ou edição que gostaria de automatizar? [Abra uma issue](https://github.com/victorlcampos/aditor/issues) com o cenário, o sistema operacional e o resultado esperado. Correções e melhorias via pull request são bem-vindas; execute as verificações acima antes de enviar.

Se o Aditor faz sentido para o seu próximo agente, **deixe uma estrela e experimente sua primeira captura**.

## Licença

Código distribuído sob a [licença MIT](LICENSE). FFmpeg e ffprobe são componentes de terceiros, sujeitos às respectivas licenças da distribuição utilizada.
