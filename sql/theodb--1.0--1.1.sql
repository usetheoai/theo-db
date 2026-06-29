-- TheoDB umbrella extension — upgrade 1.0 -> 1.1.
-- PostgreSQL chains update scripts automatically (theodb--1.0.sql -> theodb--1.0--1.1.sql); with
-- theodb.control default_version = '1.1', a fresh `CREATE EXTENSION theodb` builds 1.0 then applies this
-- delta. ALTER EXTENSION theodb UPDATE runs it on existing installs.
--
-- Retirement migration (audit #9 — missing_deprecation_marker): the legacy plpython3u `theodb.embed`
-- shipped through v0.15.0 as a `theodb` member; M17 (v0.16.0) moved theodb.embed to the Rust `theodb_rs`
-- extension. An existing v0.x install therefore still owns the plpython3u embed, and adding `theodb_rs`
-- would clash on `CREATE FUNCTION theodb.embed`. This delta DROPs the legacy function — but ONLY when it
-- is genuinely the plpython3u one AND not owned by `theodb_rs` (so the Rust wrapper is never touched, and
-- a blind DROP can't fail with "extension theodb_rs requires it"). On a fresh 1.1 install there is no
-- plpython3u embed (sql/30 is schema-only post-M17), so the guard finds nothing and the DROP is a no-op.
DO $$
BEGIN
  IF EXISTS (
    SELECT 1
    FROM pg_proc p
    JOIN pg_language l ON l.oid = p.prolang
    WHERE p.proname = 'embed'
      AND p.pronamespace = 'theodb'::regnamespace
      AND l.lanname = 'plpython3u'                       -- the legacy impl (the Rust wrapper is LANGUAGE sql)
      AND NOT EXISTS (                                    -- and NOT a member of theodb_rs (don't drop the Rust one)
        SELECT 1
        FROM pg_depend d
        JOIN pg_extension e ON e.oid = d.refobjid
        WHERE d.objid = p.oid
          AND d.deptype = 'e'
          AND e.extname = 'theodb_rs'
      )
  ) THEN
    DROP FUNCTION IF EXISTS theodb.embed(text, text);
    RAISE NOTICE 'theodb 1.0->1.1: retired the legacy plpython3u theodb.embed (now served by theodb_rs)';
  END IF;
END
$$;
