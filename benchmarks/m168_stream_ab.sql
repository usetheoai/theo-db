-- M168 — PAIRED A/B: streaming chunk-group decode vs the eager whole-relation decode.
--
-- WHY PAIRED, IN ONE SESSION, ON ONE BINARY. This box drifts up to 1.88x between runs on sub-200 ms queries
-- (M167 § 6, measured on queries the GUC provably cannot affect). A cross-run comparison therefore cannot license
-- a throughput claim, whatever it reports — so `theodb.enable_columnar_topk_stream` exists precisely to make the
-- toggle the only asymmetry, exactly as `enable_columnar_late_mat` did for the M167 headline.
--
-- OS BRAÇOS ALTERNAM DE VERDADE. A primeira versão deste arquivo trazia esta mesma frase enquanto o código rodava
-- SEMPRE eager-depois-stream nas 20 iterações (achado de review). O drift é grande e monotônico — o q23 eager caiu
-- 6008 -> 3977 ms ao longo dos 5 pares, 1,51x — e sempre era pago pelo braço que ia primeiro. Agora a ordem
-- inverte a cada par, então o efeito de ordem cancela em vez de se somar a um dos lados.
--
-- O NÚMERO DE PARES É PAR (6), e isso não é estética. Com 5 pares e ordem alternada, o eager vai primeiro em
-- 3 deles — incluindo o par 1, o mais frio, onde o drift é maior. O contrabalanceamento fica incompleto e o viés
-- residual empurra o ganho para cima e a regressão para baixo (achado de review).
-- CTAS materializes the k surviving rows: a `count(*)` wrapper lets PostgreSQL skip forming them, which erased the
-- effect entirely in a M167 draft (verdict § 7.3).
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;
SET theodb.enable_columnar_late_mat = on;
SET work_mem = '64MB';

DROP TABLE IF EXISTS ab_res;
CREATE TEMP TABLE ab_res (pair int, q text, mode text, ms double precision);

DO $ab$
DECLARE
  t0 timestamptz;
  i  int;
  j  int;
  qs text[] := ARRAY[
    $q$SELECT * FROM hits WHERE URL LIKE '%google%' ORDER BY EventTime LIMIT 10$q$,
    $q$SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime LIMIT 10$q$,
    $q$SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY SearchPhrase LIMIT 10$q$,
    $q$SELECT SearchPhrase FROM hits WHERE SearchPhrase <> '' ORDER BY EventTime, SearchPhrase LIMIT 10$q$
  ];
  qn text[] := ARRAY['q23','q24','q25','q26'];
  modes text[];
  m text;
BEGIN
  FOR i IN 1..6 LOOP
    FOR j IN 1..4 LOOP
      -- Ordem invertida em pares alternados: nos ímpares eager vai primeiro, nos pares stream vai primeiro.
      IF i % 2 = 1 THEN
        modes := ARRAY['eager','stream'];
      ELSE
        modes := ARRAY['stream','eager'];
      END IF;
      FOREACH m IN ARRAY modes LOOP
        EXECUTE format('SET theodb.enable_columnar_topk_stream = %s', CASE WHEN m='stream' THEN 'on' ELSE 'off' END);
        t0 := clock_timestamp();
        EXECUTE 'CREATE TEMP TABLE _ab_t AS ' || qs[j];
        INSERT INTO ab_res VALUES (i, qn[j], m, extract(epoch from clock_timestamp()-t0)*1000);
        EXECUTE 'DROP TABLE _ab_t';
      END LOOP;
    END LOOP;
  END LOOP;
END
$ab$;

\echo '### M168 paired A/B — median of 6 pairs per query (ratio > 1 = streaming is SLOWER)'
SELECT q,
       round(percentile_cont(0.5) WITHIN GROUP (ORDER BY ms) FILTER (WHERE mode='eager')::numeric, 1)  AS eager_ms,
       round(percentile_cont(0.5) WITHIN GROUP (ORDER BY ms) FILTER (WHERE mode='stream')::numeric, 1) AS stream_ms,
       round((percentile_cont(0.5) WITHIN GROUP (ORDER BY ms) FILTER (WHERE mode='stream')
            / percentile_cont(0.5) WITHIN GROUP (ORDER BY ms) FILTER (WHERE mode='eager'))::numeric, 3) AS ratio,
       count(*) FILTER (WHERE mode='stream') AS pairs
FROM ab_res GROUP BY q ORDER BY q;

\echo '### per-pair detail (so a single outlier cannot hide behind a median)'
SELECT q,
       string_agg(round(ms::numeric,1)::text, ' ' ORDER BY pair) FILTER (WHERE mode='eager')  AS eager_ms,
       string_agg(round(ms::numeric,1)::text, ' ' ORDER BY pair) FILTER (WHERE mode='stream') AS stream_ms
FROM ab_res GROUP BY q ORDER BY q;
