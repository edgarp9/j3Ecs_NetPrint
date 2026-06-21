#!/usr/bin/env python3
from __future__ import annotations

import json
import platform
import shutil
import subprocess
import sys
from pathlib import Path

THIRD_PARTY_NOTICE_FILE = "THIRD_PARTY_NOTICES.txt"
PROJECT_LICENSE_FILE = "LICENSE"
ABOUT_FILE = "about.txt"
SOURCE_NOTICE_FILE = "SOURCE_NOTICE.md"
RUST_STANDARD_LIBRARY_NOTICE_FILE = "RUST_STANDARD_LIBRARY_NOTICES.html"


def main() -> int:
    project_root = Path(__file__).resolve().parent
    cargo = shutil.which("cargo")
    if cargo is None:
        print("error: cargo was not found in PATH.", file=sys.stderr)
        return 127

    release_dir = resolve_release_dir(cargo, project_root)
    print(f"Building release binary in {project_root}", file=sys.stderr, flush=True)

    build_result = subprocess.run(
        [cargo, "build", "--release"],
        cwd=project_root,
    )
    if build_result.returncode != 0:
        return build_result.returncode

    if not release_dir.is_dir():
        print(f"error: release directory was not created: {release_dir}", file=sys.stderr)
        return 1

    copy_release_file(project_root, release_dir, PROJECT_LICENSE_FILE, "project license")
    copy_release_file(project_root, release_dir, ABOUT_FILE, "about text")
    copy_release_file(project_root, release_dir, SOURCE_NOTICE_FILE, "source notice")
    copy_release_file(
        project_root,
        release_dir,
        THIRD_PARTY_NOTICE_FILE,
        "third-party notices",
    )
    copy_rust_standard_library_notice(release_dir)

    print(f"Release build complete: {release_dir}", file=sys.stderr, flush=True)
    if not open_directory(release_dir):
        print(f"warning: could not open release directory: {release_dir}", file=sys.stderr)

    return 0


def resolve_release_dir(cargo: str, project_root: Path) -> Path:
    metadata_result = subprocess.run(
        [cargo, "metadata", "--format-version", "1", "--no-deps"],
        cwd=project_root,
        check=False,
        capture_output=True,
        text=True,
    )
    if metadata_result.returncode != 0:
        return project_root / "target" / "release"

    try:
        metadata = json.loads(metadata_result.stdout)
        target_dir = Path(metadata["target_directory"])
    except (json.JSONDecodeError, KeyError, TypeError):
        return project_root / "target" / "release"

    return target_dir / "release"


def copy_release_file(
    project_root: Path,
    release_dir: Path,
    file_name: str,
    label: str,
) -> None:
    source = project_root / file_name
    if not source.is_file():
        print(f"warning: {label} file was not found: {source}", file=sys.stderr)
        return

    destination = release_dir / file_name
    shutil.copy2(source, destination)
    print(f"Copied {label}: {destination}", file=sys.stderr, flush=True)


def copy_rust_standard_library_notice(release_dir: Path) -> None:
    rustc = shutil.which("rustc")
    if rustc is None:
        print(
            "warning: rustc was not found in PATH; "
            "Rust standard library notices were not copied.",
            file=sys.stderr,
        )
        return

    sysroot_result = subprocess.run(
        [rustc, "--print", "sysroot"],
        check=False,
        capture_output=True,
        text=True,
    )
    if sysroot_result.returncode != 0:
        print(
            "warning: failed to resolve rustc sysroot; "
            "Rust standard library notices were not copied.",
            file=sys.stderr,
        )
        return

    sysroot = Path(sysroot_result.stdout.strip())
    source = sysroot / "share" / "doc" / "rust" / "COPYRIGHT-library.html"
    if not source.is_file():
        print(
            f"warning: Rust standard library notice file was not found: {source}",
            file=sys.stderr,
        )
        return

    destination = release_dir / RUST_STANDARD_LIBRARY_NOTICE_FILE
    shutil.copy2(source, destination)
    print(
        f"Copied Rust standard library notices: {destination}",
        file=sys.stderr,
        flush=True,
    )


def open_directory(path: Path) -> bool:
    system = platform.system()
    try:
        if system == "Windows":
            subprocess.Popen(["explorer", str(path)])
            return True

        if system == "Linux":
            return open_linux_directory(path)

        if system == "Darwin":
            opener = shutil.which("open")
            if opener is not None:
                subprocess.Popen([opener, str(path)])
                return True
    except OSError as error:
        print(f"warning: failed to launch file manager: {error}", file=sys.stderr)

    return False


def open_linux_directory(path: Path) -> bool:
    opener_commands = (
        ("xdg-open", [str(path)]),
        ("gio", ["open", str(path)]),
        ("gnome-open", [str(path)]),
        ("kde-open", [str(path)]),
    )

    for executable, args in opener_commands:
        opener = shutil.which(executable)
        if opener is None:
            continue

        subprocess.Popen(
            [opener, *args],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        return True

    return False


if __name__ == "__main__":
    raise SystemExit(main())
