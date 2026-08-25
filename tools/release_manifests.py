#!/usr/bin/env python3
"""Generate package-manager definitions from completed release archives."""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re


TARGETS = {
    "linux_x64": ("x86_64-unknown-linux-gnu", ".tar.gz"),
    "linux_arm64": ("aarch64-unknown-linux-gnu", ".tar.gz"),
    "macos_x64": ("x86_64-apple-darwin", ".tar.gz"),
    "macos_arm64": ("aarch64-apple-darwin", ".tar.gz"),
    "windows_x64": ("x86_64-pc-windows-msvc", ".zip"),
}


def digest(path: pathlib.Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            value.update(block)
    return value.hexdigest()


def archive_name(target: str, suffix: str) -> str:
    return f"ocomment-{target}{suffix}"


def release_url(repository: str, version: str, target: str, suffix: str) -> str:
    return (
        f"https://github.com/{repository}/releases/download/v{version}/"
        f"{archive_name(target, suffix)}"
    )


def ruby_formula(repository: str, version: str, hashes: dict[str, str]) -> str:
    def stanza(key: str, indent: str = "    ") -> str:
        target, suffix = TARGETS[key]
        return (
            f'{indent}url "{release_url(repository, version, target, suffix)}"\n'
            f'{indent}sha256 "{hashes[key]}"'
        )

    return f'''class Ocomment < Formula
  desc "Fast, byte-preserving comment checker and remover"
  homepage "https://github.com/{repository}"
  version "{version}"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    if Hardware::CPU.arm?
{stanza("macos_arm64", "      ")}
    else
{stanza("macos_x64", "      ")}
    end
  end

  on_linux do
    if Hardware::CPU.arm?
{stanza("linux_arm64", "      ")}
    else
{stanza("linux_x64", "      ")}
    end
  end

  def install
    bin.install "ocomment"
    man1.install "ocomment.1"
    bash_completion.install "ocomment.bash" => "ocomment"
    zsh_completion.install "_ocomment"
    fish_completion.install "ocomment.fish"
  end

  test do
    assert_match version.to_s, shell_output("#{{bin}}/ocomment --version")
  end
end
'''


def scoop_manifest(repository: str, version: str, hashes: dict[str, str]) -> dict:
    target, suffix = TARGETS["windows_x64"]
    return {
        "version": version,
        "description": "Fast, byte-preserving comment checker and remover",
        "homepage": f"https://github.com/{repository}",
        "license": "MIT|Apache-2.0",
        "architecture": {
            "64bit": {
                "url": release_url(repository, version, target, suffix),
                "hash": hashes["windows_x64"],
                "extract_dir": f"ocomment-{target}",
            }
        },
        "bin": "ocomment.exe",
        "checkver": {"github": f"https://github.com/{repository}"},
        "autoupdate": {
            "architecture": {
                "64bit": {
                    "url": (
                        f"https://github.com/{repository}/releases/download/"
                        f"v$version/ocomment-{target}.zip"
                    )
                }
            }
        },
    }


def winget_manifest(repository: str, version: str, hashes: dict[str, str]) -> str:
    target, suffix = TARGETS["windows_x64"]
    url = release_url(repository, version, target, suffix)
    return f'''# yaml-language-server: $schema=https://aka.ms/winget-manifest.singleton.1.12.0.schema.json
PackageIdentifier: OComment.OComment
PackageVersion: {version}
PackageLocale: en-US
Publisher: OComment
PackageName: OComment
License: MIT OR Apache-2.0
LicenseUrl: https://github.com/{repository}/blob/v{version}/LICENSE-MIT
ShortDescription: Fast, byte-preserving comment checker and remover
PackageUrl: https://github.com/{repository}
InstallerType: zip
Installers:
  - Architecture: x64
    InstallerUrl: {url}
    InstallerSha256: {hashes["windows_x64"].upper()}
    NestedInstallerType: portable
    NestedInstallerFiles:
      - RelativeFilePath: ocomment-{target}\\ocomment.exe
        PortableCommandAlias: ocomment
ManifestType: singleton
ManifestVersion: 1.12.0
'''


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--release-dir", required=True, type=pathlib.Path)
    parser.add_argument("--version", required=True)
    # The release workflow passes $GITHUB_REPOSITORY; the default is for a
    # person generating the definitions by hand, and it has to name the
    # repository the archives are actually published from.
    parser.add_argument("--repository", default="P4suta/OComment")
    args = parser.parse_args()

    version = args.version.removeprefix("v")
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?", version):
        parser.error("version must be a semantic version, optionally prefixed by v")
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", args.repository):
        parser.error("repository must have owner/name form")

    hashes = {}
    for key, (target, suffix) in TARGETS.items():
        path = args.release_dir / archive_name(target, suffix)
        if not path.is_file():
            parser.error(f"missing release archive: {path}")
        hashes[key] = digest(path)

    (args.release_dir / "ocomment.rb").write_text(
        ruby_formula(args.repository, version, hashes), encoding="utf-8"
    )
    (args.release_dir / "ocomment-scoop.json").write_text(
        json.dumps(scoop_manifest(args.repository, version, hashes), indent=2) + "\n",
        encoding="utf-8",
    )
    (args.release_dir / "ocomment.winget.yaml").write_text(
        winget_manifest(args.repository, version, hashes), encoding="utf-8"
    )
    print("generated Homebrew, Scoop, and WinGet release definitions")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
