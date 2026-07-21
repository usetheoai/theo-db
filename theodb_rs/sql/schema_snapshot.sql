-- M137 — o ORÁCULO da cadeia de upgrade.
--
-- Lista todo objeto que é membro da extensão `theodb_rs`, de forma comparável entre bancos diferentes.
-- Duas propriedades fazem isso funcionar, e ambas são deliberadas:
--
--   1. `pg_describe_object()` devolve um identificador qualificado e **sem OID**
--      (ex.: `function theodb.embed(text)`), então a saída de dois bancos distintos é comparável.
--      Comparar OIDs seria inútil: eles diferem por banco.
--   2. `ORDER BY 1` mata a instabilidade de ordem. O SQL de instalação gerado pelo pgrx é ordenado por um
--      grafo de dependências cuja ordem NÃO é estável (o próprio header gerado diz isso), então `diff(1)`
--      sobre o schema bruto é ruído. Ordenar o conjunto de objetos remove essa variável.
--
-- `deptype = 'e'` seleciona exatamente os membros da extensão (o que `ALTER EXTENSION ... ADD` registra).
--
-- LIMITE CONHECIDO (plano § Unresolved Questions, Q1): `pg_depend` registra MEMBRESIA, não ACL. Um upgrade
-- que perca um `REVOKE ... FROM PUBLIC` passa neste oráculo. Cobrir isso exige um snapshot separado de
-- `proacl` — declarado como não coberto por este milestone em vez de silenciado.
--
-- Uso:  psql -tA -f theodb_rs/sql/schema_snapshot.sql -d <db>
SELECT pg_describe_object(d.classid, d.objid, d.objsubid) AS object
FROM pg_depend d
JOIN pg_extension e ON e.oid = d.refobjid
WHERE d.refclassid = 'pg_extension'::regclass
  AND d.deptype = 'e'
  AND e.extname = 'theodb_rs'
ORDER BY 1;

-- M137 §ACL — o oráculo acima cobre MEMBRESIA (`pg_depend`), não PRIVILÉGIO. Um upgrade que faça `DROP`+`CREATE`
-- numa função perde o `REVOKE ... FROM PUBLIC` (só `CREATE OR REPLACE` preserva ACL), e isso passaria batido.
-- Este segundo bloco fecha o buraco: lista as ACLs explícitas da superfície da extensão, ordenadas.
-- Rodar com:  psql -tA -f schema_snapshot.sql   (as duas queries saem em sequência e o diff cobre ambas)
SELECT 'acl ' || n.nspname || '.' || p.proname || ' = ' || COALESCE(array_to_string(p.proacl, ','), '(default)')
FROM pg_proc p
JOIN pg_namespace n ON n.oid = p.pronamespace
JOIN pg_depend d ON d.objid = p.oid AND d.classid = 'pg_proc'::regclass AND d.deptype = 'e'
JOIN pg_extension e ON e.oid = d.refobjid AND e.extname = 'theodb_rs'
WHERE p.proacl IS NOT NULL
ORDER BY 1;
