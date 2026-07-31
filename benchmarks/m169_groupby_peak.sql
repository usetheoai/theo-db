-- M169 T3.2 — o pico de memória do GROUP BY de alta cardinalidade, que o streaming NÃO reduz.
--
-- Por que existe separado do gate de streaming: o M169 remove o pico do DECODE (a relação inteira num
-- `RecordBatch`). Ele NÃO toca o outro pico — a tabela de hash do agregado, que é O(grupos distintos) e independe
-- de quantas linhas chegam por vez. Prometer que o streaming conserta `GROUP BY WatchID, ClientIP` seria vender o
-- que não acontece; medir é a única forma de dizer ONDE está a linha.
--
-- MEDIDO no baseline de 100M (2026-07-31): a q32 (`GROUP BY WatchID, ClientIP`) ROTEIA (`agg_routed=true`) e
-- mesmo assim estoura o teto de 300 s — falha de ESTADO. As q33/q34 (`GROUP BY URL`) falhavam por OFFSETS, e são
-- o que o T2.1 endereça. Duas causas, um sintoma parecido.
--
-- REQUER `THEODB_ADMIT_TRACE=1` no ambiente do POSTMASTER (não da sessão): sem ele a linha `theodb_stream_pool`
-- não é emitida e esta corrida não mede nada. Quem roda este arquivo faz grep por `peak_reserved=` na saída; zero
-- ocorrências significa medição inválida, não "o pico foi baixo".
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;
SET theodb.enable_columnar_agg_stream = on;
SET theodb.enable_columnar_late_mat = on;
SET work_mem = '64MB';   -- pool = work_mem*2 + 64 MiB = 192 MiB; um teto BAIXO de propósito, para a linha aparecer

DO $gate$
BEGIN
  IF NOT current_setting('theodb.enable_columnar_agg_stream')::bool THEN
    RAISE EXCEPTION 'M169 T3.2: enable_columnar_agg_stream está OFF — o pico medido seria o do caminho EAGER, '
                    'que é o que este milestone substituiu. Medição inválida.';
  END IF;
END
$gate$;

DROP TABLE IF EXISTS t_peak CASCADE;
CREATE TABLE t_peak (g bigint, k1 bigint, k2 bigint) USING theodb_columnar;
INSERT INTO t_peak SELECT g, g, g FROM generate_series(1, 2000000) g;   -- 200 chunk-groups

\echo '### T3.2-A — AggregateMode do plano: Single ou Partial?'
-- Decide o modo de OOM do DataFusion: `Single` só pode fazer spill; `Partial` pode emitir cedo e deixar o merge
-- para o nó acima. Sem saber qual é, "o spill basta?" não tem resposta — são políticas com tetos diferentes.
EXPLAIN (COSTS OFF, VERBOSE) SELECT k1, count(*) FROM t_peak GROUP BY k1;

\echo '### T3.2-B — varredura de cardinalidade: onde o pico cruza o teto da pool'
-- `\o /dev/null` descarta o RESULTADO sem tocar o plano. Um `LIMIT`/`OFFSET` para conter a saída acrescentaria um
-- nó acima do agregado e poderia mudar a decisão de roteamento — aí o pico medido seria o de outro plano.
\o /dev/null
SELECT k1 % 1000,    count(*) FROM t_peak GROUP BY 1;
SELECT k1 % 100000,  count(*) FROM t_peak GROUP BY 1;
SELECT k1 % 1000000, count(*) FROM t_peak GROUP BY 1;
SELECT k1,           count(*) FROM t_peak GROUP BY 1;
\echo '### T3.2-C — a forma da q32: DUAS chaves, cardinalidade quase-única'
SELECT k1, k2, count(*) FROM t_peak GROUP BY k1, k2;
\o

\echo '### T3.2 — leitura'
DO $gate$
BEGIN
  RAISE NOTICE 'M169 T3.2: 5 agregações emitidas com cardinalidades 1e3 / 1e5 / 1e6 / 2e6 / 2e6-duas-chaves. O '
               'pico de cada uma está nas linhas WARNING `theodb_stream_pool: peak_reserved=…` acima, em BYTES. '
               'ZERO dessas linhas = THEODB_ADMIT_TRACE ausente no postmaster = corrida inválida, não pico baixo.';
END
$gate$;
