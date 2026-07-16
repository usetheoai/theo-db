# M99 D1 — columnar MVCC isolation permutation harness. CI does not run `cargo pgrx test`, so this is a
# standalone harness (Citus-style) run on the build droplet against a temp instance of the pgrx-managed
# PostgreSQL (which has the theodb_rs extension installed). Regenerate the extension first with
# `cargo pgrx install` (or a `cargo pgrx test` run) so `pg_isolation_regress --temp-instance` picks up the
# current .so.
check-isolation:
	bash run.sh

# Durability crash-recovery proofs (issues #46 / #47 — the ADR-0014 "Prova pendente"). Each spins a real cluster,
# induces a REAL crash (SIGABRT via the M48 test-crash GUCs / standby promotion), and asserts consistency after
# WAL replay. Run on the build droplet after `cargo pgrx install`.
check-crash:
	bash crash.sh          # #M99 — committed columnar stripe survives crash + WAL replay
	bash crash_fold.sh     # #47  — VACUUM meta-pivot crash-safe across all fold phases (never silently wrong)
	bash crash_unlogged.sh # #46  — UNLOGGED index survives standby promotion (INIT fork WAL-logged)

.PHONY: check-isolation check-crash
