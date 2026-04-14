#!/usr/bin/env python3
import json
import subprocess
import sys
from collections import Counter


def run_git(args):
    result = subprocess.run(["git", *args], capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit(result.stderr.strip() or "git command failed")
    return result.stdout


def classify_path(path):
    lower = path.lower()
    if lower.endswith((".md", ".mdx", ".rst", ".txt")):
        return "docs"
    if "/test" in lower or "/tests" in lower or lower.endswith((".spec.ts", ".spec.js", ".test.ts", ".test.js")):
        return "tests"
    if lower.endswith((".json", ".yaml", ".yml", ".toml", ".ini")):
        return "config"
    if lower.endswith((".png", ".jpg", ".jpeg", ".gif", ".svg", ".webp")):
        return "assets"
    if lower.endswith(("lock", ".lock")) or "lockfile" in lower:
        return "lockfile"
    return "code"


def parse_name_status(text):
    entries = []
    for line in text.splitlines():
        if not line.strip():
            continue
        parts = line.split("\t")
        status = parts[0]
        path = parts[-1]
        entries.append({
            "status": status,
            "path": path,
            "category": classify_path(path),
        })
    return entries


def main():
    mode = "--cached" if len(sys.argv) > 1 and sys.argv[1] == "--staged" else "HEAD"
    args = ["diff", "--name-status"]
    if mode == "--cached":
        args.append("--cached")
    else:
        args.append("HEAD")

    entries = parse_name_status(run_git(args))
    counts = Counter(entry["category"] for entry in entries)
    statuses = Counter(entry["status"][0] for entry in entries)
    payload = {
        "mode": "staged" if mode == "--cached" else "working-tree",
        "total_files": len(entries),
        "category_counts": dict(counts),
        "status_counts": dict(statuses),
        "entries": entries,
    }
    print(json.dumps(payload, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
