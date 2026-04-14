#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

required_files=(
  ".github/workflows/ci.yml"
  ".github/workflows/release.yml"
  ".github/workflows/pages.yml"
  ".github/ISSUE_TEMPLATE/bug_report.yml"
  ".github/ISSUE_TEMPLATE/feature_request.yml"
  ".github/pull_request_template.md"
  "CHANGELOG.md"
  "CONTRIBUTING.md"
  "ROADMAP.md"
  "docs/guides/release-playbook.md"
  "docs/guides/installation-and-permissions.md"
  "docs/guides/compatibility-and-support.md"
  "docs/guides/operations-and-observability.md"
  "website/index.html"
  "website/styles.css"
)

for file in "${required_files[@]}"; do
  if [[ ! -f "$file" ]]; then
    echo "Missing required file: $file" >&2
    exit 1
  fi
done

desktop_package_version="$(node -p "require('./apps/desktop/package.json').version")"
tauri_config_version="$(node -p "require('./apps/desktop/src-tauri/tauri.conf.json').version")"
desktop_cargo_version="$(node -e 'const fs = require("fs"); const cargoToml = fs.readFileSync("apps/desktop/src-tauri/Cargo.toml", "utf8"); const match = cargoToml.match(/^version\s*=\s*"([^"]+)"/m); if (!match) process.exit(1); process.stdout.write(match[1]);')"

if [[ "$desktop_package_version" != "$tauri_config_version" || "$desktop_package_version" != "$desktop_cargo_version" ]]; then
  echo "Desktop version mismatch:" >&2
  echo "  package.json: $desktop_package_version" >&2
  echo "  tauri.conf.json: $tauri_config_version" >&2
  echo "  src-tauri/Cargo.toml: $desktop_cargo_version" >&2
  exit 1
fi

required_readme_links=(
  "CHANGELOG.md"
  "CONTRIBUTING.md"
  "ROADMAP.md"
  "docs/guides/release-playbook.md"
  "docs/guides/installation-and-permissions.md"
)

for pattern in "${required_readme_links[@]}"; do
  if ! rg -q "$pattern" README.md README.zh-CN.md; then
    echo "README link check failed for pattern: $pattern" >&2
    exit 1
  fi
done

echo "Release readiness scaffold looks consistent."
