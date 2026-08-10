-- M169 — oráculo da METADE EXECUTANTE do fail-open (achado A-03 do /review).
--
-- O que já existia: 5 testes unitários sobre `stream_failure_is_fail_open`, o PREDICADO. O que NÃO existia:
-- nada exercitava o ramo que vem depois dele — o `reset_stream_cg_count()`, a re-entrada no decode eager, e a
-- afirmação de que a resposta do recuo é a MESMA que o caminho streamado daria. Um predicado correto com um
-- ramo errado passa nos cinco testes e entrega resultado errado.
--
-- POR QUE ISTO É DETERMINÍSTICO AGORA, e não era antes. O recuo dispara quando o braço streamado falha por
-- recurso. Com o spill apontando para `pgsql_tmp` (fix do F1) o agregado passou a DERRAMAR em vez de falhar,
-- então esgotar a pool deixou de ser gatilho confiável. Mas o mesmo fix derivou o teto do diretório temporário
-- de `temp_file_limit`, e estourar esse teto devolve `ResourcesExhausted`
-- (`datafusion-execution-54.0.0/src/disk_manager.rs:421`, `resources_err!`) — a classe que o fail-open casa.
-- Ou seja: `temp_file_limit` baixo é um gatilho EXATO, verificado na fonte, e não uma aproximação por pressão.
--
-- Uso: psql -f benchmarks/m169_fallback_oracle.sql

\set ON_ERROR_STOP on
\timing off

DROP TABLE IF EXISTS m169_fb;
CREATE TABLE m169_fb (k bigint, v int) USING theodb_columnar;
-- Cardinalidade alta o bastante para o agregado precisar derramar sob uma pool pequena: 800k grupos distintos
-- a ~92 B/grupo (medido no T3.2) são ~74 MB de estado, acima da pool de `work_mem*2 + 64MB`.
INSERT INTO m169_fb SELECT g, (g % 7)::int FROM generate_series(1, 800000) g;

-- ---------------------------------------------------------------------------------------------------------
-- BRAÇO A — referência. Streaming DESLIGADO: a resposta vem do caminho eager, que é o comportamento pré-M169.
-- ---------------------------------------------------------------------------------------------------------
SET theodb.enable_columnar_agg = on;
SET theodb.enable_columnar_agg_stream = off;
RESET work_mem;
RESET temp_file_limit;

CREATE TEMP TABLE arm_a AS
  SELECT k, count(*) AS c, sum(v) AS s FROM m169_fb GROUP BY k;

-- ---------------------------------------------------------------------------------------------------------
-- BRAÇO B — streaming LIGADO, com o spill estrangulado. O braço streamado tem de falhar por recurso e a
-- consulta tem de completar mesmo assim, pelo recuo.
-- ---------------------------------------------------------------------------------------------------------
SET theodb.enable_columnar_agg_stream = on;
SET work_mem = '64kB';
SET temp_file_limit = '1MB';

CREATE TEMP TABLE arm_b AS
  SELECT k, count(*) AS c, sum(v) AS s FROM m169_fb GROUP BY k;

-- O contador é lido NESTA sessão, logo após o braço B. Ele é zerado na ENTRADA da rota (fix do review), então
-- `0` significa exatamente uma coisa: a resposta NÃO veio do caminho streamado.
SELECT theodb_columnar_stream_chunk_groups() AS stream_cgs_after_b \gset

RESET work_mem;
RESET temp_file_limit;

-- ---------------------------------------------------------------------------------------------------------
-- GATE 1 — o recuo de fato aconteceu. Sem isto o teste passaria trivialmente numa configuração em que o
-- streaming completou, e não teria exercitado nada do que existe para exercitar.
-- ---------------------------------------------------------------------------------------------------------
DO $$
BEGIN
  IF :stream_cgs_after_b <> 0 THEN
    RAISE EXCEPTION 'VACUO: o braço B NÃO recuou (stream_cgs=%). O gatilho não disparou — este teste não '
                    'exercitou o ramo do fail-open, e um verde aqui seria falso.', :stream_cgs_after_b;
  END IF;
END $$;

-- ---------------------------------------------------------------------------------------------------------
-- GATE 2 — a resposta do recuo é a MESMA. EXCEPT simétrico: `A - B` e `B - A`, porque `A - B` sozinho não
-- detecta linha a mais em B.
-- ---------------------------------------------------------------------------------------------------------
SELECT count(*) AS diverged FROM (
  (SELECT * FROM arm_a EXCEPT ALL SELECT * FROM arm_b)
  UNION ALL
  (SELECT * FROM arm_b EXCEPT ALL SELECT * FROM arm_a)
) d \gset

DO $$
BEGIN
  IF :diverged <> 0 THEN
    RAISE EXCEPTION 'DIVERGIU: o recuo devolveu resultado diferente do caminho de referência (% linhas)', :diverged;
  END IF;
END $$;

-- ---------------------------------------------------------------------------------------------------------
-- CONTROLE POSITIVO — o oráculo TEM de reprovar uma divergência deliberada. Um comparador que nunca reprova
-- não é evidência de nada, e este projeto já publicou `diverged=0` contra uma tabela vazia.
-- ---------------------------------------------------------------------------------------------------------
CREATE TEMP TABLE arm_b_corrupt AS SELECT k, c, s FROM arm_b;
UPDATE arm_b_corrupt SET c = c + 1 WHERE k = 1;

SELECT count(*) AS ctrl FROM (
  (SELECT * FROM arm_a EXCEPT ALL SELECT * FROM arm_b_corrupt)
  UNION ALL
  (SELECT * FROM arm_b_corrupt EXCEPT ALL SELECT * FROM arm_a)
) d \gset

DO $$
BEGIN
  IF :ctrl = 0 THEN
    RAISE EXCEPTION 'ORÁCULO QUEBRADO: o controle positivo passou. O comparador não detecta divergência, '
                    'logo o diverged=0 acima não é evidência de coisa alguma.';
  END IF;
  RAISE NOTICE 'controle positivo OK (detectou % linhas divergentes)', :ctrl;
END $$;

DROP TABLE m169_fb;
\echo '=== M169 fallback oracle: OK — o recuo disparou (stream_cgs=0) e devolveu resultado idêntico ==='
