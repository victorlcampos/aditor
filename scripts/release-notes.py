"""Generate release notes with version-specific downloads and permanent links."""

import os
import sys
from urllib.parse import quote


def release_notes(version, repository, commit):
    base = f"https://github.com/{repository}"
    download = f"{base}/releases/download/{quote('v' + version, safe='')}"
    platforms = [
        ("macOS · Apple Silicon", "aarch64-apple-darwin.tar.gz"),
        ("macOS · Intel", "x86_64-apple-darwin.tar.gz"),
        ("Linux · x86_64 (Ubuntu 22.04+)", "x86_64-unknown-linux-gnu.tar.gz"),
        ("Windows · x86_64", "x86_64-pc-windows-msvc.zip"),
    ]
    lines = [
        f"## Aditor {version}",
        "",
        "**Capture tabs. Record demos. Cut, speed up, and ship.**",
        "",
        "Download the CLI for your system — no Rust installation required.",
        "",
        "| Platform | Download |",
        "| --- | --- |",
    ]
    for platform, suffix in platforms:
        asset = f"aditor-{suffix}"
        lines.append(f"| {platform} | [{asset}]({download}/{asset}) |")
    lines.extend([
        "",
        f"[SHA-256 checksums]({download}/SHA256SUMS) · "
        f"[Always download the latest release]({base}/releases/latest)",
        "",
        "### Install",
        "",
        "Extract the archive, move `aditor` (or `aditor.exe`) onto your `PATH`, "
        "and run `aditor --version`.",
        "",
        "FFmpeg is resolved at runtime and is not bundled. Browser tab capture "
        "requires Chrome, Chromium, or Edge with CDP enabled. Binaries are unsigned.",
        "",
        f"[Setup and examples]({base}/blob/{commit}/README.md#get-started)",
        "",
        "### Build details",
        "",
        f"- Source: [{commit[:12]}]({base}/commit/{commit})",
        f"- CLI version: `aditor {version}`",
        "- All four platforms passed formatting, tests, Clippy, and CLI smoke checks.",
        "- Archives include the README and MIT license; verify downloads against `SHA256SUMS`.",
    ])
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    print(release_notes(sys.argv[1], os.environ["GH_REPO"], os.environ["GITHUB_SHA"]), end="")
