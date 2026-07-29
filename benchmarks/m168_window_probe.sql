-- M168 § 3.5 — a busca pela janela do fail-open, como artefato em vez de relato.
--
-- WHY. Uma versão anterior do verdict publicava quatro linhas de tabela sob o título "Três conclusões, todas
-- medidas", e três delas não existiam em artefato nenhum (achado de review — o mesmo defeito pelo qual a § 3 já
-- tinha sido corrigida, deslocado uma seção adiante). Este arquivo executa exatamente aqueles quatro cenários e
-- deixa o log falar.
--
-- A pergunta: existe um `k` em que o caminho EAGER serve e o STREAMING não? É a premissa do fail-open. Cada bloco
-- roda a mesma consulta nos dois braços e reporta se cada um serviu ou estourou.
--
-- REQUER `THEODB_ADMIT_TRACE=1` no ambiente do postmaster.
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;
SET theodb.enable_columnar_late_mat = on;

\set ON_ERROR_STOP off
-- ON_ERROR_STOP fica OFF de propósito: o ponto deste arquivo é registrar QUAIS combinações estouram. Um erro aqui
-- é o dado, não uma falha do teste.

DROP TABLE IF EXISTS win_res;
CREATE TABLE win_res (cenario text, braco text, linhas bigint, erro text);

\echo '################ CENÁRIO 1: work_mem=32MB · SELECT * (105 col) · k=200000 ################'
SET work_mem = '32MB';
SET theodb.enable_columnar_topk_stream = on;
\echo '--- braço STREAMING ---'
DROP TABLE IF EXISTS w1a;
CREATE TEMP TABLE w1a AS SELECT * FROM hits ORDER BY EventTime, CounterID, WatchID LIMIT 200000;
INSERT INTO win_res SELECT 'c1_wide_k200k', 'stream', count(*), NULL FROM w1a;
SET theodb.enable_columnar_topk_stream = off;
\echo '--- braço EAGER ---'
DROP TABLE IF EXISTS w1b;
CREATE TEMP TABLE w1b AS SELECT * FROM hits ORDER BY EventTime, CounterID, WatchID LIMIT 200000;
INSERT INTO win_res SELECT 'c1_wide_k200k', 'eager', count(*), NULL FROM w1b;

\echo '################ CENÁRIO 2: work_mem=32MB · SELECT * (105 col) · k=100000 ################'
SET theodb.enable_columnar_topk_stream = on;
\echo '--- braço STREAMING ---'
DROP TABLE IF EXISTS w2a;
CREATE TEMP TABLE w2a AS SELECT * FROM hits ORDER BY EventTime, CounterID, WatchID LIMIT 100000;
INSERT INTO win_res SELECT 'c2_wide_k100k', 'stream', count(*), NULL FROM w2a;
SET theodb.enable_columnar_topk_stream = off;
\echo '--- braço EAGER ---'
DROP TABLE IF EXISTS w2b;
CREATE TEMP TABLE w2b AS SELECT * FROM hits ORDER BY EventTime, CounterID, WatchID LIMIT 100000;
INSERT INTO win_res SELECT 'c2_wide_k100k', 'eager', count(*), NULL FROM w2b;

\echo '################ CENÁRIO 3: work_mem=32MB · 2 colunas · k=400000 (a janela INVERSA) ################'
SET theodb.enable_columnar_topk_stream = on;
\echo '--- braço STREAMING ---'
DROP TABLE IF EXISTS w3a;
CREATE TEMP TABLE w3a AS SELECT EventTime, SearchPhrase FROM hits ORDER BY EventTime, SearchPhrase LIMIT 400000;
INSERT INTO win_res SELECT 'c3_narrow_k400k', 'stream', count(*), NULL FROM w3a;
SET theodb.enable_columnar_topk_stream = off;
\echo '--- braço EAGER ---'
DROP TABLE IF EXISTS w3b;
CREATE TEMP TABLE w3b AS SELECT EventTime, SearchPhrase FROM hits ORDER BY EventTime, SearchPhrase LIMIT 400000;
INSERT INTO win_res SELECT 'c3_narrow_k400k', 'eager', count(*), NULL FROM w3b;

\echo '################ CENÁRIO 4: work_mem=64MB · 5 colunas · k=50000 ################'
SET work_mem = '64MB';
SET theodb.enable_columnar_topk_stream = on;
\echo '--- braço STREAMING ---'
DROP TABLE IF EXISTS w4a;
CREATE TEMP TABLE w4a AS SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits
  ORDER BY EventTime, CounterID, WatchID LIMIT 50000;
INSERT INTO win_res SELECT 'c4_med_k50k', 'stream', count(*), NULL FROM w4a;
SET theodb.enable_columnar_topk_stream = off;
\echo '--- braço EAGER ---'
DROP TABLE IF EXISTS w4b;
CREATE TEMP TABLE w4b AS SELECT EventTime, CounterID, WatchID, UserID, SearchPhrase FROM hits
  ORDER BY EventTime, CounterID, WatchID LIMIT 50000;
INSERT INTO win_res SELECT 'c4_med_k50k', 'eager', count(*), NULL FROM w4b;

\echo '################ RESUMO — uma linha ausente significa que aquele braço ESTOUROU ################'
SELECT cenario, braco, linhas FROM win_res ORDER BY cenario, braco;
\echo ''
\echo 'A janela do fail-open existiria num cenário com linha para "eager" e SEM linha para "stream".'
\echo 'A janela inversa é um cenário com linha para "stream" e SEM linha para "eager".'
DROP TABLE win_res;
