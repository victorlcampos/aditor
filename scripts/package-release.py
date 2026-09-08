"""Smoke-test and package a native release binary for GitHub Releases."""

import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import zipfile


def main():
    target = os.environ["RELEASE_TARGET"]
    version = os.environ["ADITOR_RELEASE_VERSION"]
    windows = target.endswith("windows-msvc")
    executable = "aditor.exe" if windows else "aditor"
    binary = Path("target") / target / "release" / executable
    actual = subprocess.check_output([str(binary), "--version"], text=True).strip()
    if actual != f"aditor {version}":
        raise RuntimeError(f"Unexpected CLI version: {actual!r}")
    subprocess.run([str(binary), "--help"], check=True, stdout=subprocess.DEVNULL)

    dist = Path("dist")
    dist.mkdir(exist_ok=True)
    # Keep asset names stable so /releases/latest/download/<asset> always works.
    name = f"aditor-{target}"
    archive = dist / (name + (".zip" if windows else ".tar.gz"))
    with tempfile.TemporaryDirectory() as temporary:
        package = Path(temporary) / name
        package.mkdir()
        for source in [binary, Path("README.md"), Path("LICENSE")]:
            shutil.copy2(source, package / source.name)
        if windows:
            with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as output:
                for source in sorted(package.iterdir()):
                    output.write(source, f"{name}/{source.name}")
        else:
            with tarfile.open(archive, "w:gz") as output:
                output.add(package, arcname=name)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    (dist / f"{archive.name}.sha256").write_text(
        f"{digest}  {archive.name}\n", encoding="utf-8"
    )
    (dist / f"version-{target}.txt").write_text(f"{version}\n", encoding="utf-8")


if __name__ == "__main__":
    main()
