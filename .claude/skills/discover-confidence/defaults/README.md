---
type: defaults-bundle
purpose: Fallback definitions when project has no `.claude/rules/discover-opportunity-*` files
---

# Default Bundle (FALLBACK ONLY)

**This directory is FALLBACK.** Project rules in `.claude/rules/` always win.

The skill `/discover-confidence` checks `.claude/rules/` first. Only when the relevant rule files are missing or empty does it fall back to:

- `corner-coverage.md` — the 4-corner definition + the scoped `unknown` exception
- `../templates/rubric-opportunity.md` — the YAML rubric (cannot be overridden by project; always loaded from here)

## How fallback is detected

```python
PROJECT_GOLDEN_RULE = Path(".claude/rules/discover-opportunity-golden-rule.md")
PROJECT_THRESHOLDS = Path(".claude/rules/discover-opportunity-thresholds.txt")
DEFAULTS = Path(".claude/skills/discover-confidence/defaults")

def get_active_thresholds() -> Path:
    return PROJECT_THRESHOLDS if PROJECT_THRESHOLDS.exists() else \
           Path(".claude/skills/discover-confidence/templates/discover-opportunity-thresholds.example.txt")
```

Project rules win. Always.
