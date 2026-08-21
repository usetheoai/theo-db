#!/bin/bash
# Stop hook: end-of-session sanity checks (agnostic).
#
# Behavior:
#   1. TDD gate (warn-first): for every changed production source file, warn
#      if no sibling test file is detected in the same directory (heuristic;
#      supports common *_test.* / *.test.* / *.spec.* / test_*.* naming).
#   2. CHANGELOG discipline (HARD GATE — Inquebrável Rule 6 + cycle-review BLOCKER):
#      if production source changed but CHANGELOG.md did not, BLOCK.
#   3. Secret leak (HARD GATE — cycle-review BLOCKER): if newly tracked files
#      match secret patterns (.env / credentials* / *.pem / *.key), BLOCK.
#   4. Pre-release honesty (warn-first): if README.md was modified, scan for
#      unverified production-ready / SLA claims.
#
# Hard gates align with rules/cycle-review.md § Hard gates (BLOCKER-level).
# Warn-first items are advisory — output is fed to Claude as context.
#
# Exit codes:
#   0 — clean OR only advisory warnings emitted
#   2 — hard-gate violation (CHANGELOG missing or secrets committed)
#
# Override: setting STOP_VALIDATION_WARN_ONLY=1 reverts every gate to warn-first
# (escape hatch for legitimate bulk reorgs; document the rationale in CHANGELOG).

set -uo pipefail

# shellcheck source=lib/detect-layout.sh
source "$(dirname "$0")/lib/detect-layout.sh"

# ----------------------------------------------------------------------------
# Collect ALL modified files (unstaged + staged + last commit)
# ----------------------------------------------------------------------------
UNSTAGED=$(git diff --name-only 2>/dev/null || true)
STAGED=$(git diff --cached --name-only 2>/dev/null || true)
LAST_COMMIT=$(git diff --name-only HEAD~1..HEAD 2>/dev/null || true)

ALL_FILES=$(echo -e "${UNSTAGED}\n${STAGED}\n${LAST_COMMIT}" | sort -u | grep -v '^$' || true)

WARNINGS=()
BLOCKERS=()

# ----------------------------------------------------------------------------
# Reference leakage (third layer of the provenance guard) — evaluated BEFORE the
# no-diff early exit on purpose. Layers 1 and 2 live in validate-command.sh and
# block copying content OUT of the study zone and citing its paths in commit
# messages; neither sees a manual paste. This checks the RESULT: a block of
# consecutive lines shared between the project and the zone.
# It must run even when the session produced only UNTRACKED files — `git diff`
# does not list those, so the early exit below would skip the check exactly in
# the "pasted a brand-new file" case, which is the likeliest way a copy lands.
# Advisory by design: exact-shingle matching is strong evidence, not proof, and a
# false BLOCK would be worse than a WARN. SKIPs when the zone is absent.
# ----------------------------------------------------------------------------
LEAK_SCRIPT="$PROJECT_DIR/scripts/check_reference_leakage.py"
if [ -f "$LEAK_SCRIPT" ] && command -v python3 >/dev/null 2>&1; then
  LEAK_OUT=$(python3 "$LEAK_SCRIPT" --repo "$PROJECT_DIR" --strict 2>&1 || true)
  if echo "$LEAK_OUT" | grep -q "SUSPECTED COPY"; then
    msg="Suspected literal copy of third-party study material (provenance risk). Review each match; if legitimate, record source + licence in CHANGELOG.md:"
    while IFS= read -r line; do
      case "$line" in
        *"shares"*"consecutive lines with"*) msg+="\n    -${line}" ;;
      esac
    done <<< "$LEAK_OUT"
    WARNINGS+=("$msg")
  fi
fi

if [ -z "$ALL_FILES" ] && [ ${#WARNINGS[@]} -eq 0 ]; then
  exit 0
fi

# Escape hatch
WARN_ONLY="${STOP_VALIDATION_WARN_ONLY:-0}"

# ----------------------------------------------------------------------------
# 1. TDD gate (warn-first) — heuristic test pairing
# ----------------------------------------------------------------------------
# Recognized source extensions: .go .py .ts .tsx .js .jsx .rs .java .kt .rb .cs
# Recognized test-name patterns in the same directory:
#   <name>_test.<ext>          (Go, Python, etc.)
#   <name>.test.<ext>          (TS/JS Jest convention)
#   <name>.spec.<ext>          (TS/JS Jasmine/RSpec)
#   test_<name>.<ext>          (Python pytest)
# Falls back to "ANY test file in the same directory" (idiomatic in some langs).
# Skips generated/doc files and obvious vendored/third-party trees.
SRC_CHANGED=$(echo "$ALL_FILES" \
  | grep -E '\.(go|py|ts|tsx|js|jsx|rs|java|kt|rb|cs)$' \
  | grep -vE '(^|/)(node_modules|vendor|dist|build|target|\.venv|__pycache__|\.next|\.nuxt)/' \
  | grep -vE '(_test|\.test|\.spec)\.[a-z]+$' \
  | grep -vE '(^|/)test_[^/]+\.[a-z]+$' \
  | grep -vE '(^|/)zz_generated[^/]*\.go$' \
  | grep -vE '(^|/)doc\.go$' \
  || true)

if [ -n "$SRC_CHANGED" ]; then
  MISSING_TESTS=()
  while IFS= read -r src_file; do
    [ -z "$src_file" ] && continue

    pkg_dir=$(dirname "$src_file")
    base_no_ext="${src_file##*/}"
    base_no_ext="${base_no_ext%.*}"
    ext="${src_file##*.}"

    # Candidate file names in same directory
    if [ -f "${pkg_dir}/${base_no_ext}_test.${ext}" ] || \
       [ -f "${pkg_dir}/${base_no_ext}.test.${ext}" ] || \
       [ -f "${pkg_dir}/${base_no_ext}.spec.${ext}" ] || \
       [ -f "${pkg_dir}/test_${base_no_ext}.${ext}" ]; then
      continue
    fi

    # Fallback: ANY test-named file in the same package directory
    found=$(find "$pkg_dir" -maxdepth 1 \( \
        -name "*_test.${ext}" -o -name "*.test.${ext}" -o -name "*.spec.${ext}" -o -name "test_*.${ext}" \
      \) -print -quit 2>/dev/null || true)
    if [ -n "$found" ]; then
      continue
    fi

    MISSING_TESTS+=("$src_file")
  done <<< "$SRC_CHANGED"

  if [ ${#MISSING_TESTS[@]} -gt 0 ]; then
    msg="TDD gate (warn-first) — Inquebrável Rule 7: the following production source files have no sibling test file detected:"
    for f in "${MISSING_TESTS[@]}"; do
      msg+="\n    - $f"
    done
    msg+="\n  See $ECO/rules/testing.md for the project's test pairing convention."
    WARNINGS+=("$msg")
  fi
fi

# ----------------------------------------------------------------------------
# 2. CHANGELOG discipline (HARD GATE — Inquebrável Rule 6 + cycle-review BLOCKER)
# ----------------------------------------------------------------------------
if [ -f "CHANGELOG.md" ]; then
  CODE_CHANGED=$(echo "$ALL_FILES" \
    | grep -E '\.(go|py|ts|tsx|js|jsx|rs|java|kt|rb|cs)$' \
    | grep -vE '(_test|\.test|\.spec)\.[a-z]+$' \
    | grep -vE '(^|/)(node_modules|vendor|dist|build|target|\.venv|__pycache__)/' \
    || true)
  # B-088 — "o arquivo esta no diff" NAO e "a entrada existe". Medido sobre esta propria sessao:
  # o commit `c52dfda` entregou o B-081, tocou o CHANGELOG por OUTRAS razoes, e nao acrescentou
  # entrada nenhuma — o gate passou, e a falta so apareceu no corte da release, uma sessao depois.
  #
  # Quando ha commit na sessao, a pergunta passa a ser sobre a ENTRADA. O checador espelha a
  # definicao de "codigo de producao" logo acima, de proposito: duas definicoes no mesmo
  # repositorio divergem, e a segunda mudaria em silencio o que o portao significa.
  #
  # O caminho antigo (presenca de arquivo) fica para o estado NAO COMMITADO, onde nao ha revisao a
  # inspecionar. E menos rigoroso e e o melhor disponivel ali — dize-lo e melhor que fingir.
  ENTRY_CHECKER=""
  for c in "$ECO/scripts/check_changelog_entry.py" ".claude/scripts/check_changelog_entry.py"; do
    [ -f "$c" ] && { ENTRY_CHECKER="$c"; break; }
  done
  # Bloqueia SO em rc=1 (violacao). rc=2 e "nao pude inspecionar" — um merge commit, um repo sem
  # historico — e tratar os dois iguais colapsa "nao pude perguntar" com "perguntei e a resposta e
  # nao". Foi o que bloqueou o encerramento desta sessao apos um merge limpo.
  ENTRY_RC=0
  if [ -n "$ENTRY_CHECKER" ] && [ -n "$LAST_COMMIT" ]; then
    python3 "$ENTRY_CHECKER" --rev HEAD >/dev/null 2>&1 || ENTRY_RC=$?
  fi
  if [ -n "$CODE_CHANGED" ] && [ "$ENTRY_RC" = "1" ]; then
    msg="CHANGELOG.md foi tocado mas o ultimo commit NAO acrescentou entrada ao [Unreleased], e ele muda codigo de producao (Inquebravel Rule 6; B-088). Tocar o arquivo nao e registrar a mudanca."
    if [ "$WARN_ONLY" = "1" ]; then WARNINGS+=("$msg"); else BLOCKERS+=("$msg"); fi
  elif [ -n "$CODE_CHANGED" ] && ! echo "$ALL_FILES" | grep -qE '^CHANGELOG\.md$'; then
    msg="CHANGELOG.md not updated despite production source changes (Inquebrável Rule 6; cycle-review BLOCKER). Add an entry to [Unreleased] before stopping. Override with STOP_VALIDATION_WARN_ONLY=1 only when the change is a bulk reorg with the rationale documented separately."
    if [ "$WARN_ONLY" = "1" ]; then
      WARNINGS+=("$msg")
    else
      BLOCKERS+=("$msg")
    fi
  fi
fi

# ----------------------------------------------------------------------------
# 2z. Citacao de bundle resolve (HARD GATE — B-069, bullet 3)
# ----------------------------------------------------------------------------
# A alegacao e o artefato nao podem divergir. Um documento que cita um bundle tem de citar um que
# EXISTE — em disco, ou sob `git:<sha>:<caminho>`, a forma que o acervo ja usa para o que foi
# removido de proposito.
#
# ESTE GATE NAO EXIGE que todo documento cite bundle. Medido em 2026-08-21: 170 documentos, 13 citam.
# Exigir de todos reprovaria a maioria, e portao que nunca passa alguem desliga — foi por isso que o
# item ficou parado. O que ele exige e que QUEM CITA cite algo que resolve: nao ter prova e uma
# coisa, alegar uma prova inexistente e outra, e pior, porque convida o leitor a confiar num arquivo
# que ninguem pode abrir.
#
# Quando entrou, havia 26 citacoes quebradas em 9 arquivos — todas residuo de UMA remocao deliberada
# (`7cd157d`). Nao eram fabricacoes; eram ponteiros que a limpeza deixou para tras.
BUNDLE_CHECKER=""
for c in "$ECO/scripts/check_bundle_citations.py" ".claude/scripts/check_bundle_citations.py"; do
  [ -f "$c" ] && { BUNDLE_CHECKER="$c"; break; }
done
if [ -n "$BUNDLE_CHECKER" ] && [ -d wiki ]; then
  BUNDLE_OUT="$(python3 "$BUNDLE_CHECKER" 2>&1)"; BUNDLE_RC=$?
  if [ "$BUNDLE_RC" = "1" ]; then
    msg="Citacao de bundle que nao resolve (B-069). $(echo "$BUNDLE_OUT" | head -1)"
    if [ "$WARN_ONLY" = "1" ]; then WARNINGS+=("$msg"); else BLOCKERS+=("$msg"); fi
  fi
fi

# ----------------------------------------------------------------------------
# 2a. Backlog registry integrity (HARD GATE — B-051)
# ----------------------------------------------------------------------------
# POR QUE AQUI. O DoD do B-051 pedia "no mesmo lugar em que o `okf-validate` roda". Medido em
# 2026-08-20: o `okf-validate` NÃO é invocado em lugar nenhum — nem hook, nem workflow, nem
# script. Seguir a letra significaria rodar em lugar nenhum, que é o oposto da intenção escrita
# no mesmo bullet ("não num passo que alguém precise lembrar"). Este hook é o lugar mecanizado
# que existe: já BLOQUEIA por CHANGELOG e por segredo.
#
# Só roda quando o BACKLOG.md foi TOCADO na sessão. Validar um registro que ninguém mexeu
# transformaria dívida herdada em bloqueio de toda sessão — e um portão que reprova sempre é um
# portão que alguém desliga.
if echo "$ALL_FILES" | grep -qE '(^|/)BACKLOG\.md$' && [ -f "BACKLOG.md" ]; then
  BL_CHECKER=""
  for c in ".claude/skills/backlog-review/scripts/check_backlog_structure.py" \
           "skills/backlog-review/scripts/check_backlog_structure.py"; do
    [ -f "$c" ] && { BL_CHECKER="$c"; break; }
  done
  if [ -n "$BL_CHECKER" ]; then
    BL_OUT=$(python3 "$BL_CHECKER" BACKLOG.md 2>&1) || true
    # Exit 1 = blocker (INVALID). Exit 3 = major (NEEDS_REVISION) — WARN, não bloqueia: um major
    # é "alguém olhe", e escalá-lo a bloqueio apagaria a distinção que o próprio checador faz.
    BL_RC=$(python3 "$BL_CHECKER" BACKLOG.md >/dev/null 2>&1; echo $?)
    if [ "$BL_RC" = "1" ]; then
      msg="BACKLOG.md failed its structural gate (B-051). Blockers found — the registry contradicts itself:\n$(echo "$BL_OUT" | grep BLOCKER | head -10)"
      if [ "$WARN_ONLY" = "1" ]; then WARNINGS+=("$msg"); else BLOCKERS+=("$msg"); fi
    elif [ "$BL_RC" = "3" ]; then
      WARNINGS+=("BACKLOG.md has major findings (B-051 gate):\n$(echo "$BL_OUT" | grep MAJOR | head -10)")
    fi
  fi
fi

# ----------------------------------------------------------------------------
# 2b. Secret leak (HARD GATE — cycle-review BLOCKER)
# ----------------------------------------------------------------------------
SECRET_HITS=$(echo "$ALL_FILES" \
  | grep -E '(^|/)(\.env(\.[a-z0-9_-]+)?|credentials([._-][a-z0-9]+)?|[a-z0-9_-]*secret[s]?(\.[a-z0-9_-]+)?\.(ya?ml|json|env|txt))$|\.(pem|key|p12|pfx|jks)$' \
  || true)
if [ -n "$SECRET_HITS" ]; then
  msg="Secret-pattern files appear in this session's diff (cycle-review BLOCKER). Verify they are intentionally NOT secrets, or remove them before stopping:"
  while IFS= read -r f; do
    [ -z "$f" ] && continue
    msg+="\n    - $f"
  done <<< "$SECRET_HITS"
  if [ "$WARN_ONLY" = "1" ]; then
    WARNINGS+=("$msg")
  else
    BLOCKERS+=("$msg")
  fi
fi

# ----------------------------------------------------------------------------
# 3. README.md production claims
# ----------------------------------------------------------------------------
if echo "$ALL_FILES" | grep -qE '(^|/)README\.md$'; then
  README_DIFF=$(git diff -- '*README.md' 2>/dev/null || true)
  if echo "$README_DIFF" | grep -qiE '^\+.*\bproduction[[:space:]]?-?[[:space:]]?(ready|grade)\b'; then
    WARNINGS+=("README.md introduces a 'production-ready' claim. Until v1.0 with measured evidence, prefer 'designed for' or 'targeted at' framings ($ECO/rules/public-copy.md).")
  fi
  if echo "$README_DIFF" | grep -qiE '^\+.*\b(99\.9|99\.95|99\.99)[[:space:]]?%[[:space:]]?(uptime|sla)'; then
    WARNINGS+=("README.md introduces a specific SLA/uptime number. Per the honesty rule, specific SLAs require sustained production measurement. Remove or qualify with 'target SLO' / 'designed to support'.")
  fi
fi

# ----------------------------------------------------------------------------
# Report
# ----------------------------------------------------------------------------
if [ ${#BLOCKERS[@]} -gt 0 ]; then
  echo "============================================" >&2
  echo "  STOP VALIDATION — HARD-GATE VIOLATION" >&2
  echo "============================================" >&2
  echo "" >&2
  for b in "${BLOCKERS[@]}"; do
    echo -e "  [BLOCK] $b" >&2
    echo "" >&2
  done
  echo "--------------------------------------------" >&2
  echo "Resolve every BLOCK above before stopping. To override for a documented reason, re-run with STOP_VALIDATION_WARN_ONLY=1." >&2
fi

if [ ${#WARNINGS[@]} -gt 0 ]; then
  echo "============================================"
  echo "  STOP VALIDATION — ADVISORY WARNINGS"
  echo "============================================"
  echo ""
  for w in "${WARNINGS[@]}"; do
    echo -e "  [WARN] $w"
    echo ""
  done
  echo "--------------------------------------------"
  echo "These are advisory (warn-first). Address them or document why they are intentional before considering the session complete."
fi

if [ ${#BLOCKERS[@]} -gt 0 ]; then
  exit 2
fi

exit 0
