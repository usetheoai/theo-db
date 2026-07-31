-- M169 T3.1 — a soma de float8 é ASSOCIATIVA entre o braço eager e o streaming?
--
-- Por que esta pergunta existe: streaming muda a ORDEM em que os valores são acumulados (um chunk-group por vez
-- em vez de um batch único), e adição em IEEE-754 não é associativa. Se `sum(float8)` divergir, o M169 troca um
-- defeito (o overflow de offsets) por outro (resultado dependente do tamanho do chunk-group) — e o segundo é pior,
-- porque é silencioso.
--
-- O ClickBench NÃO responde isto: todas as suas colunas de SUM/AVG são inteiras (AdvEngineID, IsRefresh e
-- ResolutionWidth são SMALLINT; UserID é BIGINT). O espaço de dados do benchmark é cego ao espaço de tipos —
-- é exatamente o que `testing.md § 5.1` diz e a razão de este arquivo existir.
--
-- O dado é o PIOR CASO deliberado: 0.1 repetido (não representável em binário) com um 1e17 esparso. Somar 1e17
-- primeiro faz cada 0.1 seguinte desaparecer no arredondamento; somar os 0.1 primeiro preserva a soma parcial.
-- Se houver divergência de ordem, esta forma a expõe; um dado de magnitude uniforme a esconderia.
\set ON_ERROR_STOP on
SET max_parallel_workers_per_gather = 0;
SET theodb.enable_columnar_agg = on;
SET theodb.enable_columnar_late_mat = on;
SET work_mem = '64MB';
-- `::text` de float8 no PG >= 12 é a representação MAIS CURTA que faz round-trip exato, então comparar texto
-- compara os bits. Arredondar (`round(x::numeric, 9)`) mascararia justamente a divergência procurada — era o
-- defeito do meu próprio RED antes desta medição.
SET extra_float_digits = 3;

DROP TABLE IF EXISTS t_fassoc CASCADE;
CREATE TABLE t_fassoc (g bigint, f float8) USING theodb_columnar;
INSERT INTO t_fassoc
SELECT g, CASE WHEN g % 10000 = 0 THEN 1e17 ELSE 0.1 END
FROM generate_series(1, 25000) g;   -- 3 chunk-groups, o último parcial

DROP TABLE IF EXISTS fassoc_res;
CREATE TEMP TABLE fassoc_res (arm text, metric text, val text);

SET theodb.enable_columnar_agg_stream = off;
INSERT INTO fassoc_res
SELECT 'eager', m, v FROM (
  SELECT 'sum_f' AS m, sum(f)::text AS v FROM t_fassoc
  UNION ALL SELECT 'avg_f', avg(f)::text FROM t_fassoc
) q;

SET theodb.enable_columnar_agg_stream = on;
INSERT INTO fassoc_res
SELECT 'stream', m, v FROM (
  SELECT 'sum_f' AS m, sum(f)::text AS v FROM t_fassoc
  UNION ALL SELECT 'avg_f', avg(f)::text FROM t_fassoc
) q;

\echo '### M169-T3.1 — valores medidos, bit a bit (sem arredondamento)'
SELECT arm, metric, val FROM fassoc_res ORDER BY metric, arm;

\if :{?gate_selftest}
  \echo '### AUTO-TESTE ARMADO: corrompendo o braço stream no ÚLTIMO dígito — o gate DEVE abortar'
  -- Um ULP. Se o gate não pegar isto, ele não pega divergência de associatividade nenhuma, que é do mesmo
  -- tamanho — e o "IDENTICO" acima seria um verde que não prova nada.
  UPDATE fassoc_res SET val = '2.00000000000002e+17' WHERE arm = 'stream' AND metric = 'sum_f';
\endif

\echo '### M169-T3.1 — veredito'
DO $gate$
DECLARE
  d bigint;
BEGIN
  SELECT count(*) INTO d FROM (
    (SELECT metric, val FROM fassoc_res WHERE arm = 'eager'
     EXCEPT ALL
     SELECT metric, val FROM fassoc_res WHERE arm = 'stream')
    UNION ALL
    (SELECT metric, val FROM fassoc_res WHERE arm = 'stream'
     EXCEPT ALL
     SELECT metric, val FROM fassoc_res WHERE arm = 'eager')
  ) x;
  IF d <> 0 THEN
    -- Falhar aqui NÃO é opcional: um `sum(float8)` que depende do tamanho do chunk-group é pior que o overflow
    -- que o M169 remove, porque é silencioso. O caminho seria declinar float8 do streaming, como o M154 declinou
    -- float do count(distinct) pelo mesmo motivo (IEEE-754, não desleixo).
    RAISE EXCEPTION 'M169 FLOAT-ASSOC GATE FAILED: % métrica(s) divergem entre eager e streaming — o streaming '
                    'mudou o resultado de float8', d;
  END IF;
  RAISE NOTICE 'M169 FLOAT-ASSOC GATE ok: sum/avg de float8 concordam BIT A BIT entre eager e streaming sobre o '
               'dado adversarial (0.1 não-representável + 1e17 esparso, 3 chunk-groups). LIMITE HONESTO: isto é '
               'uma forma medida, não uma prova de independência de ordem para toda entrada.';
END
$gate$;
