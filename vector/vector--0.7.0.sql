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

-- ==== M149 (#182): aliases de AM e opclasse (mesmo corpo do upgrade 0.5.1--0.6.0) ====
-- M149 (#182) — upgrade 0.5.1 → 0.6.0 do shim de compatibilidade pgvector.
--
-- O 0.5.1 completou o drop-in no nível BOOTSTRAP (#181): `CREATE EXTENSION vector` passou a funcionar e
-- colunas `vector(N)` a ser criadas. Mas a migration real de uma app pgvector ainda quebrava no
-- `CREATE INDEX ... USING hnsw (col vector_cosine_ops)` — o AM e as opclasses existiam apenas sob a
-- nomenclatura própria (`theodb_hnsw`, `theodb_hnsw_*_ops`). Este upgrade adiciona os ALIASES.
--
-- Nada é reimplementado (Regra 9): o AM `hnsw` usa exatamente o mesmo handler own-code
-- (`theodb_hnsw_amhandler`) e as opclasses reusam os mesmos operadores e funções de suporte do
-- `theodb_hnsw`. É rotulagem de catálogo, não uma segunda implementação.
--
-- Honestidade (Regra 3): o `default_version` sobe para 0.6.0 porque a superfície mudou — a versão declara
-- o contrato de features que o tooling inspeciona, e prometer 0.5.1 (que no pgvector já inclui HNSW) sem
-- entregar o AM era exatamente a divergência apontada no review do #181.

-- O AM alias: MESMO handler own-code do theodb_hnsw. Registrar sob o nome que as apps escrevem.
CREATE ACCESS METHOD hnsw TYPE INDEX HANDLER theodb_hnsw_amhandler;

-- As opclasses que o tooling pgvector referencia. Strategy 1 = ordering operator (FOR ORDER BY float_ops),
-- idêntico ao que as opclasses `theodb_hnsw_*_ops` já declaram (verificado em pg_amop: amoppurpose='o',
-- amopsortfamily=float_ops).

-- NÃO é DEFAULT, e isso é compatibilidade e não descuido. Medido em 2026-08-20 contra
-- `pgvector/pgvector@sha256:be400b5…` (pgvector 0.8.3): sob `ivfflat` a `vector_l2_ops` É default,
-- e sob `hnsw` **nenhuma opclass é**. A versão anterior deste shim marcava DEFAULT aqui, com o
-- comentário dizendo que espelhava o `theodb_hnsw_l2_ops` — espelhou o AM PRÓPRIO em vez do
-- pgvector, que é o que este arquivo existe para imitar.
--
-- O efeito era observável e escapava: `pg_get_indexdef` OMITE a opclass quando ela é default, então
-- um índice migrado de pgvector aparecia como `USING hnsw (embedding)` no TheoDB contra
-- `USING hnsw (embedding vector_l2_ops)` na origem. O `migration-smoke` compara as definições e
-- reprovava — só que ninguém via, porque a restauração falhava antes, no `ivfflat` ausente (B-037).
CREATE OPERATOR CLASS vector_l2_ops FOR TYPE vector USING hnsw AS
    OPERATOR 1 <-> (vector, vector) FOR ORDER BY float_ops;

-- Cosseno — a opclasse mais usada por apps de embedding (é a que o theo-memory declara).
CREATE OPERATOR CLASS vector_cosine_ops FOR TYPE vector USING hnsw AS
    OPERATOR 1 <=> (vector, vector) FOR ORDER BY float_ops,
    FUNCTION 1 theodb_metric_cosine();

-- Inner product (negativo) — completa o trio do pgvector.
CREATE OPERATOR CLASS vector_ip_ops FOR TYPE vector USING hnsw AS
    OPERATOR 1 <#> (vector, vector) FOR ORDER BY float_ops,
    FUNCTION 1 theodb_metric_ip();

-- =====================================================================================
-- 0.7.0 — o alias `ivfflat` (B-037). ADICIONADO em 2026-08-20.
--
-- O DEFEITO. O shim registrava o alias `hnsw` e parava aí. Medido em 2026-08-12 contra
-- `theodb:b034`: `SELECT amname FROM pg_am WHERE amtype='i'` devolvia `hnsw`, `theodb_hnsw` e
-- `theodb_ivfflat` — **sem `ivfflat`**. Logo `CREATE INDEX ... USING ivfflat (...)`, que é o que
-- uma app pgvector escreve, falhava; e o `ivfflat.probes` já havia sido registrado como GUC no
-- B-034, então o produto aceitava o botão de ajuste de um índice que não se podia criar pelo nome.
--
-- COMO ISSO APARECEU. O `migration-smoke` da esteira restaura um dump de pgvector vanilla contendo
-- `CREATE INDEX items_ivf ON items USING ivfflat (embedding vector_l2_ops) WITH (lists = 10)` e
-- reprovava com `access method "ivfflat" does not exist`. O gate estava certo o tempo inteiro;
-- ficou escondido atrás de sete jobs que falhavam antes dele por outra causa (B-084).
--
-- Nada é reimplementado (Regra 9): o AM `ivfflat` usa o MESMO handler own-code
-- (`theodb_ivfflat_amhandler`) e as opclasses reusam os mesmos operadores e a mesma support proc de
-- metric-tag do `theodb_ivfflat`. É rotulagem de catálogo, exatamente como o alias `hnsw`.
--
-- A reloption `WITH (lists = N)` já é aceita pelo AM próprio (`am/options.rs`, M34), então o alias
-- cobre a sintaxe pgvector INTEIRA e não só o nome.
-- =====================================================================================

CREATE ACCESS METHOD ivfflat TYPE INDEX HANDLER theodb_ivfflat_amhandler;

-- Os nomes de opclasse são escopados POR access method, então repetir `vector_l2_ops` aqui não
-- colide com a do alias `hnsw` acima — são objetos distintos, como no pgvector.

-- L2 / distância euclidiana — DEFAULT, espelhando `theodb_ivfflat_l2_ops`.
CREATE OPERATOR CLASS vector_l2_ops DEFAULT FOR TYPE vector USING ivfflat AS
    OPERATOR 1 <-> (vector, vector) FOR ORDER BY float_ops;

-- Cosseno.
CREATE OPERATOR CLASS vector_cosine_ops FOR TYPE vector USING ivfflat AS
    OPERATOR 1 <=> (vector, vector) FOR ORDER BY float_ops,
    FUNCTION 1 theodb_metric_cosine();

-- Inner product (negativo) — completa o trio.
CREATE OPERATOR CLASS vector_ip_ops FOR TYPE vector USING ivfflat AS
    OPERATOR 1 <#> (vector, vector) FOR ORDER BY float_ops,
    FUNCTION 1 theodb_metric_ip();
