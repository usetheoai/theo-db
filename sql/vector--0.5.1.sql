-- M148 — pgvector compatibility shim (issue #181).
--
-- Deliberadamente NÃO cria tipo, operador nem opclasse: tudo isso é provido pelo `theodb_rs` (own-code,
-- M69/M70). Criar aqui colidiria com os objetos reais. O shim existe apenas para que
-- `CREATE EXTENSION IF NOT EXISTS vector` — o primeiro comando do bootstrap de toda app pgvector —
-- suceda, completando o drop-in declarado na ADR-0029 § D2 (que resolveu o nível SQL/tipos mas deixou o
-- nível tooling/drivers em aberto).
--
-- O `requires = 'theodb_rs'` no control já garante a ordem de instalação. O guard abaixo é
-- defense-in-depth fail-fast (Regra 8 / `rules/error-handling.md`): se por qualquer razão o tipo real não
-- estiver presente, a app DEVE falhar aqui — alto, cedo e claro — em vez de acreditar que tem pgvector e
-- quebrar depois, de forma obscura, na primeira coluna `vector(N)`.

\echo Use "CREATE EXTENSION vector" to load this file. \quit

DO $theodb_vector_shim$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM pg_type t
        JOIN pg_namespace n ON n.oid = t.typnamespace
        WHERE t.typname = 'vector' AND n.nspname = 'public'
    ) THEN
        RAISE EXCEPTION
            'theodb: the pgvector-compatibility shim requires the own-code public.vector type, which is provided by the theodb_rs extension'
            USING ERRCODE = 'undefined_object',
                  HINT = 'Install it first: CREATE EXTENSION theodb_rs; (or let the dependency resolve itself with CREATE EXTENSION vector CASCADE)';
    END IF;
END
$theodb_vector_shim$;
