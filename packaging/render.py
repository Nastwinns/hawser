#!/usr/bin/env python3
"""Render packaging manifests with a version and real SHA256 checksums.

Reads the Homebrew and Scoop templates under packaging/ and substitutes the
placeholder tokens, writing the filled results to dist/hawser.rb and
dist/hawser.json.

Usage:
    render.py <version> <sha_macos_arm64> <sha_macos_x64> \
        <sha_linux_x64> <sha_windows_x64>

Homebrew covers macOS arm64/x64 and Linux x64; Scoop covers Windows x64.
Stdlib only, no third-party dependencies.
"""

import sys
from pathlib import Path

USAGE = (
    "usage: render.py <version> <sha_macos_arm64> <sha_macos_x64> "
    "<sha_linux_x64> <sha_windows_x64>"
)


def main(argv):
    if len(argv) != 6:
        sys.stderr.write(USAGE + "\n")
        return 2

    version = argv[1].lstrip("v")
    sha_macos_arm64 = argv[2]
    sha_macos_x64 = argv[3]
    sha_linux_x64 = argv[4]
    sha_windows_x64 = argv[5]

    packaging = Path(__file__).resolve().parent
    root = packaging.parent
    dist = root / "dist"
    dist.mkdir(parents=True, exist_ok=True)

    substitutions = {
        "@VERSION@": version,
        "@SHA_MACOS_ARM64@": sha_macos_arm64,
        "@SHA_MACOS_X64@": sha_macos_x64,
        "@SHA_LINUX_X64@": sha_linux_x64,
        "@SHA_WINDOWS_X64@": sha_windows_x64,
    }

    jobs = [
        (packaging / "homebrew" / "hawser.rb", dist / "hawser.rb"),
        (packaging / "scoop" / "hawser.json", dist / "hawser.json"),
    ]

    for template, target in jobs:
        text = template.read_text(encoding="utf-8")
        for token, value in substitutions.items():
            text = text.replace(token, value)
        target.write_text(text, encoding="utf-8")
        print(f"wrote {target}")

    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
