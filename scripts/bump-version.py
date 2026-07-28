#!/usr/bin/env python3

import argparse
import re
import subprocess
import sys
from pathlib import Path


def run(*args: str) -> str:
    return subprocess.run(
        args,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout.strip()


def replace(
    files: dict[Path, str],
    path: str,
    pattern: str,
    replacement: str,
    *,
    count: int = 1,
) -> None:
    file = Path(path)
    updated, replacements = re.subn(pattern, replacement, files[file], count=count)
    if replacements != count:
        raise SystemExit(
            f"{path}: expected {count} match(es) for {pattern!r}, found {replacements}"
        )
    files[file] = updated


def render_metadata(version: str, grammar_commit: str) -> dict[Path, str]:
    paths = [
        "Cargo.toml",
        "Cargo.lock",
        "server/Cargo.toml",
        "server/Cargo.lock",
        "extension.toml",
        "flake.nix",
        "grammar/tree-sitter.json",
    ]
    files = {Path(path): Path(path).read_text() for path in paths}

    replace(files, "Cargo.toml", r'(?m)^version = "[^"]+"$', f'version = "{version}"')
    replace(
        files,
        "Cargo.lock",
        r'(?m)(^\[\[package\]\]\nname = "zed-cea"\nversion = ")[^"]+(")',
        rf"\g<1>{version}\g<2>",
    )
    replace(
        files,
        "server/Cargo.toml",
        r'(?m)^version = "[^"]+"$',
        f'version = "{version}"',
    )
    replace(
        files,
        "server/Cargo.lock",
        r'(?m)(^\[\[package\]\]\nname = "cea-language-server"\nversion = ")[^"]+(")',
        rf"\g<1>{version}\g<2>",
    )
    replace(
        files,
        "extension.toml",
        r'(?m)^version = "[^"]+"$',
        f'version = "{version}"',
    )
    replace(
        files,
        "extension.toml",
        r'(?m)^commit = "[0-9a-f]+"$',
        f'commit = "{grammar_commit}"',
    )
    replace(files, "extension.toml", r'(?m)^path = "[^"]+"$', 'path = "grammar"')
    replace(
        files,
        "flake.nix",
        r'(?m)^      version = "[^"]+";$',
        f'      version = "{version}";',
    )
    replace(
        files,
        "grammar/tree-sitter.json",
        r'(?m)^    "version": "[^"]+",$',
        f'    "version": "{version}",',
    )
    return files


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Bump project versions, the grammar pin, and CHANGELOG.md"
    )
    parser.add_argument("version", help="new release version in MAJOR.MINOR.PATCH form")
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify that metadata already matches VERSION without changing files",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    if not re.fullmatch(r"\d+\.\d+\.\d+", args.version):
        raise SystemExit("version must use MAJOR.MINOR.PATCH form")

    root = Path(run("git", "rev-parse", "--show-toplevel"))
    if Path.cwd().resolve() != root.resolve():
        raise SystemExit(f"run this command from the repository root: {root}")

    grammar_commit = run(
        "git",
        "log",
        "-1",
        "--format=%H",
        "--",
        "grammar/grammar.js",
        "grammar/src",
        "grammar/test",
    )
    files = render_metadata(args.version, grammar_commit)
    heading = f"## [{args.version}]"
    changelog = Path("CHANGELOG.md")

    if args.check:
        mismatches = [
            str(path) for path, expected in files.items() if path.read_text() != expected
        ]
        if not changelog.exists() or heading not in changelog.read_text():
            mismatches.append(str(changelog))
        if mismatches:
            raise SystemExit("release metadata differs: " + ", ".join(mismatches))
        print(f"release metadata matches {args.version} ({grammar_commit[:7]})")
        return

    if run("git", "status", "--porcelain"):
        raise SystemExit("refusing to bump a dirty worktree")

    current_match = re.search(r'(?m)^version = "([^"]+)"$', Path("Cargo.toml").read_text())
    if current_match is None:
        raise SystemExit("Cargo.toml: package version not found")
    current = tuple(map(int, current_match.group(1).split(".")))
    requested = tuple(map(int, args.version.split(".")))
    if requested <= current:
        raise SystemExit(f"new version must be greater than {current_match.group(1)}")

    existing_changelog = changelog.read_text() if changelog.exists() else "# Changelog\n"
    if heading in existing_changelog:
        raise SystemExit(f"CHANGELOG.md already contains {heading}")
    if not existing_changelog.startswith("# Changelog\n"):
        raise SystemExit("CHANGELOG.md must start with '# Changelog'")

    release_notes = run(
        "git-cliff",
        "--unreleased",
        "--tag",
        f"v{args.version}",
    )
    changelog_body = existing_changelog.removeprefix("# Changelog\n").lstrip()
    files[changelog] = (
        f"# Changelog\n\n{release_notes.strip()}\n"
        + (f"\n{changelog_body}" if changelog_body else "")
    )

    for path, contents in files.items():
        path.write_text(contents)

    print(f"bumped to {args.version} with grammar {grammar_commit[:7]}")
    print("review the changelog, validate the build, then commit and tag separately")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        sys.exit(error.returncode)
