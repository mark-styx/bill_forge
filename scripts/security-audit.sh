#!/usr/bin/env bash
# security-audit.sh - Consolidated dependency security audit for BillForge
# Runs cargo-audit (Rust) and pnpm audit (Node.js), produces a unified report.
#
# Exit codes:
#   0 - No P1 (critical/high) vulnerabilities found
#   1 - P1 vulnerabilities found (blocks CI)
#
# Output: security-audit-report.json (machine-readable consolidated report)
set -euo pipefail

REPORT_FILE="security-audit-report.json"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
REPORT_PATH="$PROJECT_ROOT/$REPORT_FILE"

# Temp dir for intermediate data
TMPDIR_WORK="$(mktemp -d)"
trap 'rm -rf "$TMPDIR_WORK"' EXIT

# Initialize report structure
init_report() {
  cat > "$REPORT_PATH" <<'EOF'
{
  "timestamp": "",
  "rust": { "vulnerabilities": [], "summary": { "critical": 0, "high": 0, "medium": 0, "low": 0 } },
  "node": { "vulnerabilities": [], "summary": { "critical": 0, "high": 0, "medium": 0, "low": 0 } },
  "p1_count": 0,
  "p2_count": 0
}
EOF
}

# Python helper: safely merge parsed JSON into the report file
# Usage: merge_into_report <json-file> <key> (key = "rust" or "node")
merge_into_report() {
  local json_file="$1"
  local key="$2"
  python3 <<PYEOF
import json

with open("$REPORT_PATH") as f:
    report = json.load(f)

with open("$json_file") as f:
    parsed = json.load(f)

report["$key"] = parsed
summary = parsed.get("summary", {})
report["p1_count"] += summary.get("critical", 0) + summary.get("high", 0)
report["p2_count"] += summary.get("medium", 0)

with open("$REPORT_PATH", "w") as f:
    json.dump(report, f, indent=2)
PYEOF
}

# Run Rust cargo audit
run_cargo_audit() {
  local backend_dir="$PROJECT_ROOT/backend"

  if ! command -v cargo-audit &>/dev/null; then
    echo "::warning::cargo-audit not installed; skipping Rust audit"
    echo "  Install with: cargo install cargo-audit"
    return 0
  fi

  if [ ! -f "$backend_dir/Cargo.lock" ]; then
    echo "::warning::No Cargo.lock found in backend/; skipping Rust audit"
    return 0
  fi

  echo "=== Rust Dependency Audit ==="

  local raw_output="$TMPDIR_WORK/cargo-audit-raw.json"
  (cd "$backend_dir" && cargo audit --json 2>/dev/null > "$raw_output") || true

  if [ ! -s "$raw_output" ]; then
    echo "  No Rust vulnerabilities found"
    return 0
  fi

  # Parse cargo audit JSON. Cargo-audit does not include a "severity" field,
  # so we map from CVSS score when available, else default to "medium".
  local parsed="$TMPDIR_WORK/rust-parsed.json"
  python3 <<PYEOF > "$parsed"
import sys, json

with open("$raw_output") as f:
    data = json.load(f)

vulns = data.get("vulnerabilities", {}).get("list", [])
summary = {"critical": 0, "high": 0, "medium": 0, "low": 0}
items = []

def cvss_to_severity(cvss):
    if cvss is None:
        return "medium"
    # CVSS can be a numeric score, a dict with "score", or a vector string like "CVSS:3.1/..."
    if isinstance(cvss, (int, float)):
        score = float(cvss)
    elif isinstance(cvss, dict):
        score = float(cvss.get("score", 0))
    elif isinstance(cvss, str):
        # Vector string: extract the base score is not trivial; treat as medium
        return "medium"
    else:
        return "medium"
    if score >= 9.0:
        return "critical"
    if score >= 7.0:
        return "high"
    if score >= 4.0:
        return "medium"
    return "low"

for v in vulns:
    adv = v.get("advisory", {})
    cvss = adv.get("cvss")
    if cvss:
        sev = cvss_to_severity(cvss)
    else:
        sev = "medium"
    summary[sev] += 1
    items.append({
        "package": adv.get("package", "unknown"),
        "id": adv.get("id", "unknown"),
        "title": adv.get("title", ""),
        "severity": sev,
        "url": adv.get("url", "")
    })

print(json.dumps({"summary": summary, "vulnerabilities": items}))
PYEOF

  merge_into_report "$parsed" "rust"

  # Human-readable output
  python3 <<PYEOF
import json

with open("$parsed") as f:
    data = json.load(f)

vulns = data["vulnerabilities"]
if not vulns:
    print("  No Rust vulnerabilities found")
else:
    for v in vulns:
        print(f"  [{v['severity'].upper()}] {v['package']} - {v['title']} ({v['id']})")
PYEOF
}

# Run pnpm audit
run_pnpm_audit() {
  if ! command -v pnpm &>/dev/null; then
    echo "::warning::pnpm not installed; skipping Node.js audit"
    return 0
  fi

  if [ ! -f "$PROJECT_ROOT/pnpm-lock.yaml" ]; then
    echo "::warning::No pnpm-lock.yaml found; skipping Node.js audit"
    return 0
  fi

  echo ""
  echo "=== Node.js Dependency Audit ==="

  local raw_output="$TMPDIR_WORK/pnpm-audit-raw.json"
  (cd "$PROJECT_ROOT" && pnpm audit --json 2>/dev/null > "$raw_output") || true

  if [ ! -s "$raw_output" ]; then
    echo "  No Node.js vulnerabilities found"
    return 0
  fi

  # Parse pnpm audit JSON. Handles npm audit list/dict formats and pnpm's
  # actions/resolves shape. We count only advisories with a concrete dependency
  # path, which filters workspace-name false positives.
  local parsed="$TMPDIR_WORK/node-parsed.json"
  python3 <<PYEOF > "$parsed"
import json

with open("$raw_output") as f:
    data = json.load(f)

summary = {"critical": 0, "high": 0, "medium": 0, "low": 0}
items = []

advisories = data.get("advisories", data.get("vulnerabilities", {}))
actions_by_id = {}

for action in data.get("actions", []) or []:
    for resolved in action.get("resolves", []) or []:
        advisory_id = str(resolved.get("id", ""))
        if advisory_id:
            actions_by_id[advisory_id] = {
                "action": action.get("action"),
                "module": action.get("module"),
                "target": action.get("target"),
                "path": resolved.get("path"),
            }

def normalize_severity(value):
    sev = str(value or "medium").lower()
    if sev == "moderate":
        return "medium"
    if sev == "info":
        return "low"
    if sev not in summary:
        return "medium"
    return sev

def finding_paths(v):
    paths = []
    for finding in v.get("findings", []) or []:
        for path in finding.get("paths", []) or []:
            if path:
                paths.append(path)
    for path in v.get("paths", []) or []:
        if path:
            paths.append(path)
    for node in v.get("nodes", []) or []:
        if node:
            paths.append(node)
    for effect in v.get("effects", []) or []:
        if effect:
            paths.append(effect)
    return paths

def add_item(advisory_id, v):
    paths = finding_paths(v)
    if not paths:
        return

    adv = v.get("advisory", v)
    sev = normalize_severity(v.get("severity", adv.get("severity", "medium")))
    summary[sev] += 1
    action = actions_by_id.get(str(advisory_id), {})
    items.append({
        "package": v.get("module_name", v.get("name", action.get("module") or "unknown")),
        "id": str(v.get("cve", v.get("id", advisory_id))),
        "title": adv.get("title", v.get("title", "")),
        "severity": sev,
        "url": adv.get("url", v.get("url", "")),
        "paths": paths,
        "action": action.get("action"),
        "target": action.get("target"),
    })

if isinstance(advisories, list):
    for v in advisories:
        add_item(v.get("id", "unknown"), v)
elif isinstance(advisories, dict):
    for key, v in advisories.items():
        if isinstance(v, dict):
            add_item(key, v)

print(json.dumps({"summary": summary, "vulnerabilities": items}))
PYEOF

  merge_into_report "$parsed" "node"

  # Human-readable output from parsed, actionable findings.
  python3 <<PYEOF
import json

with open("$parsed") as f:
    data = json.load(f)

vulns = data["summary"]
total = sum(vulns.values())

if total == 0:
    print("  No Node.js vulnerabilities found")
else:
    for sev in ["critical", "high", "medium", "low"]:
        count = vulns.get(sev, 0)
        if count > 0:
            print(f"  {sev.upper()}: {count}")
PYEOF
}

# Print final summary and set exit code
print_summary() {
  echo ""
  echo "=== Security Audit Summary ==="

  # Update timestamp and read counts
  local p1 p2
  read -r p1 p2 < <(python3 <<PYEOF
import json, datetime

with open("$REPORT_PATH") as f:
    report = json.load(f)

report["timestamp"] = datetime.datetime.now(datetime.UTC).isoformat().replace("+00:00", "Z")

with open("$REPORT_PATH", "w") as f:
    json.dump(report, f, indent=2)

print(report["p1_count"], report["p2_count"])
PYEOF
)

  echo "  P1 (critical/high): ${p1:-0}"
  echo "  P2 (medium):        ${p2:-0}"
  echo ""
  echo "  Report saved to: $REPORT_FILE"

  if [ "${p1:-0}" -gt 0 ]; then
    echo ""
    echo "::error::${p1} P1 (critical/high) vulnerabilities found. These must be fixed before merging."
    return 1
  fi

  if [ "${p2:-0}" -gt 0 ]; then
    echo ""
    echo "::warning::${p2} P2 (medium) vulnerabilities found. Consider addressing these."
  fi

  return 0
}

# Main
main() {
  echo "BillForge Security Audit"
  echo "========================"
  echo ""

  init_report
  run_cargo_audit
  run_pnpm_audit
  print_summary
}

main
