#!/usr/bin/env python3
"""Package only explicit public release assets; requires Python 3.11+."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import tarfile
import tomllib

ROOT = Path(__file__).resolve().parents[1]
TARGETS = {
    f"{cpu}-unknown-linux-{libc}"
    for cpu in ("x86_64", "aarch64") for libc in ("musl", "gnu")
} | {"x86_64-apple-darwin", "aarch64-apple-darwin"}


def notices(metadata):
    """Walk normal/build edges only; read public crate license files only."""
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    root = metadata["resolve"]["root"]
    if packages[root]["name"] != "monkeytype-tui":
        raise ValueError("metadata must describe monkeytype-tui")
    selected = set()
    pending = [root]
    while pending:
        current = pending.pop()
        if current in selected:
            continue
        selected.add(current)
        for dep in nodes[current]["deps"]:
            if any(k["kind"] in (None, "build") for k in dep["dep_kinds"]):
                pending.append(dep["pkg"])
    parts = ["Third-party dependency notices for monkeytype-tui.\n"
             "Includes normal/build dependencies for the selected target.\n"
             "This collection is not an advisory or license compatibility audit.\n"]
    for package in sorted((packages[k] for k in selected - {root}),
                          key=lambda p: (p["name"], p["version"])):
        if not (package.get("source") or "").startswith("registry+"):
            raise ValueError("release dependencies must come from a registry")
        directory = Path(package["manifest_path"]).parent.resolve()
        files = set()
        # Fixed license filename patterns; never recursively scan crate contents.
        for pattern in ("LICENSE*", "LICENCE*", "COPYING*", "NOTICE*", "COPYRIGHT*"):
            files.update(p for p in directory.glob(pattern) if p.is_file())
        if package.get("license_file"):
            files.add(directory / package["license_file"])
        if not files:
            raise ValueError(f"No license text found for {package['name']}")
        parts.append(f"\n{'=' * 72}\n{package['name']} {package['version']}\n"
                     f"License expression: {package.get('license') or 'see text'}\n")
        for path in sorted(files):
            resolved = path.resolve()
            if not resolved.is_relative_to(directory):
                raise ValueError("license path escapes the registry package")
            parts.append(f"\n--- {path.name} ---\n{resolved.read_text()}\n")
    return "".join(parts).encode()


def build_package(binary, target, metadata, output, tag=None):
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version):
        raise ValueError("release packaging currently requires a stable x.y.z version")
    if tag is not None and tag != f"v{version}":
        raise ValueError(f"release tag must be v{version}")
    if target not in TARGETS:
        raise ValueError("unsupported release target")
    name = f"monkeytype-tui-{version}-{target}"
    assets = {
        "monkeytype-tui": (binary.read_bytes(), 0o755),
        "install.sh": ((ROOT / "packaging/install.sh").read_bytes(), 0o755),
        "README.md": ((ROOT / "README.md").read_bytes(), 0o644),
        "release-target.txt": ((target + "\n").encode(), 0o644),
        "THIRD-PARTY-NOTICES.txt": (notices(metadata), 0o644),
    }
    license_file = ROOT / "LICENSE"
    if license_file.is_file():
        assets["LICENSE"] = (license_file.read_bytes(), 0o644)
    output.mkdir(parents=True, exist_ok=True)
    archive = output / f"{name}.tar.gz"
    with tarfile.open(archive, "w:gz") as tar:
        for filename, (data, mode) in assets.items():
            entry = tarfile.TarInfo(f"{name}/{filename}")
            entry.size, entry.mode, entry.mtime = len(data), mode, 0
            tar.addfile(entry, io.BytesIO(data))
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    archive.with_suffix(archive.suffix + ".sha256").write_text(f"{digest}  {archive.name}\n")
    return archive


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--target", choices=sorted(TARGETS), required=True)
    parser.add_argument("--metadata", type=Path, required=True,
                        help="cargo metadata --locked --format-version 1 --filter-platform TARGET")
    parser.add_argument("--out", type=Path, default=ROOT / "target/packages")
    parser.add_argument("--tag", help="when releasing, require this to match the Cargo version")
    args = parser.parse_args()
    archive = build_package(args.binary, args.target, json.loads(args.metadata.read_text()),
                            args.out, args.tag)
    print(archive)


if __name__ == "__main__":
    main()
