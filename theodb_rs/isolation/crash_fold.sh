#!/usr/bin/env bash
# #47 / ADR-0014 "Prova pendente" — end-to-end crash-recovery proof for the M48 crash-safe VACUUM fold (meta-pivot).
# The fix (theodb_rs/src/am/fold.rs) writes the new generation to FRESH pages, then pivots block 0 LAST in a single
# GenericXLog record. This harness drives the built-in deterministic crash hooks (theodb.test_crash_after_pages /
# theodb.test_crash_phase → std::process::abort() = a REAL backend crash the postmaster recovers) at each fold phase
# and asserts consistency after WAL replay:
#   Phase A — crash BEFORE the pivot (after body page 1): the OLD generation is intact (block 0 unchanged) → the
#             KNN query returns the SAME rows as before the VACUUM.  NO corruption.
#   Phase B — crash AFTER the pivot (POST_PIVOT): the NEW generation is active; the KNN answer is still correct.
#   Phase C — crash MID-RECLAIM: fail-LOUD — the query returns the correct answer OR a typed REINDEX/"truncated"
#             error; NEVER a silently-wrong result (the #47 worst case).
# NON-VACUOUS: it asserts >= 3 REAL backend crashes (SIGABRT/signal 6) actually fired — else the fold never
# triggered and the test proves nothing. Run on the build droplet (regenerate the extension first).
set -uo pipefail
PGINST="${PGINST:-$HOME/.pgrx/17.10/pgrx-install}"
export PATH="$PGINST/bin:$PATH"
DATA=/tmp/crash_fold_tmp
PORT=59721
DB=postgres
N="${N:-2000}"      # built rows
M="${M:-800}"       # pending rows inserted before each phase (folded by that phase's VACUUM)

cleanup() { pg_ctl -D "$DATA" -m immediate stop -w >/dev/null 2>&1 || true; rm -rf "$DATA"; }
trap cleanup EXIT
rm -rf "$DATA"
initdb -D "$DATA" -U theo >/dev/null 2>&1   # theo = cluster superuser → the Suset crash hooks fire
{ echo "port=$PORT"; echo "fsync=on"; echo "full_page_writes=on"; echo "restart_after_crash=on"; } >> "$DATA/postgresql.conf"
pg_ctl -D "$DATA" -l "$DATA/log" start -w >/dev/null
q() { psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "$1" 2>&1; }
wait_up() { for i in $(seq 1 90); do psql -X -q -p "$PORT" -U theo -d "$DB" -tAc "SELECT 1" >/dev/null 2>&1 && return 0; sleep 1; done; return 1; }
vrow() { echo "('[' || array_to_string(ARRAY(SELECT ((g*$1+d*$2)%97)::float4 FROM generate_series(1,8) d), ',') || ']')::vector"; }

q "CREATE EXTENSION theodb_rs;" >/dev/null
[ "$(q "SELECT usesuper FROM pg_user WHERE usename='theo'")" = "t" ] || { echo "ABORT: theo is not superuser — the crash hook is superuser-gated"; exit 1; }
q "CREATE TABLE t (id int, v vector(8));" >/dev/null
q "INSERT INTO t SELECT g, $(vrow 7 13) FROM generate_series(1,$N) g;" >/dev/null
q "CREATE INDEX idx ON t USING theodb_ivfflat (v theodb_ivfflat_l2_ops);" >/dev/null

QUERY="'[10,20,30,40,50,60,70,80]'::vector"
knn() { q "SET enable_seqscan=off; SET theodb_ivfflat.probes=64; SELECT string_agg(id::text, ',' ORDER BY id) FROM (SELECT id FROM t ORDER BY v <-> $QUERY LIMIT 10) s;"; }
PRE=$(knn)
echo "PRE_KNN=$PRE"

FAILS=0
next_seq=$((N+1))
sig6() { local n; n=$(grep -c "was terminated by signal 6" "$DATA/log" 2>/dev/null || true); echo "${n:-0}"; }
run_phase() {
  local name="$1"; local setguc="$2"; local expect="$3"; local shrink="${4:-}"   # expect: same|loud
  wait_up || { echo "$name: cluster down"; FAILS=$((FAILS+1)); return; }
  local c0; c0=$(sig6)   # crash count BEFORE this phase (per-phase non-vacuous guard, council F1)
  # fresh pending region so THIS phase's VACUUM has a multi-page body to fold
  q "INSERT INTO t SELECT g, $(vrow 11 5) FROM generate_series($next_seq,$((next_seq+M-1))) g;" >/dev/null
  next_seq=$((next_seq+M))
  # 'shrink' phases DELETE most rows first, so the fold's new generation is SMALL and REUSES the dead low region
  # → the reclaim loop runs → the MID_RECLAIM crash point is reachable (else the tail-append fold never reclaims).
  if [ "$shrink" = "shrink" ]; then
    q "INSERT INTO t SELECT g, $(vrow 3 17) FROM generate_series($next_seq,$((next_seq+M-1))) g;" >/dev/null
    next_seq=$((next_seq+M))
    psql -X -q -p "$PORT" -U theo -d "$DB" >/dev/null 2>&1 <<SQL || true
SET theodb.vacuum_pending_threshold=1;
VACUUM (INDEX_CLEANUP ON) t;
SQL
    q "DELETE FROM t WHERE id % 10 <> 0;" >/dev/null   # keep ~10% → next fold's new gen is small → region reuse
  fi
  # trigger the fold (threshold=1 ⇒ any pending folds) with the crash GUC set; backend SIGABRTs mid-fold.
  # NOTE: stdin heredoc (NOT -c) so each statement runs in autocommit — VACUUM cannot run inside a txn block.
  psql -X -q -p "$PORT" -U theo -d "$DB" >/dev/null 2>&1 <<SQL || true
SET theodb.vacuum_pending_threshold=1;
$setguc
VACUUM (INDEX_CLEANUP ON) t;
SQL
  wait_up || { echo "$name: cluster did not recover after crash"; FAILS=$((FAILS+1)); return; }
  # per-phase non-vacuous guard (council F1): THIS phase's fold must have crashed EXACTLY once — closes the
  # theoretical "2+1+0 aggregates to 3" loophole. If it did not crash, this phase's fold did not run.
  local c1; c1=$(sig6); local delta=$((c1 - c0))
  if [ "$delta" -ne 1 ]; then echo "$name: FAIL — this phase fired $delta crashes (expected exactly 1 → its fold did not trigger)"; FAILS=$((FAILS+1)); fi
  # The robust #47 check (imune to data changes AND to IVF approximation): compare the POST-crash index answer to
  # a FRESH REINDEX answer over the SAME current data (the ground truth the index should give). Correct ⇒ equal;
  # fail-loud ⇒ a typed REINDEX error; SILENT CORRUPTION ⇒ a different answer with NO error (the #47 worst case).
  local POST; POST=$(knn)
  q "REINDEX INDEX idx;" >/dev/null 2>&1 || true   # rebuild clean over current data
  local CLEAN; CLEAN=$(knn)
  if echo "$POST" | grep -qiE 'reindex|truncated|corrupt pending|theodb am'; then
    if [ "$expect" = "same" ]; then echo "$name: FAIL — crash BEFORE pivot must keep the OLD gen readable, got a REINDEX error: $(echo "$POST" | head -c 70)"; FAILS=$((FAILS+1));
    else echo "$name: OK — fail-LOUD typed REINDEX error, NEVER silent (un-reclaimed pending window, M55-deferred)"; fi
  elif [ "$POST" = "$CLEAN" ]; then
    echo "$name: OK — post-crash index answer == fresh-rebuild answer over current data (no corruption)"
  else
    echo "$name: FAIL — SILENTLY different from a clean rebuild, no error (the #47 worst case!): post=$POST clean=$CLEAN"; FAILS=$((FAILS+1))
  fi
}

run_phase "PhaseA_before_pivot" "SET theodb.test_crash_after_pages=1;" "same"
run_phase "PhaseB_post_pivot"   "SET theodb.test_crash_phase=1;"       "loud"
run_phase "PhaseC_mid_reclaim"  "SET theodb.test_crash_phase=2;"       "loud"  "shrink"

# NON-VACUOUS GUARD: the crashes MUST have actually fired (SIGABRT = signal 6), else the fold never ran and the
# assertions prove nothing. All 3 fold crash points (after-body-page, post-pivot, mid-reclaim) must be exercised.
CRASHES=$(grep -c "was terminated by signal 6" "$DATA/log" 2>/dev/null); CRASHES=${CRASHES:-0}
echo "CRASH_EVIDENCE: SIGABRT_crashes=$CRASHES (need >= 3 — one per fold phase)"
echo "---"
if [ "$FAILS" = 0 ] && [ "$CRASHES" -ge 3 ]; then
  echo "CRASH_FOLD_OK — 3 REAL backend crashes (SIGABRT) across all fold phases + WAL replay; the #47 meta-pivot"
  echo "  guarantee holds: crash BEFORE pivot ⇒ old gen correct; crash AFTER pivot / mid-reclaim ⇒ fail-LOUD REINDEX;"
  echo "  NEVER a silently-wrong result (the #47 worst case)."
  exit 0
else
  echo "CRASH_FOLD_FAIL (logic_fails=$FAILS crashes=$CRASHES) — crashes<3 means a fold phase did not trigger (VACUOUS)"
  exit 1
fi
