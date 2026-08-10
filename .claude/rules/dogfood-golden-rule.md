# Dogfood Golden Rule

Locked contract that `/dogfood` reads to evaluate whether a project may legitimately claim `production-ready` / `v1.0`. **This file is a template — each project edits the marked sections to declare its own anchor scenario.**

Without this file, `/dogfood` emits `EVIDENCE_INSUFFICIENT` with flag `golden_rule_missing`.

## § 1 — Anchor scenario (PER-PROJECT — EDIT THIS)

The anchor scenario is the single use case that, if you cannot dogfood it, you cannot claim production-ready. Pick one. Be specific.

**Slug:** `theo-rag-sobre-theodb`

**Description:** O **`theo-rag`** — produto de RAG do próprio ecossistema, que serve usuários — passa a usar
o **TheoDB** como vector store, em vez do pgvector, na infraestrutura que o time opera. Ingestão e consulta
reais de usuário passando pelo `theodb_hnsw` e pela superfície `theodb.embed`/híbrida.

**Estado medido em 2026-08-09, e é o que torna este âncora o certo:** `theo-rag/package.json` declara
`"compose:up": "docker compose up -d pgvector"`, e o `theo-memory` faz o mesmo. **Os produtos de IA do time
usam a extensão de um concorrente, não o banco que o time constrói.** Enquanto isso for verdade, "production
ready" é uma alegação que os próprios autores não sustentam com o próprio uso.

**Why this scenario:** A promessa primária do TheoDB é ser um banco PostgreSQL-compatible cujas capacidades
vetorial e de IA são **próprias**, não uma colagem de extensões de terceiros. O `theo-rag` é exatamente a carga
que essa promessa existe para servir — e ele hoje escolhe o pgvector.

É desconfortável na medida que a regra pede: se o produto não aguenta o RAG do próprio time, não aguenta o de
ninguém; e se aguentar, a migração produz evidência que nenhum benchmark sintético produz — dado real,
consulta real, falha real. O contraponto honesto é que **migrar tem custo e risco para um produto que já
funciona**, e é por isso que este âncora vale: ele só é escolhido se o time realmente acreditar no banco.

## § 2 — Status vocabulary (LOCKED — do not change without ADR)

The `Status` field in `knowledge-base/dogfood/manifest.md` MUST take one of these values:

| Status | Meaning |
|---|---|
| `planned` | Anchor is identified but no implementation work has started. |
| `wired` | Implementation lands; the anchor is invoked at least once in CI or a manual smoke. |
| `running` | The anchor is **actively used by the team on the deployed dev environment** (`app-dev.usetheo.dev`). This is the bar for v1.0. |
| `paused` | Was `running`; explicitly stopped for a documented reason. NOT a degradation of `running`. |
| `abandoned` | Anchor is no longer pursued. Requires ADR to set. |

`/dogfood` accepts `running` as the only value satisfying hard cap #2 (`anchor_not_running`).

## § 3 — Hard caps (LOCKED)

In order; first failure short-circuits to `EVIDENCE_INSUFFICIENT`.

| # | Check | Flag |
|---|---|---|
| 1 | Manifest contains a section identifiable by `Slug` or anchor header | `anchor_missing` |
| 2 | `Status` matches the running value declared in § 2 | `anchor_not_running` |
| 3 | At least one evidence file under `knowledge-base/dogfood/evidence/` has frontmatter `scenario:` matching the anchor slug | `no_anchor_evidence` |
| 4 | The most recent matching evidence file (by frontmatter `date:`) is within the freshness threshold below | `anchor_evidence_stale` |

**Ambiente do âncora (PER-PROJECT — decisão do owner, 2026-08-10):** o alvo é **`app-dev.usetheo.dev`**, o
ambiente de desenvolvimento implantado que o time opera — não produção.

A troca é do owner e está registrada aqui em vez de aplicada em silêncio. O que ela muda: `running` deixa de
exigir produção e passa a exigir o `theo-rag` **rodando no `app-dev` sobre o TheoDB, servindo consultas**.
O que ela **não** muda: continua exigindo **uso**, não instalação — um serviço implantado que ninguém
exercita não é dogfood, é um contêiner ligado.

**Medido em 2026-08-10, antes da troca:** `app-dev.usetheo.dev` responde 200 em 0,5 s, **mas devolve a SPA
para qualquer rota** — `/api/rag/health` retornou HTML, e uma rota inventada também deu 200. **Não há
evidência de que o `theo-rag` esteja implantado lá**, nem sobre qual banco. Registrar isto é o ponto: o gate
novo aponta para um ambiente cujo estado ainda não foi verificado, e a primeira coisa que ele exige é
verificá-lo.

**Freshness threshold (PER-PROJECT — EDIT THIS):** `30 days` by default. Reduce for fast-moving products; never raise without ADR.

## § 4 — Soft caps (PER-PROJECT — EDIT OR EXTEND)

Soft caps cap the verdict at `EVIDENCE_WITH_CAVEATS`. They fire when hard caps pass but evidence is thin.

| Soft cap | Default | Rationale |
|---|---|---|
| Total evidence count for the anchor | ≥ 3 | Single evidence point is not a trend. |
| Failure stories present | ≥ 1 | A dogfood without failures is theatre. |
| Evidence from ≥ 2 different operators | recommended | Avoid "the one person who knows how" syndrome. |

## § 5 — Evidence file frontmatter (LOCKED)

Every file under `knowledge-base/dogfood/evidence/` MUST have YAML frontmatter:

```yaml
---
scenario: <slug>        # matches the anchor slug or a declared sibling
date: YYYY-MM-DD        # local date of the dogfood run
operator: <name>        # who ran it
outcome: pass | partial | fail
summary: <one line>
---
```

Missing any field → evidence file ignored by hard cap #3.

## § 6 — When this rule may change

Per `cycle-rule-schema.md § Golden Rule Change Protocol`, plus one extra requirement:
sign-off from at least one operator who has logged anchor evidence. Rule-specific
deviations:

- Changing the anchor slug = abandoning the previous anchor (requires ADR).
- Loosening the freshness threshold = downgrading the gate (requires ADR).
- Adding a new `Status` value or new hard cap = expanding the contract (requires ADR).

## § 7 — Failure modes the rule guards against

- "Production-ready" claim backed only by synthetic benchmarks.
- Silently swapping the anchor when the original becomes inconvenient.
- Aging evidence (dogfood worked 6 months ago; nothing since).
- Single-operator knowledge (only one person can actually run the anchor).
- Dogfood theatre — checking the box without using the product.
