-- Upgrade 0.6.0 -> 0.7.0: acrescenta o alias `ivfflat` (B-037).
--
-- Só o DELTA. Quem já tem 0.6.0 tem o alias `hnsw` e o guard; recriá-los aqui falharia com
-- "already exists" e transformaria um upgrade em erro.

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

-- =====================================================================================
-- O QUE ESTE UPGRADE **NÃO** FAZ, e a razão.
--
-- A 0.7.0 também corrige a `vector_l2_ops` sob `hnsw`, que a 0.6.0 marcava DEFAULT indevidamente
-- (o pgvector 0.8.3 não tem opclass default para `hnsw` — medido). Um upgrade não consegue desfazer
-- isso: o PostgreSQL não tem `ALTER OPERATOR CLASS ... NOT DEFAULT`, e a única via seria
-- `DROP OPERATOR CLASS`, que derruba **todo índice dependente**. Trocar uma divergência cosmética
-- de `pg_get_indexdef` por perda de índice de usuário seria um péssimo negócio.
--
-- Consequência declarada: uma instalação que veio da 0.6.0 mantém `vector_l2_ops` como default sob
-- `hnsw`; uma instalação nova, não. A diferença aparece só na forma como `\d` e `pg_get_indexdef`
-- imprimem a definição — nenhum índice muda de comportamento. Registrado no [[B-037]].
-- =====================================================================================
