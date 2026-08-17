---
type: Decision
title: ADR 0035 — Veredito MEDIDO do north star vetorial contra ScaNN/AlloyDB
description: Paridade own-code de recall alcançada; superioridade de QPS sobre o ScaNN medida como não-alcançável por extensão Postgres permissiva; throughput multi-cliente superior no regime 128d clusterizado.
resource: git:f7c7b93:docs/adr/0035-m73-northstar-vector-verdict.md
tags: [adr, veredito, north-star, scann, honest-negative, m73]
adr_id: "0035"
adr_status: Accepted
decision_date: 2026-07-10
milestone: M73
generated: { by: claude-code/opus-5, at: 2026-08-07T16:12:05Z }
sources:
  - id: adr0035
    resource: git:f7c7b93:docs/adr/0035-m73-northstar-vector-verdict.md
    title: ADR-0035 — M73 veredito do North Star vetorial
    last_modified: 2026-07-10
---

O veredito rastreável que o [ADR 0002](/decisions/0002-north-star-equal-or-superior-to-alloydb.md)
exigia. Registra **onde o TheoDB está**, não uma mudança de mandato — essa é decisão separada, no
[ADR 0033](/decisions/0033-north-star-reposition-proposal.md).

# Evidência consolidada

## Eixo 1 — TheoDB contra pgvector

**Recall: paridade de valor alcançada.** O gap foi fechado pelo
[extendCandidates](/decisions/0034-hnsw-extend-candidates-navigability.md): f32 de 0,974 para
**0,990**, SBQ de 0,986 para **0,994** a 500k, contra 0,994 do pgvector.

**Fronteira de latência, honesta:** a iso-recall alta, o TheoDB ainda precisa de ~1,8× o `ef` do
pgvector a 500k — o fix subiu o teto de recall, não igualou a eficiência de recall por `ef`.

**Multi-cliente** ([m72](/benchmarks/m72-qps-multiclient.md), 1M × 128d, 8 clientes concorrentes): a
recall casado ~0,91, o `theodb_hnsw` **supera** o pgvector — 0,917 a 597,7 QPS contra 0,9095 a 539,5
QPS, isto é **+11%**, com p50 de 13,6 contra 16,5 ms — e alcança um recall (0,97 a 354 QPS) em que o
pgvector platôa antes (~0,914) **neste regime clusterizado de 128d**, que é justamente o regime-alvo
do extendCandidates. O build também é ~3× mais rápido. **Honesto:** é o regime favorável ao TheoDB; a
fronteira de alta dimensão e alto recall permanece do pgvector.

> **ACRÉSCIMO 2026-08-16 — o que este veredito mediu, dito com precisão.** O gap de ~25× foi medido contra a
> **biblioteca ScaNN OSS** ([m33](/benchmarks/m33-scann-headtohead.md) a declara "proxy sancionado"). O produto
> concorrente não expõe a biblioteca: expõe `CREATE INDEX ... USING scann`, **um access method do PostgreSQL**,
> que paga o mesmo imposto de MVCC, WAL e página que o `theodb_hnsw`. Como este ADR atribui o gap a
> "AH-LUT anisotrópico **+ não pagar o imposto MVCC/WAL**", **a segunda metade da causa não se aplica ao AM** —
> só à biblioteca.
>
> Isto **não** invalida o veredito: a vantagem algorítmica do AH-LUT é real e medida, e nenhuma medição nova a
> contradiz. O que muda é o que se pode afirmar sobre o **produto**: contra o `scann` AM, o gap não foi medido.
>
> O AlloyDB Omni traz o ScaNN e o colunar sem GCP (`docker pull google/alloydbomni:18`), então a comparação que
> o `ADR-0061` exige — concorrente na mesma corrida, na mesma máquina — passou a ser possível. Registrado como
> [[B-057]]. Gatilho: avaliação independente de AlloyDB publicada em 2026-08-15 (boringSQL / Radim Marek), que
> mediu o `scann` AM com índice 30× menor que o ivfflat e build 7–9× mais rápido, e **não conseguiu estabelecer
> a recall** — obteve 0,15 e identificou a causa: `scann.num_leaves_to_search` não tem efeito sem
> `LOAD 'alloydb_scann'`, sem aviso. O avaliador declara não confiar no número e não o publica; registramos a
> não-reprodução, não uma refutação.

> **ACRÉSCIMO 2026-08-17 — o gap FOI medido contra o access method, e ele colapsa.** O [[B-057]] rodou a
> comparação que o acréscimo acima dizia ser possível: `theodb_hnsw` e `scann` no **mesmo arnês**
> (`theodb-bench`), na **mesma máquina** (droplet efêmero `138.197.22.192`, 8 vCPU / 16 GB), com o **mesmo
> SIFT-128** verificado por checksum (`dd6f0a6e…ca5984`), comparados a **recall casado**.
>
> A **100 000 vetores**, k=10, 500 queries:
>
> | recall casado | `theodb_hnsw` | `scann` AH + rescore | razão |
> |---|---|---|---|
> | ≈ 0,96 | 365,6 QPS @ 0,9616 | 438,8 QPS @ 0,9590 | scann **1,20×** |
> | ≈ 0,996 | 148,9 QPS @ 0,9956 | 244,7 QPS @ 0,9958 | scann **1,64×** |
>
> **1,2–1,6×, não 25×.** A hipótese do B-057 estava certa: a segunda metade da causa que este ADR atribui ao gap
> — "não pagar o imposto MVCC/WAL" — é o que respondia pela maior parte dele. O AM paga o mesmo imposto que nós,
> e sobra a vantagem algorítmica do AH-LUT, que nesta medição vale **menos de 2×**.
>
> **A configuração importou mais que o resultado, e três tentativas erradas vieram antes da certa** — cada uma
> produzindo um número que parecia publicável:
>
> 1. Sem `LOAD 'alloydb_scann'`: `SET scann.num_leaves_to_search` sucede, `pg_settings` não lista o GUC, a busca
>    corre no default `0`. É a armadilha que o avaliador independente documentou; o portão de knob do [[B-060]]
>    recusou a corrida.
> 2. Com `quantizer='SQ8'`: `VALID`, fronteira completa — e **quantização escalar**, não o AH que este ADR
>    credita. `quantizer='AH'` **falha** com `AH quantization is not enabled for the index` a menos que
>    `scann.enable_ah_quantizer` esteja ligado **no build**; o flag vem `off`.
> 3. Com AH e sem rescore: teto de recall em **0,658**, e 4× mais leaves comprando 1,4 ponto — assinatura de erro
>    de quantização. `scann.pre_reordering_num_neighbors` vem `-1`; medido no mesmo índice e nos mesmos 80 leaves,
>    `-1` → **0,6568**, `100` → **0,9964**, `500` → **0,9998**.
>
> A terceira é a que importa registrar: publicar *"o scann do AlloyDB teto em 0,66 enquanto o nosso chega a
> 0,9956"* seria **alegação falsa contra outro produto**, e a mais perigosa das que este projeto rastreia, porque
> nos favorecia. A classe [[B-034]]/[[B-041]] apontada para fora.
>
> **Ressalvas, e nenhuma é opcional.** A escala medida é **100 000**, e este ADR mediu a **1M** — índice
> IVF-com-quantização e índice de grafo não escalam igual, e comparar as duas escalas seria comparar dois
> experimentos. Quase todas as linhas vêm `(unstable)` do próprio arnês (perfil `smoke`, 3 repetições, sem teste
> pareado de significância). TheoDB em PostgreSQL **18.6**, Omni em **17.9** — a corrida cruza uma major. E
> **nenhum dos lados foi tunado**: `num_leaves=316` (≈√100 000), `pre_reordering=100` e `m=16` são escolhas.
>
> **O que este acréscimo autoriza e o que não autoriza.** Autoriza: dizer que *contra o access method do produto
> concorrente, a 100k, a recall casado, o gap é de ordem 1,2–1,6× e não de 25×*. **Não** autoriza reescrever o
> item 2 do veredito abaixo — o "NÃO-ALCANÇÁVEL" foi medido contra a biblioteca e continua verdadeiro sobre ela.
> O que ele torna insustentável é usar aquele item como se falasse do **produto**: para o produto, a evidência
> agora aponta na direção oposta, e fechar 1,6× é uma pergunta de engenharia, não de paradigma.
>
> Reabrir o veredito é decisão do owner, e depende do número a 1M. Evidência completa em
> `.claude/knowledge-base/discoveries/opportunities/b057-scann-am-headtohead-opportunity.md`.

> **CORREÇÃO DO ACRÉSCIMO ACIMA — 2026-08-17, mesmo dia, algumas horas depois.** O acréscimo anterior compara
> **o índice errado do nosso lado**, e é preservado em vez de reescrito porque a alegação já estava no disco.
>
> Ele mediu `theodb_hnsw m=16` — **grafo puro, sem quantizador, sem AH, sem rescore** — contra o `scann` do
> AlloyDB **com AH e rescore**. Isso compara o nosso índice de grafo com o IVF-quantizado deles: uma comparação
> real, e **não** a que este ADR pede. O TheoDB **tem** a receita do ScaNN, e o arco no código chama-se
> literalmente `pg_scann` (M75 construiu o algoritmo em `ann/ivf_aqah.rs`, M77 o access method com páginas v4
> persistidas em `am/scan.rs::scan_ivf_aq`):
>
> | peça do ScaNN | onde está no TheoDB |
> |---|---|
> | partição IVF | `theodb_ivfflat` `WITH (lists = N)` |
> | quantizador anisotrópico | `WITH (pq_subspaces = M)` → `AqQuantizer` |
> | AH-LUT batched | `pq_bits = 4` (LUT16 `pshufb`), `ah_score_block` layout block32 |
> | o T anisotrópico | `WITH (aq_threshold = …)` |
> | SOAR | `WITH (soar_lambda = …)` |
> | rescore exato (stage 2) | `WITH (separate_storage=1, refine=1)`, pool = `64 × theodb_hnsw.over_fetch` |
>
> Verificado por execução: `CREATE INDEX … USING theodb_ivfflat (emb theodb_ivfflat_l2_ops) WITH (lists=20,
> pq_subspaces=16, pq_bits=4, separate_storage=1, refine=1)` constrói, e o plano usa `Index Scan using aq_i`.
>
> **O que o número anterior autoriza, com precisão:** *"o `scann` AM do AlloyDB é 1,2–1,6× mais rápido que o
> `theodb_hnsw` a recall casado, a 100k SIFT-128"*. **Não** autoriza nada sobre o `pg_scann`, que é o índice
> que este ADR de fato compara.
>
> Isto é a mesma classe de erro que o acréscimo anterior registra ter pego três vezes no lado do concorrente —
> medir a configuração errada e chamar de comparação — desta vez do nosso lado. A medição pareada
> (`vector/sift/pg-scann` × `vector/sift/scann-ah`, ambos com os dois estágios e ambos pelo arnês) está em
> curso; o resultado entra aqui como novo acréscimo.
>
> **Achado intermediário, já medido e já útil:** a primeira fronteira do `pg_scann` teto em **recall 0,8212**
> com o pool de rescore verificado em 128 candidatos para k=10. Cento e vinte e oito rescores exatos não podem
> limitar recall a 0,82 a menos que os vizinhos verdadeiros **não estejam** no conjunto de candidatos — logo o
> teto é erro de quantização do estágio 1, não profundidade de probe. `pq_subspaces=16` sobre 128 dimensões são
> **8 dimensões por subespaço**; o ScaNN e o FAISS usam 2. Está sendo varrido (16/32/64) como suíte registrada,
> porque é parâmetro de build e não de busca.

## Eixo 2 — o gap de paradigma até o ScaNN

O head-to-head [m33](/benchmarks/m33-scann-headtohead.md) mediu o [ScaNN](/technologies/scann.md)
~**25×** acima em QPS. A vantagem é quantização anisotrópica mais Asymmetric Hashing com LUT SIMD —
não grafo em precisão plena.

O melhor quantizador permissivo do SOTA, o [RaBitQ](/technologies/rabitq.md), medido a 1M × 768d: 8,2
ms a 98,4% de recall — **competitivo** com precisão plena (~10–15 ms), **não** 25×. O ganho dele é
**memória** (5,3 MB residentes na variante em disco), não QPS.

# O veredito

1. **Paridade own-code classe-pgvector de RECALL: ALCANÇADA.** O TheoDB tem tipo vetorial próprio,
   access method HNSW próprio, e recall de valor equivalente ao pgvector a 500k.
2. **Superioridade de QPS vetorial sobre o AlloyDB/ScaNN: NÃO-ALCANÇÁVEL** como extensão Postgres
   permissiva. Perseguida por todos os caminhos honestos e medida: os 25× do ScaNN vêm do algoritmo
   dele — AH-LUT anisotrópico em 128d, com anos de tuning — somados ao fato de **não pagar o imposto
   de MVCC, WAL e heap** que qualquer extensão paga.
3. **Trade-off documentado:** código próprio, paridade de recall, e throughput multi-cliente
   **competitivo a superior no regime 128d clusterizado**, com a **fronteira de alta dimensão e alto
   recall ainda do pgvector**. Regime-dependente, medido, sem claim universal.

**Posicionamento permitido:** "paridade de recall classe-pgvector com índice vetorial próprio" e
"eficiência de memória RaBitQ para billion-scale". **Jamais** "mais rápido que o AlloyDB no
vetor".[^adr0035]

# Alternativas rejeitadas

**Declarar superioridade** — nenhum benchmark a sustenta; o oposto foi medido. **Declarar fracasso do
pilar** — desonesto na outra direção: a paridade own-code de recall **é** entrega real, e a fundação
de memória é diferencial genuíno. **Adiar o veredito esperando uma alavanca mágica** — os caminhos já
foram medidos; o veredito honesto é o entregável.

# Consequências

O north star ganha a **prova medida de onde o TheoDB está**, com rastreabilidade total.

**Honestidade:** o eixo "superar o AlloyDB no QPS vetorial" é medido como não-alcançável por extensão
permissiva. **Isso não é falha de execução — é a fronteira do que a arquitetura permite.** Os
diferenciais reais ficam em abertura, portabilidade, independência de modelo, AI-native/HTAP e
custo/escala, não em QPS vetorial puro.

Confirmado e estendido pelo caminho do access method no
[ADR 0037](/decisions/0037-m82-am-ivf-aq-measured-verdict.md).

[^adr0035]: ADR-0035 — M73: veredito MEDIDO do North Star vetorial vs ScaNN/AlloyDB
