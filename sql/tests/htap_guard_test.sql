-- htap_guard_test.sql — M142 T0.1: prova o guard fail-closed da superfície HTAP codegen.
--
-- Self-adapting (roda nas DUAS imagens):
--   • pg_duckdb AUSENTE (imagem default) → theodb.olap_sql / theodb.htap_refresh_sql DEVEM RAISE
--     ERRCODE '0A000' (feature_not_supported) com HINT sobre a imagem theodb-htap.
--   • pg_duckdb PRESENTE (imagem theodb-htap) → ambas retornam o TEXT correto (statement DuckDB).
--
-- Antes do guard (RED): htap_refresh_sql (LANGUAGE sql, sem checagem) retorna o COPY mesmo sem pg_duckdb →
-- a asserção do RAISE falha. Depois do guard (GREEN): passa nas duas imagens.
-- Uso: psql -v ON_ERROR_STOP=1 -f sql/tests/htap_guard_test.sql

\set ON_ERROR_STOP on

-- probe table (regclass real para as funções)
CREATE TEMP TABLE _htap_guard_probe (category text, amount numeric);
INSERT INTO _htap_guard_probe VALUES ('a', 10), ('b', 20);

DO $$
DECLARE
    has_duck boolean := to_regproc('duckdb.query') IS NOT NULL;
    txt      text;
    raised   boolean;
    probe    regclass := '_htap_guard_probe'::regclass;
BEGIN
    IF has_duck THEN
        -- ── caminho POSITIVO (pg_duckdb presente) ──
        -- htap_refresh_sql retorna o COPY parquet
        txt := theodb.htap_refresh_sql(probe);
        ASSERT position('FORMAT parquet' IN txt) > 0,
            'htap_refresh_sql deve retornar COPY (FORMAT parquet) quando pg_duckdb presente';
        -- registra um snapshot e olap_sql retorna o duckdb.query
        PERFORM theodb.htap_register(probe, '/var/lib/postgresql/htap/_probe.parquet');
        txt := theodb.olap_sql(probe);
        ASSERT position('duckdb.query' IN txt) > 0,
            'olap_sql deve retornar SELECT * FROM duckdb.query(...) quando pg_duckdb presente';
        RAISE NOTICE 'HTAP_GUARD_TEST_OK (positive path — pg_duckdb present)';
    ELSE
        -- ── caminho GUARD (pg_duckdb ausente) ──
        -- olap_sql DEVE RAISE 0A000 (o guard, ANTES do lookup de snapshot)
        raised := false;
        BEGIN
            txt := theodb.olap_sql(probe);
        EXCEPTION WHEN sqlstate '0A000' THEN raised := true;
        END;
        ASSERT raised, 'olap_sql deve RAISE 0A000 quando pg_duckdb ausente (guard fail-closed)';

        -- htap_refresh_sql DEVE RAISE 0A000
        raised := false;
        BEGIN
            txt := theodb.htap_refresh_sql(probe);
        EXCEPTION WHEN sqlstate '0A000' THEN raised := true;
        END;
        ASSERT raised, 'htap_refresh_sql deve RAISE 0A000 quando pg_duckdb ausente (guard fail-closed)';

        RAISE NOTICE 'HTAP_GUARD_TEST_OK (guard path — pg_duckdb absent)';
    END IF;
END $$;
