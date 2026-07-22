-- TheoDB M62/M143 — HTAP unified surface (lakehouse row↔columnar) — OWN-CODE edition (sem pg_duckdb).
--
-- M143 (remoção total do pg_duckdb): o lakehouse agora é 100% own-code (DataFusion + Arrow, no theodb_rs). O
-- design "codegen" do M62 (funções que RETORNAVAM texto para o cliente rodar) existia SÓ porque o pg_duckdb
-- proibia executar DuckDB dentro de uma função. Com own-code essa restrição DESAPARECE — o DataFusion roda dentro
-- da função (`theodb_rs`: `public.read_parquet`/`public.olap`/`public.write_parquet`). Então a superfície colapsa
-- em funções DIRETAS: `theodb.htap_refresh` escreve o snapshot own-code + registra; `theodb.olap` lê+agrega
-- own-code. Sem `duckdb.query`, sem `COPY … (FORMAT parquet)`, sem guard de pg_duckdb. Ver ADR-0057.
--
-- O catálogo de snapshots (`_htap_snapshots`, `_htap_path`, `htap_register`, `htap_freshness`) é SQL puro e
-- permanece. `htap_register` continua público para quem materializa fora do fluxo `htap_refresh`.
-- Injection-safe (regclass); erro tipado fail-closed (`error-handling.md` §2). Idempotente (CREATE OR REPLACE).

-- Snapshot catalog — one row per materialized relation. refreshed_at é o relógio de freshness.
CREATE TABLE IF NOT EXISTS theodb._htap_snapshots (
    rel          regclass PRIMARY KEY,
    parquet_path text        NOT NULL,
    refreshed_at timestamptz NOT NULL
);

COMMENT ON TABLE theodb._htap_snapshots IS
    'M62 HTAP snapshot catalog: (rel, parquet_path, refreshed_at) — one row per materialized relation. '
    'refreshed_at is the freshness clock (theodb.htap_freshness derives the lag; theodb.olap resolves the path). '
    'Upserted by theodb.htap_register (M143: written own-code by theodb.htap_refresh, no DuckDB).';

-- theodb._htap_path — o path default do snapshot, derivado do oid (único por tabela). O dir base é criado no build
-- da imagem (chown postgres). M143: escrito pelo writer Parquet own-code (public.write_parquet), não pelo DuckDB.
CREATE OR REPLACE FUNCTION theodb._htap_path(rel regclass)
RETURNS text
LANGUAGE sql
STABLE
AS $$
    SELECT '/var/lib/postgresql/htap/theodb_htap_' || rel::oid || '.parquet';
$$;

-- theodb.htap_refresh — M143 OWN-CODE: materializa a tabela num snapshot Parquet (public.write_parquet, in-function)
-- E registra o snapshot, numa chamada só. Substitui o par codegen (htap_refresh_sql RETORNA COPY → cliente roda →
-- htap_register). Sem DuckDB. Retorna o refreshed_at registrado.
CREATE OR REPLACE FUNCTION theodb.htap_refresh(p_rel regclass)
RETURNS timestamptz
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    v_path text := theodb._htap_path(p_rel);
BEGIN
    -- escreve o Parquet own-code (DataFusion/Arrow, sem DuckDB); a função Rust resolve o nome canônico via
    -- $1::regclass (injection-safe). PERFORM (descarta a contagem de linhas de retorno).
    PERFORM public.write_parquet(p_rel::regclass::text, v_path);
    -- registra o snapshot (upsert). Reusa a validação/relógio de htap_register.
    RETURN theodb.htap_register(p_rel, v_path);
END;
$$;

COMMENT ON FUNCTION theodb.htap_refresh(regclass) IS
    'M62/M143 HTAP: materializa a relação num snapshot Parquet OWN-CODE (public.write_parquet — DataFusion/Arrow, '
    'sem DuckDB) e registra o snapshot, numa chamada. Retorna o refreshed_at. VOLATILE (escreve arquivo + catálogo). '
    'Substitui o antigo par codegen htap_refresh_sql (COPY) + client-run + htap_register.';

-- theodb.htap_register — upsert do catálogo. SQL puro, sem DuckDB — sempre rodou dentro de função. Público para
-- quem materializa fora do htap_refresh. Retorna o refreshed_at. Erro tipado em path vazio.
CREATE OR REPLACE FUNCTION theodb.htap_register(p_rel regclass, p_parquet_path text)
RETURNS timestamptz
LANGUAGE plpgsql
VOLATILE
AS $$
DECLARE
    snap_at timestamptz := now();
BEGIN
    IF p_parquet_path IS NULL OR btrim(p_parquet_path) = '' THEN
        RAISE EXCEPTION 'theodb.htap_register: parquet_path must be a non-empty path for %', p_rel
            USING ERRCODE = '22023';   -- invalid_parameter_value (typed, fail-closed)
    END IF;

    INSERT INTO theodb._htap_snapshots (rel, parquet_path, refreshed_at)
        VALUES (p_rel, p_parquet_path, snap_at)
        ON CONFLICT (rel) DO UPDATE
            SET parquet_path = EXCLUDED.parquet_path,
                refreshed_at = EXCLUDED.refreshed_at;

    RETURN snap_at;
END;
$$;

COMMENT ON FUNCTION theodb.htap_register(regclass, text) IS
    'M62 HTAP: register (rel, parquet_path, now()) in theodb._htap_snapshots. Pure SQL (no DuckDB). Upsert (latest '
    'snapshot wins). Returns refreshed_at. Raises invalid_parameter_value on a blank path. VOLATILE (catalog write).';

-- theodb.olap — M143 OWN-CODE: resolve o snapshot do catálogo e RETORNA as linhas do agregado canônico
-- (category, count(*), round(avg(amount),4)) lidas+agregadas own-code (public.olap — DataFusion, sem DuckDB).
-- Substitui olap_sql (que RETORNAVA um SELECT * FROM duckdb.query(...) para o cliente rodar). Erro tipado se não há
-- snapshot (fail-closed, nunca NULL silencioso).
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
            USING ERRCODE = 'P0002';  -- no_data_found
    END IF;

    -- lê+agrega o Parquet own-code (public.olap do theodb_rs — DataFusion, in-function). Sem DuckDB.
    RETURN QUERY SELECT * FROM public.olap(snap_path);
END;
$$;

COMMENT ON FUNCTION theodb.olap(regclass) IS
    'M62/M143 HTAP: retorna o agregado canônico (category, count, round(avg,4)) do snapshot Parquet registrado, '
    'lido+agregado OWN-CODE (public.olap — DataFusion/Arrow, sem DuckDB). Substitui o antigo olap_sql (codegen '
    'duckdb.query). Raises no_data_found (P0002) se não há snapshot — chame theodb.htap_refresh antes. STABLE.';

-- theodb.htap_freshness — o lag do snapshot (now() - refreshed_at). SQL puro, sem DuckDB.
CREATE OR REPLACE FUNCTION theodb.htap_freshness(p_rel regclass)
RETURNS interval
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    snap_at timestamptz;
BEGIN
    SELECT s.refreshed_at INTO snap_at FROM theodb._htap_snapshots s WHERE s.rel = p_rel;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'theodb.htap_freshness: no snapshot for %; call theodb.htap_refresh(%) first', p_rel, p_rel
            USING ERRCODE = 'P0002';
    END IF;

    RETURN now() - snap_at;
END;
$$;

COMMENT ON FUNCTION theodb.htap_freshness(regclass) IS
    'M62 HTAP: the snapshot lag (now() - refreshed_at) for a relation. Freshness is a dated contract (grows between '
    'refreshes, resets on theodb.htap_refresh/register). Pure SQL (no DuckDB). Raises no_data_found (P0002) if none.';

-- Least-privilege: a superfície orquestra escrita de arquivo server-side + leitura own-code. Não concedida a PUBLIC.
REVOKE ALL ON FUNCTION theodb.htap_refresh(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.htap_register(regclass, text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.olap(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.htap_freshness(regclass) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb._htap_path(regclass) FROM PUBLIC;
