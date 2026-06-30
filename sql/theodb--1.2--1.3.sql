-- TheoDB umbrella extension — upgrade 1.2 -> 1.3.
-- M19 (ROADMAP-v2): the last plpython3u (`ai.nl_to_sql`) AND the plpgsql `ai.hybrid_search_rrf` /
-- `ai.hybrid_search(jsonb)` / `theodb.import_pinecone` (FUNCTION) move from the SQL `theodb` extension to the
-- Rust `theodb_rs` extension. This delta retires the legacy `theodb`-owned definitions so that adding/updating
-- `theodb_rs` does not clash on `CREATE FUNCTION`. KEEPERS (NOT dropped): `ai.nl_query` (plpgsql L3 sandbox,
-- ADR-F) and `theodb.import_pinecone_chunked` (plpgsql PROCEDURE, ADR-D — only a PROCEDURE can COMMIT per
-- batch). Guard: drop ONLY when the routine is a member of `theodb` AND NOT a member of `theodb_rs` (so the
-- Rust version is never touched, and a blind DROP cannot fail with "extension theodb_rs requires it"). On a
-- fresh 1.3 install the regenerated base no longer defines these, so the loop finds nothing — a no-op.
-- Matches by exact (schema, name): `import_pinecone` retires the FUNCTION but never `import_pinecone_chunked`.
--
-- IN-PLACE UPGRADE ORDER (non-greenfield): run `ALTER EXTENSION theodb UPDATE TO '1.3'` (this delta — drops
-- the legacy routines) BEFORE creating/updating the M19 `theodb_rs`; otherwise the Rust `CREATE FUNCTION`
-- clashes. The greenfield container path orders theodb then theodb_rs.
DO $$
DECLARE r record;
BEGIN
  FOR r IN
    SELECT p.oid::regprocedure AS sig, p.proname, n.nspname
    FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace
    WHERE (n.nspname, p.proname) IN (
            ('ai', 'nl_to_sql'),
            ('ai', 'hybrid_search_rrf'),
            ('ai', 'hybrid_search'),
            ('theodb', 'import_pinecone')   -- FUNCTION only; import_pinecone_chunked (PROCEDURE) is a keeper
          )
      -- a member of theodb (the SQL umbrella)…
      AND EXISTS (
        SELECT 1 FROM pg_depend d
        JOIN pg_extension e ON e.oid = d.refobjid
        WHERE d.objid = p.oid AND d.deptype = 'e' AND e.extname = 'theodb')
      -- …and NOT a member of theodb_rs (never drop the Rust-owned port).
      AND NOT EXISTS (
        SELECT 1 FROM pg_depend d
        JOIN pg_extension e ON e.oid = d.refobjid
        WHERE d.objid = p.oid AND d.deptype = 'e' AND e.extname = 'theodb_rs')
  LOOP
    EXECUTE format('DROP FUNCTION %s', r.sig);
    RAISE NOTICE 'theodb 1.2->1.3: retired the legacy theodb-owned %', r.sig;
  END LOOP;
END
$$;
