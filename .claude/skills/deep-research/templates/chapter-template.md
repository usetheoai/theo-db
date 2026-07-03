# Capítulo {NN} — {Título}

> Status: 🟢 ORIGINAL (ancorado no nosso código) | 🔵 CURADO (trilha de leitura) | 🟡 ROADMAP (aspiracional).
> Preencha as 5 camadas se 🟢. Se 🔵/🟡, use a variante curada (§ ao fim). Toda citação `arquivo:linha` DEVE
> resolver no disco; todo número DEVE ter link de benchmark OU o marcador `UNBENCHMARKED`.

**Pré-requisitos:** {capítulos anteriores relevantes}.

---

## {NN}.1 — TEORIA: {o conceito}

{A ideia, a intuição, o paper seminal. Onde nasce, qual problema resolve, por que é usado. Uma figura/diagrama
ASCII se ajudar. ≥ 2 fontes primárias.}

> **Paper seminal:** {Autor(es), Título, Venue Ano — arXiv/DOI}. {Uma linha do porquê ler.}

---

## {NN}.2 — MATEMÁTICA: {as fórmulas e a complexidade}

{As fórmulas com LaTeX. A complexidade de build e de query (Big-O). Os parâmetros e o que cada um troca.}

| Símbolo | Papel | No TheoDB |
|---|---|---|
| {sym} | {o que faz} | {const/GUC/reloption real} |

- **Busca:** O({...}) — {a assinatura empírica que prova isso}.
- **Build:** O({...}) — {o custo real medido}.

---

## {NN}.3 — NOSSA IMPLEMENTAÇÃO

{Como o TheoDB implementa. CADA afirmação com `arquivo:linha` real. Trace o fluxo pelo código.}

- **{Componente}:** `theodb_rs/src/{...}:{linha}` — {o que faz}.
- **Decisão de engenharia:** {o que divergiu da teoria e por quê} — ADR `docs/adr/{...}.md`.

> **Decisão registrada:** {blueprint/ADR que ancora}.

---

## {NN}.4 — NOSSO BENCHMARK

{Todo número daqui vem de `docs/benchmarks/{artefato}.json` — dataset, hardware, comando de repro. Sem artefato →
`UNBENCHMARKED` + a medição proposta.}

| {param} | recall@k | QPS | p50 |
|---|---|---|---|
| {...} | {...} | {...} | {...} |

**Trade-off honesto:** {o custo que pagamos — build lento, memória, etc.}.

---

## {NN}.5 — SOTA & GAP HONESTO

{Onde estamos no mapa do estado da arte, em condições CASADAS (mesmo recall). Onde ganhamos, onde perdemos, com o
número. O caminho que fecharia o gap (aponte pro capítulo/milestone).}

- **vs {SOTA}:** {o número em recall casado} — {ganhamos/perdemos/paridade}. Fonte: `docs/benchmarks/{...}.json`.

---

## {NN}.6 — Pontos-chave

1. {...}

## {NN}.7 — Exercícios

1. **(Leitura de código)** {rastrear um fluxo pelo código real}.
2. **(Matemática)** {um cálculo}.
3. **(Experimento)** {rodar um benchmark e interpretar}.

## Referências

- **Paper seminal:** {...}
- **Nossos artefatos:** blueprint `{...}`, benchmark `docs/benchmarks/{...}.json`, ADRs `{...}`.
- **Código:** `theodb_rs/src/{...}`.
- **Referência de implementação SOTA:** {peer em `.claude/knowledge-base/references/` ou paper}.

---

<!-- VARIANTE CURADA (🔵/🟡) — use no lugar das 5 camadas quando o tópico é fundamento ou roadmap:

## {NN}.1 — Por que isto importa no TheoDB
{A conexão: onde no nosso sistema este fundamento aparece / apareceria.}

## {NN}.2 — Trilha de leitura (as fontes canônicas)
{Lista anotada: o que ler, em que ordem, e o que extrair de cada. NÃO reproduza o conteúdo — aponte pra ele.}

## {NN}.3 — Aterrissagem
{O `arquivo:linha` do nosso código onde este conceito é usado, se 🟢-adjacente; ou o marcador 🟡 roadmap.}
-->
