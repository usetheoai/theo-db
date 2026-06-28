-- TheoDB umbrella extension — upgrade 1.0 -> 1.1 (skeleton).
-- Establishes the ALTER EXTENSION theodb UPDATE convention (M15 T3.1). PostgreSQL chains update
-- scripts automatically (theodb--1.0.sql -> theodb--1.0--1.1.sql). Real upgrades add objects here
-- with the idempotent idiom: CREATE OR REPLACE FUNCTION ... + DO $$ existence guards + @extschema:vector@
-- for cross-extension references (per pgvectorscale's upgrade corpus, blueprint Q7a).
-- This 1.1 seed makes no schema change — it proves the upgrade-chain mechanism is wired.
DO $$
BEGIN
  -- no-op: 1.1 introduces no new object (YAGNI — the seed exists to anchor the convention).
  NULL;
END
$$;
