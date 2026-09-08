//! build.rs — valida o modo `embed-ffmpeg`.
//!
//! O binário padrão NÃO baixa nada em build-time (build offline OK).
//! Para gerar um binário 100% self-contained (ffmpeg+ffprobe dentro do
//! executável, funciona offline sem nada instalado):
//!
//!   aditor fetch-ffmpeg            # baixa build estática p/ ./third_party
//!   ADITOR_FFMPEG_BIN=$PWD/third_party/ffmpeg \
//!   ADITOR_FFPROBE_BIN=$PWD/third_party/ffprobe \
//!     cargo build --release --features embed-ffmpeg

fn main() {
    println!("cargo:rerun-if-env-changed=ADITOR_FFMPEG_BIN");
    println!("cargo:rerun-if-env-changed=ADITOR_FFPROBE_BIN");

    #[cfg(feature = "embed-ffmpeg")]
    {
        for var in ["ADITOR_FFMPEG_BIN", "ADITOR_FFPROBE_BIN"] {
            let path = std::env::var(var).unwrap_or_else(|_| {
                panic!(
                    "feature embed-ffmpeg exige {var} apontando para o binário estático. \
                     Rode `aditor fetch-ffmpeg` e exporte a variável (veja build.rs)."
                )
            });
            if !std::path::Path::new(&path).exists() {
                panic!("{var}={path} não existe");
            }
        }
    }
}
