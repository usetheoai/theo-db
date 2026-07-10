# ADR-0036 — M74: veredito do lever condicional de quantização (RaBitQ) — memória, não superioridade de QPS

- **Status:** Accepted (2026-07-10)
- **Contexto:** M74 (Roadmap v5) — o milestone CONDICIONAL de quantização SOTA no índice. Fecha o pilar vetorial P0.
- **Depende de:** M73 (veredito head-to-head, `docs/adr/0035`), ADR-0032 (vendor do core RaBitQ).

## Contexto (o gate condicional do M74)

O M74 só arranca se houver um lever de quantização **não já refutado** por M57 (SBQ no carrier HNSW) / M59
(anisotrópico+AH no carrier HNSW), e é **measurement-first + anti-sunk-cost (D3)**: proibido implementar o AM
completo sem blueprint com evidência de viabilidade. O DoD prevê três saídas honestas: (a) implementar com
recall≥0.99 + ganho de QPS medido; (b) ADR "nenhum lever viável"; e — a realidade medida — (c) um lever **viável
mas cujo ganho não é o que o North Star perseguia**.

## Evidência (medida, spike D3 a 1M×768d)

O lever candidato do SOTA permissivo é **RaBitQ** (Gao & Long, arXiv:2405.12497 — 1-bit, training-free, bound de
erro provado), core já vendorizado (`theodb_rs/src/rabitq/vendor/`, Apache-2.0, ADR-0032). Spike medido
(`docs/benchmarks/rabitq-spike/rabitq_ivf_mstg_1m768d.log`, consolidado em
`docs/benchmarks/vector-pillar-verdict-2026-07.md`):

| Índice RaBitQ (1M×768d) | recall pico | p50 @ pico | memória residente |
|---|---|---|---|
| MSTG-mem (grafo + RaBitQ) | 98.4% | **8.2 ms** | 3.4 GB |
| MSTG-disk (mmap) | 98.4% | 245 ms | **5.3 MB** |
| IVF-RaBitQ | 91% | 17.7 ms | — |
| _full-precision (ref M34+M60)_ | ~0.98 | ~10–15 ms | ~3 GB |

## Decisão

**O lever RaBitQ É viável e não-refutado — mas o seu ganho medido é MEMÓRIA/escala, não superioridade de QPS.**
Portanto:

1. **NÃO implementar agora o AM IVF-RaBitQ completo** perseguindo o branch (a) "ganho de QPS vs baseline" — a
   medição mostra que esse ganho não existe no nosso regime (768d): MSTG-RaBitQ-mem (8.2ms @ 98.4%) é
   **competitivo** com full-precision (~10–15ms), **não** os 25× do ScaNN. Implementar o AM completo só para igualar
   a latência que já temos seria complexidade essencial sem retorno no eixo que o milestone perseguia (anti-sunk-cost).
2. **Manter o core vendorizado (ADR-0032) como fundação** de uma feature futura de **memória/billion-scale**
   (32× compressão, 5.3 MB residentes @ 98.4% na variante disk) — posicionada como **"escala/custo"**, jamais como
   "mais rápido que o AlloyDB" (`public-copy.md`). Essa é a saída (c): lever viável, ganho real, mas fora do eixo
   de QPS-superioridade. O full AM fica **escopado como follow-up** (M-futuro), gated por demanda real de
   billion-scale (D3).
3. **O veredito de QPS-superioridade do pilar é o do M73** (`docs/adr/0035`): não-alcançável como extensão Postgres
   permissiva. O M74 confirma que o melhor quantizador permissivo do SOTA não muda esse veredito.

## Alternativas rejeitadas

- **(a) Implementar o AM IVF-RaBitQ completo agora buscando QPS-superioridade** — refutado pela medição: o ganho é
  memória, não QPS; construir o AM inteiro para não melhorar QPS é esforço sem necessidade de projeto (o esforço é
  bem-vindo quando a necessidade existe — aqui a medição diz que não existe no eixo QPS). Custo alto, retorno no
  eixo-alvo nulo.
- **(b) "Nenhum lever viável"** — desonesto: o RaBitQ **é** viável e não-refutado (correto, memória-eficiente, bound
  provado). Declarar "nenhum lever" apagaria a descoberta real (a fundação de memória que fica).
- **Perseguir o 25× do ScaNN com mais bits/rerank** — o recall do RaBitQ 1-bit trava em 98.4%; chegar a 99+ exige
  rerank que come a vantagem de latência; e nada disso fecha o gap de paradigma (AH-LUT anisotrópico do ScaNN +
  não pagar o imposto MVCC/WAL do Postgres). Fora de alcance como extensão permissiva.

## Consequências

- **Positivas:** o pilar fecha com veredito **honesto e medido** (Regra 3, Regra 5). O core RaBitQ fica pronto
  (vendorizado, atribuído) para a feature de memória quando billion-scale for demanda real. Nenhuma complexidade
  acidental adicionada (o AM completo não foi construído especulativamente — YAGNI/D3).
- **Custos / honestidade:** o eixo original do North Star (superioridade de QPS vetorial) **não** foi alcançado por
  este lever — como já documentado no M73. O M74 não inventa uma vitória; entrega a prova medida de que o SOTA
  permissivo de quantização não a alcança. O reposicionamento formal do North Star continua sendo decisão do owner
  (ADR-0033, proposto).

## Cross-references

- Evidência: `docs/benchmarks/vector-pillar-verdict-2026-07.md`, `docs/benchmarks/rabitq-spike/rabitq_ivf_mstg_1m768d.log`
- Veredito do pilar (M73): `docs/adr/0035-m73-northstar-vector-verdict.md`
- Vendor do core: `docs/adr/0032-vendor-rabitq-core.md`
- Reposicionamento (proposto, owner): `docs/adr/0033-north-star-reposition-proposal.md`
- Regras: `public-copy.md` (§3 honestidade), Unbreakable Rule 3 (honestidade), Rule 5 (perf = claim medida)
