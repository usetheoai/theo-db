-- TheoDB umbrella extension — upgrade 1.2 -> 1.3.
-- M19 (ROADMAP-v2): `ai.nl_to_sql` (the LAST plpython3u) moved from the SQL `theodb` extension to the Rust
-- `theodb_rs` extension. This delta retires the legacy `theodb`-owned plpython3u definition so that
-- adding/updating `theodb_rs` does not clash on `CREATE FUNCTION ai.nl_to_sql`. `ai.nl_query` STAYS a plpgsql
-- keeper in `theodb` (L3 sandbox — ADR-F) and is NOT dropped. Guard: drop ONLY when the function is a member
-- of `theodb` AND NOT a member of `theodb_rs` (so the Rust version is never touched, and a blind DROP cannot
-- fail with "extension theodb_rs requires it"). On a fresh 1.3 install the regenerated base no longer defines
-- the plpython3u nl_to_sql, so the loop finds nothing — a no-op. Mirrors the M17/M18 retirement idiom.
--
-- IN-PLACE UPGRADE ORDER (non-greenfield): run `ALTER EXTENSION theodb UPDATE TO '1.3'` (this delta — drops
-- the legacy plpython3u nl_to_sql) BEFORE creating/updating the M19 `theodb_rs`; otherwise the Rust
-- `CREATE FUNCTION ai.nl_to_sql` clashes. The greenfield container path orders theodb then theodb_rs.
DO $$
DECLARE r record;
BEGIN
  FOR r IN
    SELECT p.oid::regprocedure AS sig
    FROM pg_proc p
    WHERE p.pronamespace = 'ai'::regnamespace
      AND p.proname = 'nl_to_sql'
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
