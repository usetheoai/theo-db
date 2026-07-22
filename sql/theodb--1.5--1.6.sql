-- TheoDB umbrella extension — upgrade 1.5 -> 1.6.
-- M143 (remoção total do pg_duckdb): a superfície M62 vira OWN-CODE (DataFusion/Arrow no theodb_rs, sem DuckDB).
-- As funções codegen que RETORNAVAM texto pg_duckdb (theodb.htap_refresh_sql → COPY, theodb.olap_sql →
-- duckdb.query) são REMOVIDAS e substituídas por funções DIRETAS: theodb.htap_refresh (escreve o snapshot
-- own-code via public.write_parquet + registra) e theodb.olap (lê+agrega own-code via public.olap). Ver ADR-0057
-- e sql/85-theodb-htap.sql (a fonte greenfield — este delta re-aplica em intenção byte-idêntica).
--
-- Greenfield (theodb--1.0.sql já concatena o sql/85 novo) → este delta é o caminho in-place para um 1.5 instalado.
-- Idempotente: DROP IF EXISTS + CREATE OR REPLACE. Requer o theodb_rs com o módulo parquet own-code (M143).

-- Remove as funções codegen do pg_duckdb (substituídas).
DROP FUNCTION IF EXISTS theodb.htap_refresh_sql(regclass);
DROP FUNCTION IF EXISTS theodb.olap_sql(regclass);

-- theodb.htap_refresh — OWN-CODE: materializa a relação num snapshot Parquet (public.write_parquet) + registra.
CREATE OR REPLACE FUNCTION theodb.htap_refresh(p_rel regclass)
RETURNS timestamptz
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    v_path text := theodb._htap_path(p_rel);
    v_n    bigint;
BEGIN
    v_n := public.write_parquet(p_rel::regclass::text, v_path);
    RETURN theodb.htap_register(p_rel, v_path);
END;
$$;

-- theodb.olap — OWN-CODE: resolve o snapshot + retorna o agregado lido+agregado own-code (public.olap).
CREATE OR REPLACE FUNCTION theodb.olap(p_rel regclass)
RETURNS TABLE(category text, c bigint, a float8)
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    snap_path text;
BEGIN
    SELECT s.parquet_path INTO snap_path
        FROM theodb._htap_snapshots s WHERE s.rel = p_rel;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'theodb.olap: no snapshot for %; call theodb.htap_refresh(%) first', p_rel, p_rel
            USING ERRCODE = 'P0002';
    END IF;

    RETURN QUERY SELECT * FROM public.olap(snap_path);
END;
$$;

REVOKE ALL ON FUNCTION theodb.htap_refresh(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.olap(regclass) FROM PUBLIC;
