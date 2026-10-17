#!/usr/bin/env python3
"""Installer regressions and optional real release-archive smoke test."""
import hashlib
import importlib.util
import fcntl
import os
from pathlib import Path
import platform
import pty
import select
import sqlite3
import struct
import subprocess
import sys
import tarfile
import tempfile
import termios
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("package_release", ROOT / "scripts/package-release.py")
packager = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packager)


def host_target():
    cpu = {"arm64": "aarch64", "aarch64": "aarch64", "x86_64": "x86_64"}[platform.machine()]
    return f"{cpu}-apple-darwin" if sys.platform == "darwin" else f"{cpu}-unknown-linux-gnu"


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="monkeytype-package-test-")
        self.addCleanup(self.temp.cleanup)
        self.base = Path(self.temp.name)
        self.package = self.base / "package"
        self.package.mkdir()
        self.prefix = self.base / "prefix with spaces"
        (self.package / "install.sh").write_bytes((ROOT / "packaging/install.sh").read_bytes())
        (self.package / "monkeytype-tui").write_text("#!/bin/sh\nprintf 'test binary\\n'\n")
        (self.package / "release-target.txt").write_text(host_target() + "\n")
        (self.package / "THIRD-PARTY-NOTICES.txt").write_text("Test notices\n")
        (self.package / "README.md").write_text("Test instructions\n")

    def install(self, *args):
        # No ambient environment inspection/inheritance, and no writes to user home.
        return subprocess.run(["/bin/sh", str(self.package / "install.sh"),
                               "--prefix", str(self.prefix), *args], cwd="/",
                              env={"HOME": str(self.base), "PATH": "/usr/bin:/bin"},
                              capture_output=True, text=True)

    def test_install_outside_package_directory_and_launch(self):  # PKG01
        result = self.install()
        self.assertEqual(result.returncode, 0, result.stderr)
        binary = self.prefix / "bin/monkeytype-tui"
        result = subprocess.run([str(binary)], env={}, capture_output=True, text=True)
        self.assertEqual(result.stdout, "test binary\n")
        self.assertTrue((self.prefix / "share/doc/monkeytype-tui/THIRD-PARTY-NOTICES.txt").is_file())
        self.assertFalse((self.base / ".profile").exists())

    def test_update_requires_force_and_preserves_old_file(self):  # PKG02
        self.assertEqual(self.install().returncode, 0)
        binary = self.prefix / "bin/monkeytype-tui"
        binary.write_text("old version")
        self.assertNotEqual(self.install().returncode, 0)
        self.assertEqual(binary.read_text(), "old version")
        self.assertEqual(self.install("--force").returncode, 0)
        self.assertTrue(binary.read_text().startswith("#!/bin/sh"))

    def test_symlink_is_not_overwritten_even_with_force(self):  # PKG03
        (self.prefix / "bin").mkdir(parents=True)
        other = self.base / "unrelated"
        other.write_text("keep")
        (self.prefix / "bin/monkeytype-tui").symlink_to(other)
        self.assertNotEqual(self.install("--force").returncode, 0)
        self.assertEqual(other.read_text(), "keep")

    def test_wrong_platform_rejected_before_writing(self):  # PKG04
        (self.package / "release-target.txt").write_text("unsupported-target\n")
        self.assertNotEqual(self.install().returncode, 0)
        self.assertFalse(self.prefix.exists())

    def test_dependency_notices_exclude_dev_only_packages(self):  # PKG05
        dependency = self.base / "public-dependency"
        dependency.mkdir()
        (dependency / "LICENSE").write_text("Public test license text")
        metadata = {
            "packages": [
                {"id": "app", "name": "monkeytype-tui"},
                {"id": "dep", "name": "example", "version": "1.0.0", "source": "registry+test",
                 "manifest_path": str(dependency / "Cargo.toml"), "license": "MIT"},
                {"id": "dev", "name": "dev-only"},
            ],
            "resolve": {"root": "app", "nodes": [
                {"id": "app", "deps": [{"pkg": "dep", "dep_kinds": [{"kind": None}]},
                                          {"pkg": "dev", "dep_kinds": [{"kind": "dev"}]}]},
                {"id": "dep", "deps": []},
            ]},
        }
        text = packager.notices(metadata).decode()
        self.assertIn("Public test license text", text)
        self.assertNotIn("dev-only", text)
        self.assertNotIn(str(self.base), text)

    def test_tag_mismatch_rejected_before_packaging(self):  # PKG06
        with self.assertRaisesRegex(ValueError, "release tag"):
            packager.build_package(Path("unused"), host_target(), {}, self.base / "out", "v999.0.0")


def interactive_smoke(binary, isolated, retained=False):
    master, slave = pty.openpty()
    child = None
    try:
        baseline = termios.tcgetattr(slave)
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 30, 100, 0, 0))
        state=Path(isolated["HOME"]) / "history-state"
        child = subprocess.Popen([str(binary), "--mode", "code", "--duration", "30", "--history-dir", str(state)], stdin=slave, stdout=slave, stderr=slave,
                                 env={**isolated, "TERM": "xterm-256color", "LANG": "C.UTF-8"},
                                 start_new_session=True)
        output = bytearray()

        def wait_for(text):
            deadline = time.monotonic() + 8
            while text not in output:
                if time.monotonic() >= deadline:
                    raise TimeoutError(f"installed app did not display {text!r}")
                if select.select([master], [], [], 0.1)[0]:
                    output.extend(os.read(master, 65536))
                elif child.poll() is not None:
                    raise RuntimeError("installed app exited before expected screen")

        wait_for(b"READY")
        if retained:
            os.write(master, b"\x1bOS")
            wait_for(b"HISTORY")
            os.write(master, b"\x1bOS")
            wait_for(b"PROGRESS")
        else:
            os.write(master, b"a")
            wait_for(b"TYPING")
            # Deliberate errors still complete a snippet. The corpus test bounds
            # every bundled exercise to <=512 scored positions.
            os.write(master, b"x" * 600)
            deadline=time.monotonic()+8
            while True:
                try:
                    with sqlite3.connect(f"file:{state / 'history.sqlite3'}?mode=ro", uri=True, timeout=0.1) as db:
                        count=db.execute("SELECT count(*) FROM sessions").fetchone()[0]
                    if count==1:
                        break
                except sqlite3.OperationalError:
                    pass
                if time.monotonic()>=deadline:
                    raise TimeoutError("installed executable did not persist a completed test")
                if select.select([master], [], [], 0.05)[0]:
                    output.extend(os.read(master,65536))
        os.write(master, b"\x03")
        if child.wait(timeout=5) != 0:
            raise RuntimeError("installed app did not quit successfully")
        if termios.tcgetattr(slave) != baseline:
            raise RuntimeError("installed app did not restore terminal settings")
    finally:
        if child is not None and child.poll() is None:
            child.kill()
            child.wait()
        os.close(master)
        os.close(slave)


def smoke_archive(archive):  # PKG07
    expected, filename = archive.with_suffix(archive.suffix + ".sha256").read_text().split()
    if filename != archive.name or hashlib.sha256(archive.read_bytes()).hexdigest() != expected:
        raise ValueError("archive checksum mismatch")
    with tempfile.TemporaryDirectory(prefix="monkeytype-release-smoke-") as temporary:
        base = Path(temporary)
        with tarfile.open(archive) as tar:
            # Explicit extraction: only the six known files, no links/directories/traversal.
            names = set()
            package = base / "package"
            package.mkdir()
            for member in tar.getmembers():
                path = Path(member.name)
                allowed = {"monkeytype-tui", "install.sh", "README.md", "release-target.txt",
                           "THIRD-PARTY-NOTICES.txt", "LICENSE"}
                if len(path.parts) != 2 or path.parts[0] != archive.name.removesuffix(".tar.gz"):
                    raise ValueError("unexpected archive path")
                if path.name not in allowed or not member.isfile() or path.name in names:
                    raise ValueError("unexpected archive member")
                names.add(path.name)
                (package / path.name).write_bytes(tar.extractfile(member).read())
            if names - {"LICENSE"} != allowed - {"LICENSE"}:
                raise ValueError("missing archive assets")
        prefix = base / "installed"
        isolated = {"HOME": str(base), "PATH": "/usr/bin:/bin"}
        subprocess.run(["/bin/sh", str(package / "install.sh"), "--prefix", str(prefix)],
                       env=isolated, check=True, capture_output=True)
        for flag in ("--version", "--help"):
            result = subprocess.run([str(prefix / "bin/monkeytype-tui"), flag],
                                    env=isolated, check=True, capture_output=True, text=True)
            if "monkeytype-tui" not in result.stdout:
                raise ValueError("installed executable smoke test failed")
        interactive_smoke(prefix / "bin/monkeytype-tui", isolated)
        # Remove/reinstall the executable while keeping the independent state.
        (prefix / "bin/monkeytype-tui").unlink()
        subprocess.run(["/bin/sh", str(package / "install.sh"), "--prefix", str(prefix)],
                       env=isolated, check=True, capture_output=True)
        interactive_smoke(prefix / "bin/monkeytype-tui", isolated, retained=True)
    print("PKG07/H2E09: checksum, install/reinstall, CLI, typing, saved history/dashboard and terminal restoration passed")


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--archive":
        smoke_archive(Path(sys.argv[2]))
    else:
        unittest.main()
