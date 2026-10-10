#!/usr/bin/env python3
"""Merge the per-architecture latest-mac.yml files into one.

The Apple Silicon and Intel apps are built on separate machines, and each
writes a latest-mac.yml that lists only its own files. The in-app updater
reads one latest-mac.yml and picks the file for its architecture, so the
release needs both lists in a single file.

Usage: merge-mac-update.py OUT IN [IN...]
"""

import sys


def split(text: str) -> tuple[list[str], list[str], list[str]]:
    """Return (lines before files:, file entry lines, lines after)."""
    lines = text.splitlines()
    start = lines.index("files:")
    end = start + 1
    while end < len(lines) and lines[end].startswith((" ", "-")):
        end += 1
    return lines[:start], lines[start + 1 : end], lines[end:]


def main() -> None:
    out, first, *rest = sys.argv[1:]
    with open(first) as f:
        head, files, tail = split(f.read())
    versions = {next(line for line in head if line.startswith("version:"))}
    for path in rest:
        with open(path) as f:
            other_head, other_files, _ = split(f.read())
        versions.add(next(line for line in other_head if line.startswith("version:")))
        files += other_files
    if len(versions) != 1:
        sys.exit(f"Mac update files disagree on the version: {sorted(versions)}")
    with open(out, "w") as f:
        f.write("\n".join([*head, "files:", *files, *tail]) + "\n")


if __name__ == "__main__":
    main()
