# ADR 0001 — Sem fork do engine PostgreSQL

**Status:** Accepted  
**Date:** 2026-06-26  
**Author:** @paulohenriquevn  
**References:** PRD §15 D3, CLAUDE.md TheoDB Rule 3

---

## Contexto

TheoDB entrega PostgreSQL-compatibilidade como produto. A equipe precisa decidir como
obter a extensão de vetores (`pgvector`) sem violar a licença Apache 2.0 e sem perder
wire-compatibility com PostgreSQL 17.

Três alternativas foram avaliadas:

---

## Alternativas avaliadas

### A1 — Extension model (extensão via `CREATE EXTENSION`)
Compilar `pgvector v0.8.3` como extensão PostgreSQL e instalá-la no container sobre
`postgres:17-bookworm` (imagem oficial). Nenhum código do engine PostgreSQL é modificado.

**Adotada.** Ver §§ Decisão.

### A2 — Engine fork com patch de vetores embutido
Forkar o repositório do PostgreSQL e inserir o código de pgvector diretamente no
código-fonte do engine (modo `inline`).

**Rejeitada.** Motivos:
- Viola CLAUDE.md TheoDB Rule 3 ("Sem fork do engine PostgreSQL").
- Elimina 100% de wire-compatibility garantida por psql/libpq oficiais.
- Gera custo de manutenção de rebase a cada release minor do PG (≥ 4 releases/ano).
- Não tem precedente OSS permissivo que prove que essa estratégia escala.

### A3 — Construção do zero (scratch engine com vetores nativos)
Implementar um engine PostgreSQL-wire-compatible do zero com suporte a vetores nativo.

**Rejeitada.** Motivos:
- Escopo incompatível com V1 (multi-anos de engenharia; zero prior art permissivo nessa escala).
- Wire-compatibility com PostgreSQL é gate de produto (CLAUDE.md TheoDB Rule 6); construir
  do zero requer implementar o protocolo wire PG completo — uma tarefa de anos sem garantia.
- Violaria Unbreakable Rule 9 (não reinvente a roda) e Unbreakable Rule 11 (YAGNI).
- Risco de fabricar um clone incompatível em vez de um produto compatível.

---

## Decisão

**Adotar A1 — Extension model.**

Compilar `pgvector v0.8.3` (Apache 2.0) como extensão PostgreSQL sobre a imagem oficial
`postgres:17-bookworm`. O engine PostgreSQL não é modificado. A extensão é instalada via:

```dockerfile
ADD https://github.com/pgvector/pgvector.git#v0.8.3 /tmp/pgvector
RUN make OPTFLAGS="" && make install
```

E carregada em tempo de uso via:

```sql
CREATE EXTENSION IF NOT EXISTS vector;
```

---

## Consequências

**Positivas:**
- Wire-compatibility 100% com PostgreSQL 17 garantida (sem alteração no engine).
- Build reproduzível e auditável — SBOM via `pgvector v0.8.3` Apache 2.0.
- Atualização de pgvector desacoplada do engine (upgrade independente).
- `CREATE EXTENSION` é o mecanismo oficial de extensibilidade do PostgreSQL.

**Riscos e mitigações:**
- **ABI drift:** quando o engine PG avança de versão major (17→18), o `.so` da extensão
  precisa ser recompilado. Mitigação: CI re-build automático por versão PG declarada.
- **Política de fork (D3):** se o upstream pgvector ficar para trás dos requisitos de
  desempenho, o PRD D3 permite fork com benchmark reproduzível como gatilho. Esse ADR
  não bloqueia o fork — apenas define que o engine PostgreSQL em si não é forkado.

---

## Rationale técnico adicional

O modelo de extensões do PostgreSQL foi projetado exatamente para esse padrão:
`pg_vector`, `pg_trgm`, `PostGIS` e dezenas de outras extensões de produção seguem
essa arquitetura. O AlloyDB (nosso SOTA anchor) usa o mesmo mecanismo para sua
extensão vetorial baseada em ScaNN. Não há custo de complexidade essencial em seguir
o mesmo modelo.

---

## Referências

- PRD §15 D3 — "Extensões (pgvector/pgvectorscale) podem ser forkadas sob a Política
  de Fork; o engine PostgreSQL não."
- CLAUDE.md TheoDB Rule 3 — "Sem fork do engine PostgreSQL."
- pgvector v0.8.3 — `knowledge-base/references/pgvector/`
- AlloyDB SOTA — `knowledge-base/references/alloydb-omni/`
