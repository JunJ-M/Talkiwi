#!/usr/bin/env python3
import re
import sys


BAD_SUBJECTS = {
    "update code",
    "fix files",
    "changes",
    "misc cleanup",
    "wip",
}


def main():
    if len(sys.argv) > 1:
        message = " ".join(sys.argv[1:])
    else:
        message = sys.stdin.read()

    text = message.strip("\n")
    if not text.strip():
        print("FAIL: commit message is empty")
        raise SystemExit(1)

    lines = text.splitlines()
    subject = lines[0].strip()
    body = "\n".join(lines[1:]).strip()
    errors = []
    warnings = []

    if len(subject) > 72:
        errors.append("subject exceeds 72 characters")
    if subject.lower() in BAD_SUBJECTS:
        errors.append("subject is too vague")
    if re.search(r"\b(files?|stuff|things|misc|various)\b", subject.lower()):
        warnings.append("subject may describe artifacts instead of intent")
    if not re.match(r"^(feat|fix|refactor|test|docs|build|chore):\s+.+", subject):
        errors.append("subject should match '<type>: <intent>'")
    if subject.endswith("."):
        warnings.append("subject should usually not end with a period")
    if body and len(lines) > 1 and lines[1].strip() != "":
        warnings.append("add a blank line between subject and body")
    if body and len(body.split()) < 5:
        warnings.append("body is present but may be too short to explain why")

    if errors:
        print("FAIL")
        for item in errors:
            print(f"- {item}")
        for item in warnings:
            print(f"- warning: {item}")
        raise SystemExit(1)

    print("PASS")
    for item in warnings:
        print(f"- warning: {item}")


if __name__ == "__main__":
    main()
