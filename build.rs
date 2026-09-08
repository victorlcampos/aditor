//! build.rs — validate `embed-ffmpeg` mode.
//!
//! The default binary does NOT download anything at build time (offline builds supported).
//! To build a fully self-contained binary (ffmpeg+ffprobe inside the
//! executable, works offline without installing either):
//!
//!   aditor fetch-ffmpeg            # download a static build to ./third_party
//!   ADITOR_FFMPEG_BIN=$PWD/third_party/ffmpeg \
//!   ADITOR_FFPROBE_BIN=$PWD/third_party/ffprobe \
//!     cargo build --release --features embed-ffmpeg

fn main() {
    println!("cargo:rerun-if-env-changed=ADITOR_RELEASE_VERSION");
    let version = std::env::var("ADITOR_RELEASE_VERSION")
        .unwrap_or_else(|_| std::env::var("CARGO_PKG_VERSION").unwrap());
    println!("cargo:rustc-env=ADITOR_VERSION={version}");
    println!("cargo:rerun-if-env-changed=ADITOR_FFMPEG_BIN");
    println!("cargo:rerun-if-env-changed=ADITOR_FFPROBE_BIN");

    #[cfg(feature = "embed-ffmpeg")]
    {
        for var in ["ADITOR_FFMPEG_BIN", "ADITOR_FFPROBE_BIN"] {
            let path = std::env::var(var).unwrap_or_else(|_| {
                panic!(
                    "the embed-ffmpeg feature requires {var} to point to the static binary. \
                     Run `aditor fetch-ffmpeg` and export the variable (see build.rs)."
                )
            });
            if !std::path::Path::new(&path).exists() {
                panic!("{var}={path} does not exist");
            }
        }
    }
}
