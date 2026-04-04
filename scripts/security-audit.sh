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

  # Parse pnpm audit JSON. Handles both npm audit v7 (list) and v9 (dict) formats.
  local parsed="$TMPDIR_WORK/node-parsed.json"
  python3 <<PYEOF > "$parsed"
import json

with open("$raw_output") as f:
    data = json.load(f)

summary = {"critical": 0, "high": 0, "medium": 0, "low": 0}
items = []

advisories = data.get("advisories", data.get("vulnerabilities", {}))

if isinstance(advisories, list):
    for v in advisories:
        sev = str(v.get("severity", "medium")).lower()
        if sev not in summary:
            sev = "medium"
        summary[sev] += 1
        adv = v.get("advisory", v)
        items.append({
            "package": v.get("module_name", v.get("name", "unknown")),
            "id": str(v.get("cwe", v.get("id", "unknown"))),
            "title": adv.get("title", v.get("title", "")),
            "severity": sev,
            "url": adv.get("url", v.get("url", ""))
        })
elif isinstance(advisories, dict):
    for key, v in advisories.items():
        if isinstance(v, dict):
            sev = str(v.get("severity", "medium")).lower()
            if sev not in summary:
                sev = "medium"
            summary[sev] += 1
            adv = v.get("advisory", v)
            items.append({
                "package": v.get("module_name", v.get("name", key)),
                "id": str(v.get("cve", v.get("id", key))),
                "title": adv.get("title", v.get("title", "")),
                "severity": sev,
                "url": adv.get("url", v.get("url", ""))
            })

print(json.dumps({"summary": summary, "vulnerabilities": items}))
PYEOF

  merge_into_report "$parsed" "node"

  # Human-readable output from metadata.vulnerabilities counts
  python3 <<PYEOF
import json

with open("$raw_output") as f:
    data = json.load(f)

meta = data.get("metadata", {})
vulns = meta.get("vulnerabilities", {})
total = sum(vulns.values()) if isinstance(vulns, dict) else 0
if total == 0:
    # Fall back to parsed summary
    with open("$parsed") as f2:
        p = json.load(f2)
    total = sum(p["summary"].values())
    vulns = p["summary"]

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

report["timestamp"] = datetime.datetime.utcnow().isoformat() + "Z"

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
