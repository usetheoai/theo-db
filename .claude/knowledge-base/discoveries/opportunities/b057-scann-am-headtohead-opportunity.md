---
item: B-057
repo: theo-db
mode: evolve
date: 2026-08-17
verdict: pending
measured_on: droplet theo-b059-bench · 138.197.22.192 · s-8vcpu-16gb · nyc3
dataset: sift-128-euclidean · sha256 dd6f0a6ed6b7ebb8934680f861a33ed01ff33991eaee4fd60914d854a0ca5984
---

# B-057 — o gap do ADR-0035 mediu a biblioteca; contra o access method ele colapsa

Todas as medições no droplet efêmero `138.197.22.192`, com o **mesmo SIFT** verificado por checksum, na **mesma
máquina** e no **mesmo arnês**.

## Corner 1 — Evidence

### A hipótese do item, e o que a medição fez com ela

`ADR-0035` atribui o gap de **~25–44×** em QPS a *"AH-LUT anisotrópico **+ não pagar o imposto MVCC/WAL**"*, e
`wiki/benchmarks/m33-scann-headtohead.md:21` declara o ScaNN OSS como *"o proxy sancionado"*. O item aponta que
a segunda metade dessa causa **não se aplica ao produto**: o AlloyDB expõe `CREATE INDEX … USING scann`, um
access method do PostgreSQL, que paga o mesmo imposto de página, MVCC e WAL que o `theodb_hnsw`.

Medido, SIFT-128 a 100 000 vetores, k=10, 500 queries, mesma máquina:

| recall casado | `theodb_hnsw` | `scann` AH + rescore | razão |
|---|---|---|---|
| ≈ 0,96 | 365,6 QPS @ 0,9616 | **438,8 QPS @ 0,9590** | scann **1,20×** |
| ≈ 0,996 | 148,9 QPS @ 0,9956 | **244,7 QPS @ 0,9958** | scann **1,64×** |

**1,2–1,6×, não 25×.** A mesma ordem de grandeza.

Frontieras completas:

```
theodb  theodb_hnsw m=16   ef=16   517,0 QPS  recall 0,8688
                            ef=64   365,6 QPS  recall 0,9616
                            ef=256  148,9 QPS  recall 0,9956

omni    scann num_leaves=316 quantizer=AH pre_reordering=100
                            leaves=5    608,4 QPS  recall 0,7262
                            leaves=20   438,8 QPS  recall 0,9590
                            leaves=80   244,7 QPS  recall 0,9958
```

### Três configurações erradas antes de chegar à certa, e cada uma teria produzido um número publicável

Este é o achado metodológico, e vale tanto quanto o número.

**(1) Sem `LOAD 'alloydb_scann'`**, `SET scann.num_leaves_to_search` sucede, `current_setting` devolve o valor
escrito e `pg_settings` não lista o GUC. O portão do [[B-060]] recusou; sem ele, a corrida teria varrido três
pontos idênticos no default `0`.

**(2) Com `quantizer='SQ8'`** — o default da suíte que eu escrevi primeiro. A corrida deu `VALID` e produziu
uma fronteira. Mas o `ADR-0035` credita o gap ao **AH**, e `quantizer='AH'` **falha** com
`AH quantization is not enabled for the index` a menos que `scann.enable_ah_quantizer` esteja ligado **no
momento do build**. Valores válidos: `SQ8`, `Flat`, `AH`; o flag vem `off`. Eu ia responder uma pergunta sobre
AH com uma medição de quantização escalar.

**(3) Com AH e sem rescore.** A fronteira AH deu teto em recall **0,6582**, e as duas primeiras linhas já
diziam por quê: 4× mais leaves compraram **1,4 ponto** de recall. Isso é erro de quantização, não profundidade.
`scann.pre_reordering_num_neighbors` — o número de candidatos quantizados rescoreados com distância exata — vem
`-1`. Medido, mesmo índice, mesmos 80 leaves:

```
pre_reordering_num_neighbors = -1   (default)  →  recall@10 = 0,6568
pre_reordering_num_neighbors = 100            →  recall@10 = 0,9964
pre_reordering_num_neighbors = 500            →  recall@10 = 0,9998
```

**O teto era inteiramente o rescore ausente.** Publicar "o scann do AlloyDB teto em 0,66 enquanto o nosso chega
a 0,9956" seria alegação falsa contra outro produto — e a mais perigosa das que este projeto rastreia, porque
**nos favorecia**. É a mesma classe do [[B-034]] e do [[B-041]], apontada para fora em vez de para dentro.

### A armadilha do `LOAD`, verificada (bullet 3 do DoD)

Verificada no [[B-059]] contra o servidor real: `pg_settings` tem **1** entrada `scann%` em sessão nova e **111**
depois de `LOAD 'alloydb_scann'`; `shared_preload_libraries` não carrega a biblioteca. `SET` sucede, o motor
busca no default. Registrado com a medição completa em
`.claude/knowledge-base/discoveries/opportunities/b059-omni-adapter-opportunity.md § M5/M6/M7`.

## Corner 2 — Constraint relation

`unknown` — `rules/current-constraint.md` está `status = undeclared`.

## Corner 3 — Blast radius

| Alcance | Detalhe |
|---|---|
| `wiki/decisions/0035-m73-northstar-vector-verdict.md` | **atualização por acréscimo obrigatória** — o veredito "NÃO-ALCANÇÁVEL" repousa numa causa cuja segunda metade não se aplica ao produto |
| `wiki/decisions/0002-north-star-equal-or-superior-to-alloydb.md` | LOCKED. Não é alterado aqui; a medição informa a decisão do owner |
| `wiki/decisions/0033-north-star-reposition-proposal.md` | proposta de reposicionamento pendente de assinatura — este dado é material para ela |
| `wiki/benchmarks/` | um artefato novo com esta medição, e uma nota no `m33-scann-headtohead.md` sobre o que "proxy sancionado" significa |
| `CLAUDE.md` § North Star | a frase *"superioridade de QPS sobre o ScaNN/AlloyDB MEDIDA como NÃO-ALCANÇÁVEL … gap ~25-44× @ 0.99 é de paradigma"* passa a ter uma medição que a contradiz **para o produto** e a confirma **para a biblioteca** |
| `README.md` | nenhuma alegação nova sem o rigor da regra 5 |

## Corner 4 — Verification

1. As duas fronteiras vêm de bundles `VALID` do mesmo arnês, mesma máquina, mesmo dataset verificado por
   checksum. **FEITO.**
2. A comparação é a recall casado, nunca knob a knob. **FEITO.**
3. A configuração do concorrente é a que o `ADR-0035` credita — AH **e** rescore — e cada knob é provado em
   vigor pelo portão. **FEITO.**
4. O `ADR-0035` é atualizado por acréscimo, dizendo o que encolheu e o que não. **PENDENTE.**
5. A medição a **1M**, que é a escala do `ADR-0035`. **EM EXECUÇÃO** — sem ela, o número de 100k não é
   comparável ao do ADR, e essa ressalva é a mais séria deste item.

## Ressalvas que o número carrega, e nenhuma é opcional

1. **Escala.** 100 000 vetores; o `ADR-0035` mediu a 1M. Índice IVF-com-quantização e índice de grafo não
   escalam igual, e o gap pode abrir. Comparar 100k com 1M seria comparar dois experimentos.
2. **`(unstable)`** em quase todas as linhas — o próprio arnês marca. Perfil `smoke`, 3 repetições, sem teste
   de significância pareado (`papers/rigorous-perf-eval-georges-2007.pdf`).
3. **Majors diferentes:** TheoDB em PostgreSQL 18.6, Omni em 17.9.
4. **Nenhum dos lados foi tunado.** `num_leaves=316` (≈√100 000) e `pre_reordering=100` são escolhas; `m=16`
   do nosso lado também. Um ScaNN tunado pode ir melhor; um HNSW com `m` maior também.
5. Uma corrida por sistema.

Nada disto é claim de performance. É a resposta a *"o gap encolhe ao medir contra o AM em vez da biblioteca?"* —
e a resposta medida é **sim, drasticamente**.
