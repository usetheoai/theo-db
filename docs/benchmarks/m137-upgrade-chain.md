# M137 — cadeia de upgrade do `theodb_rs` (medido, com limite honesto)

> Medido 2026-07-21 na droplet (165.227.121.20), PostgreSQL **18.4** em `/tmp/pg18data`, porta 28918.
> Plano: `.claude/knowledge-base/plans/theodb-rs-upgrade-chain-plan.md`.
> Blueprint: `.claude/knowledge-base/discoveries/blueprints/pgrx-upgrade-chain-blueprint.md`.

## Headline

`ALTER EXTENSION theodb_rs UPDATE TO '1.1.0'` **funciona** — pela primeira vez em 120 releases. Mas o teste que
prova **convergência a partir de um catálogo antigo de verdade** ainda não rodou, e por isso este milestone
**não está completo**.

| | Antes | Depois (medido) |
|---|---|---|
| `ALTER EXTENSION theodb_rs UPDATE` | **impossível** (zero scripts) | `ALTER EXTENSION` → `extversion 1.1.0` |
| `default_version` | `1.0.0` congelado por 120 releases | `1.1.0` |
| Oráculo de schema | inexistente | 196 objetos, 0 OIDs crus, estável |
| Idempotência | n/a | script rodado 2× → **0 erros**, snapshot inalterado |

## 1. O oráculo (T1.1) — medido

`theodb_rs/sql/schema_snapshot.sql`, 8 linhas, sem ferramenta externa: `pg_depend` + `pg_describe_object()`
com `ORDER BY 1`.

```
objetos no snapshot        : 196
OIDs crus (>= 4 dígitos)   : 0
diff entre 2 bancos frescos: VAZIO
```

`pg_describe_object` devolve identificador qualificado e sem OID, então a saída é comparável entre bancos; o
`ORDER BY` mata a instabilidade de ordem do SQL gerado pelo pgrx (o próprio header gerado avisa que a ordem não
é estável, o que torna `diff(1)` sobre o schema bruto inútil).

## 2. O que a superfície fez enquanto a versão ficou parada (T1.2 — medido)

Funções sob `#[pg_extern]` por tag:

| tag | data | funções |
|---|---|---|
| v0.30.0 | 2026-07-02 | 18 |
| v0.60.0 | 2026-07-09 | 48 |
| v0.90.0 | 2026-07-16 | 60 |
| v0.110.0 | 2026-07-20 | 78 |
| v0.120.0 | 2026-07-21 | 78 |

**`1.0.0` rotulou pelo menos cinco catálogos diferentes.** É isso que obriga o primeiro salto a ser convergente
em vez de delta — e foi o achado que inverteu a recomendação da pesquisa (que assumia origem única).

### Removidos

**`_import_pinecone`** — única função que existiu em release anterior e não existe hoje. Entra como
`DROP FUNCTION IF EXISTS` no script.

Reproduzir: `for t in v0.30.0 v0.60.0 v0.90.0 v0.110.0 v0.120.0; do git grep -h -A6 pg_extern $t -- theodb_rs/src | grep -oE 'fn [a-z_0-9]+$'; done`

## 3. O script convergente (T2.1) — gerado, não transcrito

`scripts/gen-upgrade-script.py` transforma o SQL de instalação do pgrx (2310 linhas, 196 objetos) em script
idempotente:

```
122 CREATE FUNCTION → CREATE OR REPLACE FUNCTION
 20 objetos guardados (TYPE, CAST, OPERATOR, OPERATOR CLASS, EVENT TRIGGER)
  2 DROP IF EXISTS
```

Transcrever 2310 linhas à mão seria erro garantido, e o erro dessa classe é **silencioso**: caminho ausente é
erro alto (`extension.c:1415`), caminho incompleto sobe sem falhar.

### Dois bugs meus, ambos de ancoragem de regex

Registro porque a lição é a mesma nas duas vezes:

1. O pgrx emite um bloco de comentário antes de cada objeto, então `re.match` sobre o statement nunca casava —
   convertia **25 de 122** funções e o script "funcionava" (só não era idempotente).
2. O pgrx **indenta** `CREATE OPERATOR CLASS` em 4 espaços, então `^CREATE` deixava as opclasses desguardadas e
   o install morria em `operator class ... already exists`.

Depois do segundo, apliquei `[ \t]*` em **todos** os padrões, não só no que falhou.

### Um SQLSTATE medido, não presumido

`CREATE CAST` duplicado levanta `42710 duplicate_object`; **`CREATE OPERATOR` duplicado levanta `42723
duplicate_function`**. A primeira guarda só capturava o primeiro e o install morria em `operator <-> already
exists`. A guarda final captura as duas condições — e **deliberadamente não usa `WHEN OTHERS`**, que engoliria
erro real (Regra 8).

## 4. Provas

```
T2.1  ALTER EXTENSION theodb_rs UPDATE TO '1.1.0'  → ALTER EXTENSION, extversion = 1.1.0
      CREATE EXTENSION (limpa)                      → extversion = 1.1.0
T3.1  SCENARIO_A_OK — upgradado 196 == limpo 196, diff vazio
IDEM  script rodado 2× no mesmo banco → 0 erros, snapshot byte-idêntico
```

## 5. Convergência a partir de catálogo envelhecido (o teste decisivo) — MEDIDO

O limite 1 desta evidência (o Cenário A fraco) está **fechado**. Método e por que ele foi necessário:

**Não é possível buildar uma tag antiga contra o binário atual.** v0.90.0 declara `default = ["pg17"]`, e o M135
removeu o suporte a PG17 — o código antigo não compila no PG18 e o novo não compila no PG17. Um usuário vindo
de uma release antiga estaria também fazendo upgrade de major do Postgres, que é problema diferente (`pg_upgrade`).

Mas o que a convergência precisa exercitar é o **estado do catálogo**, não qual binário o produziu. Então
construímos um catálogo genuinamente incompleto removendo objetos de uma instalação `1.0.0`:

```
ALTER EXTENSION theodb_rs DROP FUNCTION theodb.embed(text,text);        DROP FUNCTION ...
ALTER EXTENSION theodb_rs DROP FUNCTION ai.rerank(text,text[],text,integer);   DROP FUNCTION ...
ALTER EXTENSION theodb_rs DROP FUNCTION theodb.embed_batch(text[],text); DROP FUNCTION ...
```

Resultado medido:

```
catálogo envelhecido : 193 objetos   (faltando embed, embed_batch, ai.rerank)
ALTER EXTENSION theodb_rs UPDATE TO '1.1.0'  → ALTER EXTENSION
pós-upgrade          : 196 objetos
diff vs instalação limpa : VAZIO      → CONVERGENCIA_OK
```

**Isto é o que o Cenário A da §4 não provava:** o script leva um catálogo que *genuinamente não tinha* os
objetos ao estado completo. E, pelo mesmo motivo, o teste agora tem **poder de detecção** — a ausência de um
objeto no catálogo de origem é visível no oráculo (193 ≠ 196) antes e invisível depois.

### Duas falsas leituras minhas no caminho, registradas

1. **Primeira tentativa de envelhecimento não envelheceu nada** e reportei `CONVERGENCIA_OK` comparando 196 com
   196. Causa: usei assinaturas erradas (`theodb.embed(text)` em vez de `theodb.embed(text,text)`) e meu
   `grep -cE "^ERROR"` não pegava os erros do psql, que vêm prefixados com `psql:arquivo:linha:`. O "pass" era
   vacuoso.
2. **O teste de injeção da §4 continua sem poder** pela razão explicada lá — mas deixou de importar, porque
   este teste o substitui com um oráculo que realmente distingue os dois estados.

## 6. ACL e cenário B1 — MEDIDO (os dois últimos itens do DoD)

**Paridade de ACL.** O oráculo original cobria membresia (`pg_depend`), não privilégio — e um `DROP`+`CREATE`
perde `REVOKE ... FROM PUBLIC` enquanto `CREATE OR REPLACE` preserva. Adicionado um segundo bloco ao
`schema_snapshot.sql` que lista as ACLs explícitas da superfície:

```
linhas de ACL — upgradado: 87   limpo: 87   → ACL_OK, idênticas
```

Confirma empiricamente a propriedade que motivou o desenho: o script usa `CREATE OR REPLACE` em todas as 122
funções, então owner e privilégio sobrevivem ao upgrade.

**Cenário B1** — o `.so` novo carregado contra um catálogo `1.0.0` **sem** rodar `ALTER EXTENSION`. É o usuário
que faz `apt upgrade` e esquece o comando. Para nós não é hipotético: os index AMs leem páginas em disco e o
`theodb_columnar` é um TableAM — divergência ali é crash, não mensagem de erro.

```
CREATE INDEX ... USING theodb_hnsw + busca top-5   → executou
CREATE TABLE ... USING theodb_columnar + agregado  → 1000 linhas, sum 500500
crashes antes=0  depois=0                          → SCENARIO_B1_DONE
```

## Limites honestos

1. O catálogo antigo foi construído por **remoção de objetos**, não produzido por um binário antigo — impossível
   hoje, porque as tags anteriores exigem PG17 e o M135 o removeu. Um usuário vindo de release antiga estaria
   também fazendo `pg_upgrade` de major, problema diferente desta cadeia.
2. **`CREATE EXTENSION theodb_rs VERSION '1.0.0'` deixa de funcionar** — o pgrx gera o script base a partir do
   `default_version`, então só existe `theodb_rs--1.1.0.sql`. O caminho de pin/downgrade some.
3. O oráculo compara **identidade** de objeto, não definição. Não vê: corpo de função plpgsql (5 no script),
   colunas/defaults de tabela, definição de índice, membros de opclass, comentários, e `proowner`.

## Review (council-index-storage) — um defeito silencioso meu, corrigido

**F1 (HIGH, corrupção silenciosa).** O pgrx emite **dois** statements para o tipo: o shell (`CREATE TYPE vector;`)
e a definição completa. Minha primeira versão deu aos dois o **mesmo** predicado `WHERE typname = 'vector'`.
Consequência: num catálogo que não tem o tipo, o shell é criado, o guard da definição completa vira **falso**, e o
tipo fica shell **para sempre — sem erro**. Não é hipotético: **v0.30.0 é uma tag lançada rotulada `1.0.0` cujo
catálogo não tem o nosso `vector`**. E o predicado não qualificava namespace, então um `vector` do pgvector
satisfaria os dois guards e os casts/operadores seguintes ligariam no tipo alheio — bind com layout errado.

Corrigido: shell guarda por existência, definição completa guarda por `typisdefined`, e ambos qualificam
`typnamespace = 'public'::regnamespace`. Verificado no SQL gerado — os dois guards agora diferem.

**Honestidade sobre o que NÃO foi provado:** a convergência end-to-end de um catálogo sem o tipo **não rodou**.
Minha tentativa foi inválida (rodei o script solto num banco vazio, e o `\echo ... \quit` do topo faz o psql sair
na linha 1). A correção está certa por construção e verificada no SQL emitido; a prova empírica desse caso
específico continua pendente. A regressão da cadeia normal passou (283 linhas = 196 objetos + 87 ACL).

**F4 (HIGH) — FECHADO.** `scripts/test-upgrade.sh` existe e roda os quatro cenários de ponta a ponta:

```
SCENARIO_A_OK        pós-upgrade == instalação limpa (schema + ACL)  — 283 linhas
CONVERGENCIA_OK      catálogo incompleto (277) convergiu para completo (283)
IDEMPOTENTE_OK       script rodado 2x: 0 erros, snapshot inalterado
SCENARIO_B1_DONE     .so novo sobre catálogo 1.0.0 sem UPDATE: servidor sobreviveu
```

Reproduzir: `PGINST=/root/.pgrx/18.4/pgrx-install PGPORT=28918 bash scripts/test-upgrade.sh`

O harness aborta se o envelhecimento do catálogo não remover nada — a guarda contra o pass vacuoso que
efetivamente cometi (reportar `CONVERGENCIA_OK` comparando 196 com 196).

**F5 (HIGH) — FECHADO.** `.github/workflows/schema-drift-gate.yml` falha um PR que altere `theodb_rs/src/` sem
bumpar `default_version` nem adicionar script de upgrade, e falha a edição de migração já lançada.

**Achados do review ainda ABERTOS:**

- **F1 (HIGH) — correção aplicada, prova empírica PENDENTE.** Os guards de TYPE agora diferem corretamente
  (verificado no SQL emitido), mas a convergência de um catálogo **sem o tipo** não foi exercitada — construir
  esse catálogo exige remover o tipo com CASCADE, que derruba metade da superfície. É o último item aberto.
- **F2/F3 (MEDIUM, latentes e medidos)** — `CREATE TABLE IF NOT EXISTS` não converge drift de coluna, e o
  `EXCEPTION` engole "mesmo objeto, definição diferente" em opclass/cast. O revisor mediu: nenhum caso vivo hoje
  (tabelas nasceram inteiras, opclasses byte-idênticas desde v0.60.0), mas morde na primeira mudança de membro.

## Estado do DoD

| Item | Estado |
|---|---|
| Oráculo estável e sem OID | ✅ |
| `ALTER EXTENSION UPDATE` funciona | ✅ |
| Idempotência (2× sem erro, snapshot igual) | ✅ |
| Cenário A diff vazio | ✅ |
| **Convergência de catálogo incompleto** | ✅ **193 → 196, diff vazio** (§5) |
| Poder de detecção do oráculo | ✅ (§5 — distingue 193 de 196) |
| Cenário B1 (`.so` novo, catálogo antigo, sem UPDATE) | ✅ servidor sobreviveu, AM e TableAM operaram (§6) |
| Paridade de ACL | ✅ 87 == 87 (§6) |

**Todos os itens do DoD estão fechados e medidos.**

### O que continua fora de escopo, declarado

O catálogo antigo foi construído por remoção de objetos, não produzido por um binário antigo — impossível hoje,
porque as tags anteriores exigem PG17 e o M135 o removeu. Um usuário vindo de uma release antiga estaria também
fazendo upgrade de major do Postgres, que é problema de `pg_upgrade`, não desta cadeia.
