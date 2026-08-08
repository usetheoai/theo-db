-- theodb_rs 1.3.0 → 1.4.0
--
-- Remove o access method `theodb_symqg` (E2 / SymphonyQG) da distribuição.
--
-- MOTIVO, medido: o índice estava registrado no binário default sem feature flag — um usuário podia
-- escrever `USING theodb_symqg` e receber, sem aviso, um índice medido como 3,5× mais lento no build
-- (`wiki/benchmarks/m184-pilares-superficie-medida-verdict.md`) e 2,6–3,9× mais lento na busca a recall
-- casado (`wiki/benchmarks/e2-symqg-inpg-verdict.md`). O perfil localizou os gargalos — `ambuild_symqg`
-- com 39% do build e `gather_symqg_candidates` com 18% da busca —, e o owner decidiu remover em vez de
-- esconder atrás de flag (2026-08-08).
--
-- DESTRUTIVO E DECLARADO: um índice existente criado com `USING theodb_symqg` bloqueia este DROP. É
-- deliberado — falhar alto é melhor que derrubar índice de usuário em silêncio (Regra 8). O operador
-- deve dropar o índice e recriá-lo com `theodb_hnsw`, que é o caminho recomendado e mais rápido nos
-- dois regimes.

DROP OPERATOR CLASS IF EXISTS theodb_symqg_l2_ops USING theodb_symqg;
DROP ACCESS METHOD IF EXISTS theodb_symqg;
DROP FUNCTION IF EXISTS theodb_symqg_amhandler(internal);
DROP FUNCTION IF EXISTS symqg_spike_bench(text, bigint, bigint, int);
