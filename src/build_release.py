#!/usr/bin/env python3
from __future__ import annotations

import json
import platform
import shutil
import subprocess
import sys
from pathlib import Path


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
