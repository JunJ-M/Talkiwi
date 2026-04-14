#!/usr/bin/env python3
import json
import subprocess
import sys


def run_analyzer(staged_only):
    script = [sys.executable, "scripts/analyze_changes.py"]
    if staged_only:
        script.append("--staged")
    result = subprocess.run(script, capture_output=True, text=True)
    if result.returncode != 0:
        raise SystemExit(result.stderr.strip() or result.stdout.strip() or "analyzer failed")
    return json.loads(result.stdout)


def suggest_groups(entries):
    groups = []
    docs = [e for e in entries if e["category"] == "docs"]
    tests = [e for e in entries if e["category"] == "tests"]
    config = [e for e in entries if e["category"] == "config"]
    code = [e for e in entries if e["category"] == "code"]

    if code:
        groups.append({
            "name": "primary-code-change",
            "reason": "Behavioral or structural code changes usually define the main intent.",
            "files": [e["path"] for e in code],
        })
    if tests and not code:
        groups.append({
            "name": "test-only-change",
            "reason": "Test changes without product code often deserve their own review path.",
            "files": [e["path"] for e in tests],
        })
    elif tests:
        groups.append({
            "name": "tests-supporting-code",
            "reason": "Keep with code only if tests are tightly coupled to the same intent.",
            "files": [e["path"] for e in tests],
        })
    if docs:
        groups.append({
            "name": "documentation",
            "reason": "Documentation can often be reviewed independently unless required to explain new behavior.",
            "files": [e["path"] for e in docs],
        })
    if config:
        groups.append({
            "name": "configuration",
            "reason": "Config or metadata changes may hide a separate operational intent.",
            "files": [e["path"] for e in config],
        })

    return groups


def main():
    staged_only = len(sys.argv) > 1 and sys.argv[1] == "--staged"
    payload = run_analyzer(staged_only)
    groups = suggest_groups(payload["entries"])
    result = {
        "mode": payload["mode"],
        "total_files": payload["total_files"],
        "suggested_groups": groups,
        "should_consider_split": len(groups) > 1,
    }
    print(json.dumps(result, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
