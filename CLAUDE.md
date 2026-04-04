# BillForge - Project Instructions

## Codex CLI Validation (MANDATORY)

**Every implementation must be validated by Codex CLI before declaring done.**

### When to run Codex review:
- After any code change, before committing
- After fixing bugs
- After adding new features or tests
- After refactoring

### How to validate:
```bash
# Review uncommitted changes
codex review --uncommitted

# Review specific commits
codex review --commit HEAD

# Ask Codex to evaluate a specific concern
codex exec "Review <description of what to check>"
```

### Workflow:
1. Make changes
2. Run `codex review --uncommitted` or `codex exec "<review prompt>"`
3. Read Codex feedback carefully
4. Fix any issues Codex identifies (P1 = must fix, P2 = should fix, P3 = consider)
5. Re-run Codex if fixes were non-trivial
6. Only then commit

### What to ask Codex about:
- Correctness of logic changes
- Database schema consistency
- API contract alignment (frontend types vs backend responses)
- Security concerns (CORS, auth, input validation)
- Compatibility issues (lockfile versions, dependency mismatches)
- LAN/network access configuration

Do NOT skip Codex validation. If Codex is unavailable, note it in the commit message.
