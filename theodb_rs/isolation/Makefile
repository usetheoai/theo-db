# M99 D1 — columnar MVCC isolation permutation harness. CI does not run `cargo pgrx test`, so this is a
# standalone harness (Citus-style) run on the build droplet against a temp instance of the pgrx-managed
# PostgreSQL (which has the theodb_rs extension installed). Regenerate the extension first with
# `cargo pgrx install` (or a `cargo pgrx test` run) so `pg_isolation_regress --temp-instance` picks up the
# current .so.
check-isolation:
	bash run.sh

.PHONY: check-isolation
