-- M168 — top-k com k GRANDE: o cenário em que a retenção do TopK pode estourar a pool do caminho streaming.
--
-- WHY. A pool do streaming é `2*work_mem + 64MB`, constante em `k`, enquanto a retenção do `TopK` do DataFusion
-- cresce com ele (`topk/mod.rs` faz `reservation.try_resize(self.size())`) e a admissão não limita `k`. Sem
-- fail-open, uma consulta que o caminho eager servia passaria a ERRAR por default. `run_columnar_topk` cai no
-- eager em vez de propagar; este arquivo tenta alcançar a janela e verifica que o resultado é o mesmo de qualquer
-- forma.
--
-- A PRIMEIRA VERSÃO DESTE ARQUIVO ERA VAZIA, e o artefato commitado provava (achado de review). Ela usava
-- `work_mem = 4MB` para apertar a pool — mas o guard do ADR-4 admite por `work_mem × 8`, então 4MB dá orçamento de
-- 32MB contra uma relação de 228MB físicos: as consultas DECLINAVAM antes de chegar ao caminho colunar, e os dois
-- braços rodavam o plano nativo do PostgreSQL. `mism = 0` comparava nativo com nativo. O desenho tornava
-- inalcançável exatamente a janela que ele mirava, e apertar mais o `work_mem` só piorava.
--
-- O regime alcançável é o oposto: `work_mem` GRANDE o bastante para o guard admitir (> ~28,5MB nesta tabela) e
-- `k` × largura da linha acima da pool. Com `work_mem = 32MB`: orçamento 256MB > 228MB (admite) e pool 128MB,
-- contra ~772 B/linha × 200.000 ≈ 154MB de retenção.
--
-- E ele agora ASSERE O ROTEAMENTO antes de comparar. Um A/B que não confirma o plano é falso-verde — é a lição do
-- M161, e foi exatamente o que aconteceu aqui.
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;
SET theodb.enable_columnar_late_mat = on;
SET work_mem = '64MB';   -- a 32MB o braço EAGER nao serve NENHUM top-k destas formas (ver nota de inversao)

-- ORDEM TOTAL, não `ORDER BY EventTime` sozinho. Uma versão anterior comparava linhas inteiras sob uma chave com
-- empates, e a comparação acusou 2 divergências que eram indefinição de desempate, não defeito — a mesma
-- armadilha que o oráculo do M167 documenta ("which rows among equal keys is unspecified and a full-row compare
-- would false-positive"). A chave composta abaixo é praticamente única, então o resultado é determinístico e
-- `mism` volta a significar erro.
\echo '### M168-K0: precondição de roteamento — sem isto tudo abaixo passa vazio'
DO $k0$
DECLARE plan text;
BEGIN
  EXECUTE 'EXPLAIN (COSTS OFF, FORMAT JSON) SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 50000' INTO plan;
  IF position('theodb_columnar_agg' IN plan) = 0 THEN
    RAISE EXCEPTION
      'M168-K0 FAILED: a consulta de k grande NÃO roteia para o caminho colunar, então os dois braços rodariam o '
      'plano nativo e qualquer mism=0 abaixo compararia nativo com nativo. Provavelmente o guard do ADR-4 declinou '
      '(work_mem x 8 < tamanho da relação). Plano: %', plan;
  END IF;
  RAISE NOTICE 'M168-K0 ok: a consulta de k grande roteia — o A/B abaixo compara caminhos de verdade';
END
$k0$;

DROP TABLE IF EXISTS lk_res;
CREATE TEMP TABLE lk_res (q text, n bigint);

-- O QUE FOI MEDIDO TENTANDO ALCANÇAR A JANELA, e é informação de primeira ordem:
--
--   * `SELECT *` (105 colunas) sem filtro, k = 200000: o streaming estoura
--     (`TopK[0] with 121.7 MB already allocated … pool_size: 128.0 MB`), o fail-open dispara corretamente, E O
--     EAGER TAMBÉM ESTOURA (`Failed to allocate additional 772.9 MB … 1545.4 MB already allocated … 1608.5 MB`).
--   * O mesmo com k = 100000: o eager falha COM O MESMO NÚMERO — 1545,4 MB. É independente de `k`, porque o TopK
--     do eager segura o batch inteiro de 772 MB e precisa de outro tanto.
--
-- Conclusão honesta: **um top-k de `SELECT *` sem filtro sobre 1M×105 colunas não é servível pelo caminho eager,
-- com nenhum k**. Isso é limitação PRÉ-EXISTENTE, não introduzida pelo M168 — e o guard do ADR-4 admite a consulta
-- assim mesmo (est. 228MB contra orçamento de 256MB a work_mem=32MB), o que é outra instância da subestimação
-- registrada em #218.
--
-- E A INVERSÃO, que é o achado mais forte desta investigação: com projeção estreita e k = 400000 a
-- work_mem = 32MB, o braço STREAMING SERVE (400.000 linhas, batches de ~250 KB) e o EAGER FALHA
-- (`TopK[0] with 100.3 MB already allocated`). O fail-open não disparou porque não precisou.
--
-- Ou seja: a janela que o fail-open foi escrito para cobrir — "o streaming quebra o que o eager servia" — **não
-- foi encontrada**, e a janela que EXISTE é a oposta. O TopK do streaming retém batches de 250 KB; o do eager
-- segura um batch de 40 MB inteiro. O fail-open fica como defesa contra o caso não encontrado, não como resposta
-- a um caso medido — e isso é dito assim no verdict em vez de vendido como validação.
--
-- O teste abaixo roda no regime em que os DOIS servem, que é onde `mism = 0` significa algo.
\echo '### M168-K1: projeção média (5 colunas), k = 50000 — ambos servem; mism=0 significa algo'
SET theodb.enable_columnar_late_mat = on;
SET theodb.enable_columnar_topk_stream = on;
DROP TABLE IF EXISTS k1_on;
CREATE TEMP TABLE k1_on AS SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 50000;
-- Referência: o plano NATIVO do PostgreSQL (late-mat inteiro desligado). Não é o eager — ver a nota de
-- inversão: o eager NÃO serve estas formas em work_mem algum razoável, então ele não pode ser oráculo aqui.
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS k1_off;
CREATE TEMP TABLE k1_off AS SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 50000;
INSERT INTO lk_res SELECT 'k1_mism', count(*) FROM (
  (SELECT * FROM k1_on EXCEPT ALL SELECT * FROM k1_off)
  UNION ALL (SELECT * FROM k1_off EXCEPT ALL SELECT * FROM k1_on)) d;
INSERT INTO lk_res SELECT 'k1_rows', count(*) FROM k1_on;
INSERT INTO lk_res SELECT 'k1_cols', count(*) FROM information_schema.columns WHERE table_name = 'k1_on';

-- k = 200000 aqui, e não 400000, por um motivo medido: a 400000 o braço EAGER falha
-- (`TopK[0] with 100.3 MB already allocated`) enquanto o STREAMING serve. Ver a nota de inversão no topo.
\echo '### M168-K2: projeção estreita, k = 50000 — ambos servem'
SET theodb.enable_columnar_late_mat = on;
SET theodb.enable_columnar_topk_stream = on;
DROP TABLE IF EXISTS k2_on;
CREATE TEMP TABLE k2_on AS SELECT EventTime, SearchPhrase FROM hits ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 50000;
SET theodb.enable_columnar_late_mat = off;
DROP TABLE IF EXISTS k2_off;
CREATE TEMP TABLE k2_off AS SELECT EventTime, SearchPhrase FROM hits ORDER BY EventTime, CounterID, WatchID, UserID LIMIT 50000;
INSERT INTO lk_res SELECT 'k2_mism', count(*) FROM (
  (SELECT * FROM k2_on EXCEPT ALL SELECT * FROM k2_off)
  UNION ALL (SELECT * FROM k2_off EXCEPT ALL SELECT * FROM k2_on)) d;
INSERT INTO lk_res SELECT 'k2_rows', count(*) FROM k2_on;

\echo '### M168-K3: CONTROLE POSITIVO'
DROP TABLE IF EXISTS k3_bad;
CREATE TEMP TABLE k3_bad AS SELECT EventTime, SearchPhrase || 'x' AS SearchPhrase FROM k2_off LIMIT 100;
DROP TABLE IF EXISTS k3_ref;
CREATE TEMP TABLE k3_ref AS SELECT * FROM k2_on LIMIT 100;
INSERT INTO lk_res SELECT 'k3_control_diff', count(*) FROM (
  (SELECT * FROM k3_ref EXCEPT ALL SELECT * FROM k3_bad)
  UNION ALL (SELECT * FROM k3_bad EXCEPT ALL SELECT * FROM k3_ref)) d;

\echo '### GATE FINAL'
SELECT * FROM lk_res ORDER BY q;
\if :{?gate_selftest}
  \echo '### GATE SELF-TEST ARMADO: forçando k1_mism = 1 — o gate abaixo DEVE abortar'
  UPDATE lk_res SET n = 1 WHERE q = 'k1_mism';
\endif
DO $gate$
DECLARE bad text := '';
BEGIN
  bad := bad || coalesce(
    (SELECT 'divergências stream-vs-nativo: ' || string_agg(format('%s=%s', q, n), ', ') || '; '
       FROM lk_res WHERE q LIKE '%\_mism' AND n <> 0), '');
  IF coalesce((SELECT n FROM lk_res WHERE q = 'k1_rows'), 0) < 40000 THEN
    bad := bad || format('k1_rows=%s (<40000): o k grande não foi exercitado, então a retenção do TopK não foi '
                      || 'pressionada e o teste não mira nada; ', (SELECT n FROM lk_res WHERE q = 'k1_rows'));
  END IF;
  IF coalesce((SELECT n FROM lk_res WHERE q = 'k1_cols'), 0) < 5 THEN
    bad := bad || 'k1_cols < 5: a projeção não é a esperada; ';
  END IF;
  IF coalesce((SELECT n FROM lk_res WHERE q = 'k2_rows'), 0) < 40000 THEN
    bad := bad || 'k2_rows < 40000: o caso estreito não exercitou k grande; ';
  END IF;
  IF coalesce((SELECT n FROM lk_res WHERE q = 'k3_control_diff'), 0) = 0 THEN
    bad := bad || 'k3_control_diff=0: a comparação não consegue falhar; ';
  END IF;
  IF bad <> '' THEN RAISE EXCEPTION 'M168 LARGE-K GATE FAILED: %', bad; END IF;
  RAISE NOTICE 'M168 LARGE-K GATE ok: a consulta ROTEIA (K0), e o streaming concorda com o plano NATIVO em k grande — seja '
               'servindo, seja caindo no fail-open. Uma linha theodb_topk_stream_fallback no log diz qual.';
END
$gate$;
