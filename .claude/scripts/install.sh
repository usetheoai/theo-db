#!/usr/bin/env bash
# Installs the Squad maintenance ecosystem into a target project as a plugin install
# (target/.claude/ layout). Hooks auto-detect the layout, so target/.claude/* is
# picked up identically to the standalone repo.
#
# Usage:
#   bash scripts/install.sh <target-project-dir> [--force]
#
# What it does:
#   1. Validates target is a directory.
#   2. Refuses to overwrite an existing target/.claude/ unless --force.
#   3. Copies skills/, rules/, hooks/, commands/, scripts/, plugin.json,
#      HOW-TO-USE.md into target/.claude/.
#   4. Writes settings.plugin.json as target/.claude/settings.json.
#   5. Creates empty scaffold under target/.claude/knowledge-base/
#      (plans, implementations, reviews, audits, discoveries/{plans,opportunities,snapshots},
#      adrs, grills, dogfood, judge-codex, backlog, maintenance-runs, tools).
#      agents/ ships the 8 domain specialists, copied from source.
#   6. Skips the source repo's history: caches, artifact dirs, audit trails,
#      CHANGELOG.md, .git/, .compaction-snapshots/, .attestations/.
#   7. Prints next steps.
#
# What it does NOT do:
#   - Modify the consumer's CLAUDE.md (write your own pointer to .claude/).
#   - Add anything to .gitignore (consumer decides whether to track .claude/).
#   - Install dependencies (python3, jq, ast-grep, ralph-loop plugin) — see HOW-TO-USE.md.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- args ---
if [ $# -lt 1 ]; then
  echo "Usage: bash scripts/install.sh <target-project-dir> [--force]" >&2
  exit 2
fi

TARGET="$1"
FORCE=0
if [ "${2:-}" = "--force" ]; then
  FORCE=1
fi

if [ ! -d "$TARGET" ]; then
  echo "ERROR: target is not a directory: $TARGET" >&2
  exit 2
fi

TARGET="$(cd "$TARGET" && pwd)"
ECO="$TARGET/.claude"

if [ "$TARGET" = "$SRC_DIR" ]; then
  echo "ERROR: target is the source repo itself. install.sh is for installing the ecosystem INTO another project." >&2
  exit 2
fi

if [ -d "$ECO" ] && [ "$FORCE" -ne 1 ]; then
  echo "ERROR: $ECO already exists. Use --force to overwrite (existing knowledge-base/ contents will be preserved if also present)." >&2
  exit 2
fi

echo "==> Installing Squad ecosystem"
echo "    source: $SRC_DIR"
echo "    target: $ECO"

# --- snapshot what the project owns, before overwriting it ---
# `rules/` and `agents/` are exactly where a project's own configuration lives: the routing
# table, its domain specialists, and every gate the "Next steps" below tells you to edit
# (code-quality-languages.txt, live-target.txt, acceptance-target.txt, the allow-lists).
# `--force` overwrote all of it silently. Measured: a `typescript | ... | ENABLED` line and a
# live-target block added to a fresh install were both gone after one re-run, with no message.
#
# In a repo that versions `.claude/` that is recoverable with `git restore`. TheoCode does not
# version it — the kit is a maintainer's tool, not product code — so silent was also permanent.
# The fix is not to merge (guessing which side of a config wins is how you get it wrong): it is
# to make the overwrite recoverable and loud. For an upgrade that must NOT clobber, use
# `patch_install.sh`, which copies a manifest and leaves agents/ and settings.json alone.
BACKUP_DIR=""
if [ -d "$ECO" ]; then
  BACKUP_DIR="$ECO/.install-backups/$(date +%Y%m%dT%H%M%S)"
  mkdir -p "$BACKUP_DIR"
  for item in rules agents; do
    [ -d "$ECO/$item" ] && cp -r "$ECO/$item" "$BACKUP_DIR/$item"
  done
  echo "==> Snapshot of the previous rules/ and agents/: $BACKUP_DIR"
fi

# --- copy ecosystem code ---
mkdir -p "$ECO"
for item in skills rules hooks commands scripts; do
  echo "==> Copying $item/"
  rm -rf "$ECO/$item"
  cp -r "$SRC_DIR/$item" "$ECO/$item"
done

# agents/ is copied FILE BY FILE, not wholesale. This repo dogfoods its own cycles, and
# `/implement` and `/review` write their per-run agent definitions into subdirectories here
# (`implement-slice-*/`, `review-*/`). Those are THIS repo's audit trail, not template content —
# and `cp -r` shipped two of them, dated May 2026, into every consumer install. The header above
# already promises to skip audit trails; this is what keeping that promise looks like.
echo "==> Copying agents/ (specialists only — per-run artifacts stay behind)"
rm -rf "$ECO/agents"
mkdir -p "$ECO/agents"
find "$SRC_DIR/agents" -maxdepth 1 -type f -name '*.md' -exec cp {} "$ECO/agents/" \;

# Top-level docs and manifest
for f in plugin.json HOW-TO-USE.md README.md .active_plan.example; do
  [ -f "$SRC_DIR/$f" ] && cp "$SRC_DIR/$f" "$ECO/$f"
done

# --- settings.json (plugin install variant) ---
if [ ! -f "$SRC_DIR/settings.plugin.json" ]; then
  echo "ERROR: $SRC_DIR/settings.plugin.json missing — required for plugin install layout." >&2
  exit 1
fi
cp "$SRC_DIR/settings.plugin.json" "$ECO/settings.json"
echo "==> settings.json written (plugin install variant)"

# --- knowledge-base scaffold (empty, idempotent) ---
# Mirrors the SEMANTIC structure of the source's knowledge-base/ — every
# category folder that a cycle writes to. Slug-keyed subdirs that exist in
# the source (e.g. implementations/slice-X/, tools/argo-cd/, discoveries/
# snapshots/slice-X/) are NOT mirrored — those are historical artefacts of
# the plan repo's own dogfood, not part of the template.
echo "==> Scaffolding knowledge-base/ subdirs (semantic structure)"
KB_DIRS=(
  "plans"                       # /to-plan outputs
  "implementations"             # /implement halt-loop logs
  "reviews"                     # /review reports
  "audits"                      # /code-quality + /deps-audit reports
  "acceptance"                  # /acceptance records (end-user validation of a release)
  "acceptance/evidence"         # screenshots, console/network dumps, transcripts
  "maintenance-runs"            # per-item macro-loop audit trail
  "backlog"                     # /backlog-item intake logs
  "adrs"                        # MADR 3.0 ADRs
  "grills"                      # /grill-me Q&A logs
  "dogfood"                     # /dogfood anchor manifest
  "dogfood/evidence"            # /dogfood evidence files
  "judge-codex"                 # orthogonal LLM jury outputs (optional plugin)
  "tools"                       # read-only docs of tools the project depends on (consumer populates)
  "discoveries"                 # /discover-* root
  "discoveries/plans"           # /discover-plan outputs
  "discoveries/opportunities"   # /discover-execute outputs
  "discoveries/snapshots"       # hash-verified snapshots cited by opportunities
  "progress"                    # per-slug progress.md (read by hooks + session-catchup)
)
for d in "${KB_DIRS[@]}"; do
  mkdir -p "$ECO/knowledge-base/$d"
done

# Optional: bring over the project-agnostic backlog template
if [ -f "$SRC_DIR/knowledge-base/backlog.md" ]; then
  if [ ! -f "$ECO/knowledge-base/backlog.md" ]; then
    cp "$SRC_DIR/knowledge-base/backlog.md" "$ECO/knowledge-base/backlog.md"
  fi
fi

# agents/ was copied above with the 8 domain specialists. An empty agents/ would
# leave route_domain.py pointing at files that do not exist — the routing table would
# resolve and the specialist behind it would be missing.


# --- What the overwrite actually took ---
# A snapshot nobody is told about is a snapshot nobody uses. Naming the files that CHANGED (not
# every file, which would be noise) is what turns a silent clobber into a diff someone can act on.
if [ -n "$BACKUP_DIR" ]; then
  CLOBBERED=$(
    cd "$BACKUP_DIR" && find . -type f | while read -r f; do
      cmp -s "$f" "$ECO/${f#./}" || echo "  ${f#./}"
    done
  )
  if [ -n "$CLOBBERED" ]; then
    echo ""
    # "or REMOVED" is not hedging: a specialist the source repo does not have — which is every
    # specialist a consumer writes for its own domains — is not overwritten, it is deleted by the
    # `rm -rf` above. Calling that "overwritten" would understate what just happened.
    echo "==> These files were OVERWRITTEN or REMOVED (they differed from the source):"
    echo "$CLOBBERED"
    echo ""
    echo "    Your previous copies: $BACKUP_DIR"
    echo "    Nothing was merged — diff them and re-apply what is yours. Project config lives in"
    echo "    rules/*.txt, the routing table in rules/cycle-backlog.md, and agents/*.md."
    echo "    To upgrade WITHOUT clobbering next time, use patch_install.sh instead."
  fi
fi

# --- Validation ---
# Run FROM THE TARGET. test_e2e_smoke.py resolves the ecosystem from the CWD, and the normal way
# to invoke this script is `cd squad && bash scripts/install.sh <target>` — so it was validating
# the source repo and printing OK for the installation it never opened. Measured: with a routed
# specialist and a cycle rule deleted from a fresh install, it answered
# `ecosystem: /home/paulo/Projetos/squad` / `ALL CHECKS PASSED` / exit 0. A check that cannot
# fail is worse than no check: it puts a green line next to a broken install.
#
# check_xrefs.py resolves from its own path and caught the same corruption (exit 1). Two lines
# printed the same word for two different amounts of verification.
echo "==> Validating install (from the target, not from here)"
( cd "$TARGET" && python3 .claude/scripts/check_xrefs.py --strict > /dev/null 2>&1 ) \
  && echo "    check_xrefs.py: OK" \
  || { echo "    check_xrefs.py: FAIL (re-run manually)"; }

( cd "$TARGET" && python3 .claude/scripts/test_e2e_smoke.py > /dev/null 2>&1 ) \
  && echo "    test_e2e_smoke.py: OK" \
  || { echo "    test_e2e_smoke.py: FAIL (re-run manually)"; }

cat <<EOF

==> Installation complete.

Next steps for the target project:

  1. (optional) Add a CLAUDE.md at the project root pointing to .claude/ and
     listing project-specific stack/conventions. Hooks read it on SessionStart.

  2. Configure project-specific gates (defaults are no-op until set):
       .claude/rules/code-quality-languages.txt    # uncomment languages you ship
       .claude/rules/discover-web-allowlist.txt    # domains for /discover-execute
       .claude/rules/code-quality-thresholds.txt   # per-project overrides
       .claude/rules/deps-audit-allowlist.txt      # CVE exemptions (with sunset)

  3. Verify ralph-loop plugin is installed (required by /implement, /discover-execute,
     /plan-improve):
       jq '.enabledPlugins' ~/.claude/settings.json | grep ralph-loop

  4. Open the project in Claude Code. The settings.json wires hooks; skills/
     and commands/ are auto-discovered.

  5. First run: /to-plan "{one-sentence feature}"  OR  /grill-me {topic}
EOF
