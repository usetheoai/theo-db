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
	bash crash_parquet.sh  # M146 — export Parquet publicado sobrevive a crash imediato integro e legivel

# M146 (#168/#177) — prova de injeção de corrupção: nenhuma corrupção do índice pode derrubar o backend; toda
# corrupção detectada tem de sair como erro SQL tipado. O review pass-2 (F2-test-4) apontou que o script não
# tinha CHAMADOR nenhum — a prova existia e ninguém a executava. `CHECKSUMS=off` é obrigatório aqui: com o
# default do PG 18 (checksums ligados) a página é rejeitada pela verificação do PostgreSQL ANTES do nosso
# decodificador, e o script sai 2 (INCONCLUSIVE) justamente para não fingir cobertura.
# Varredura ampla o bastante para alcançar o decodificador nos dois AMs (medido: ours=8 no HNSW).
CORRUPT_OFFSETS := $(shell for p in 3 4 5 6 7 8 9 10; do for d in 2000 3000 4000 5000 5500 6200 7000 7600; do echo $$(($$p*8192+$$d)); done; done)

check-corrupt:
	CHECKSUMS=off OFFSETS="$(CORRUPT_OFFSETS)" bash corrupt_index.sh
	AM=theodb_ivfflat AM_OPTS="WITH (lists=8, pq_subspaces=4, aq_threshold=2000, separate_storage=1)" \
	  CHECKSUMS=off OFFSETS="$(CORRUPT_OFFSETS)" bash corrupt_index.sh

# M148 (#181) — compatibilidade de bootstrap pgvector. Um harness sem chamador é uma prova que ninguém
# executa (a lição registrada acima sobre o corrupt_index.sh); este alvo existe para não repetir isso.
check-compat:
	bash pgvector_compat_check.sh

# #190 — INSERT no colunar com valores TOAST. RED enquanto o defeito existir.
check-toast:
	bash columnar_toast_check.sh

.PHONY: check-isolation check-crash check-corrupt check-compat check-toast
