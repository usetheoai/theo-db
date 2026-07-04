# SEPA — Initial brief (m46) — 2026-07-04

## Achados (MODE=TIGHT)

- [CRITICAL] 2 de 3 RED tests faltam. Só `decode_neighbors_into_matches_original` existe (linha ~687).
  `test_traverse_presize_is_recall_neutral` (byte-exato + pages_read idêntico = contrato recall-neutro) e
  `test_traverse_ef_zero_is_clamped` ausentes. Código de produção já escrito SEM teste falhando provando
  recall-neutralidade → TDD invertido. HALT commit até ambos escritos e rodados.
- [MAJOR] Baseline do teste-âncora: mudança já aplicada, não dá p/ capturar baseline pré-mudança da árvore
  atual. O teste precisa derivar a ordem esperada INDEPENDENTEMENTE (brute-force kNN exato sobre corpus(),
  ou golden vector do rev pré-mudança 66c05b9) — não do binário mutado.
- [MAJOR] pages_read é log-only (`pgrx::log!` linha ~596), não observável a um teste. Precisa de seam OU
  asserir ordem de outra forma.
- [MAJOR] ROADMAP.md modificado no working tree; T2.1 Files-to-edit = só hnsw_page.rs. ROADMAP é T3.1/global.
  Não empacotar no commit da T2.1.
- [MAJOR] CHANGELOG.md não modificado; Regra 6 exige [Unreleased] antes do commit.
- [INFO] Diff de produção casa com o plano e está em escopo (decode_neighbors_into limpa scratch; neighbors_into;
  pre-size cap=ef*m0, visited=cap*2, cands=cap, result=ef+1; scratch reusado). Sem dead code.
- [INFO] REFACTOR binding presente: `cap = ef.saturating_mul(m0.max(1)).max(1)` com âncora pgvector citada.
  saturating_mul cobre EC-3 (overflow).

## Ação recomendada
HALT commit. Escrever os 2 testes faltantes primeiro (ef_zero_clamp + presize_recall_neutral com ordem
derivada independente por brute-force exato). Adicionar seam p/ pages_read ou asserir ordem de outro modo.
Manter ROADMAP/CHANGELOG fora do commit de código T2.1 (CHANGELOG atualizar; ROADMAP flip é T3.1/global).
Rodar `cargo pgrx test` com os 3 testes antes de declarar GREEN.
