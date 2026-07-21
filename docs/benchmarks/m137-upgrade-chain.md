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

## Limites honestos

1. **O Cenário A que passou é FRACO, e isso é o principal limite deste artefato.** Os dois lados da comparação
   derivam do **mesmo** SQL gerado (o `1.0.0` instalado hoje é o HEAD, não o de v0.30.0). Ele prova que o script
   não corrompe nem duplica; **não** prova convergência a partir de um catálogo antigo de verdade.
2. **O teste de injeção de falha NÃO teve poder de detecção — e a razão é instrutiva.** Removi uma definição de
   função do script de upgrade e o snapshot não mudou: porque o `CREATE EXTENSION VERSION '1.0.0'` já havia
   criado os 196 objetos, e um `CREATE OR REPLACE` faltante sobre objeto existente é no-op. O teste só ganha
   poder contra um catálogo que genuinamente **não tem** o objeto.
3. **Portanto o teste decisivo continua pendente:** buildar uma tag antiga (ex. v0.90.0, 60 funções), instalar,
   e só então rodar o `UPDATE` e comparar. É o método do ParadeDB (rebuild a partir da tag git). Sem ele, a
   palavra "convergente" é desenho, não evidência.
4. **ACL fora do oráculo** — `pg_depend` registra membresia, não `proacl`. Um upgrade que perca um
   `REVOKE ... FROM PUBLIC` passa no Cenário A.
5. **Cenário B1 (`.so` novo contra catálogo antigo, sem UPDATE) não foi executado.**

## Estado do DoD

| Item | Estado |
|---|---|
| Oráculo estável e sem OID | ✅ |
| `ALTER EXTENSION UPDATE` funciona | ✅ |
| Idempotência (2× sem erro, snapshot igual) | ✅ |
| Cenário A diff vazio | ✅ **mas fraco** (limite 1) |
| Falha injetada faz o teste falhar | ❌ **não provado** (limite 2) |
| Convergência de catálogo antigo real | ❌ **pendente** (limite 3) |
| Cenário B1 | ❌ **pendente** |

**Quatro itens do DoD estão abertos. O milestone não está completo, e este artefato não afirma que está.**
