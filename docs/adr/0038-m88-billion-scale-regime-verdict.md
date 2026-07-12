# ADR-0038 — M88: veredito terminal da track storage-separation (SQ8 vs f32 no regime out-of-RAM)

- **Status:** Accepted (2026-07-12)
- **Contexto:** M88 (Roadmap v7, track storage-separation M83→M88) — a **medição terminal** da hipótese de que,
  num regime onde os dados de refine f32 **não cabem em RAM**, a separação de armazenamento com refine SQ8 (v6,
  ~4× menor que o f32 v5) converte a vantagem de **memória** em vantagem de **QPS** (menos páginas lidas do disco
  por query). Rodado dentro do Postgres num box DO `m-8vcpu-64gb` (Intel Xeon Platinum 8280, 62 GB usáveis).
- **Natureza:** registra um **veredito medido** (parcial/inconclusivo honesto), não uma mudança de mandato. O
  mandato LOCKED permanece `docs/adr/0002`; o reposicionamento do North Star é `docs/adr/0033` (decisão do owner).
- **Relação:** **estende** o `docs/adr/0037` (M82, veredito medido do pg_scann-as-AM) e o `docs/adr/0035` (M73,
  teto do pilar vetorial) com a medição terminal da separação de armazenamento. Fecha a track M83→M88.

## Decisão

O veredito terminal da track storage-separation é **`SIZE_CONFIRMED / OUT_OF_RAM_QPS_INCONCLUSIVE`**.
Especificamente, MEDIDO a 16M (`docs/benchmarks/m88-billion-scale-verdict.{md,json}`, sign-off council-benchmark):

1. **Vantagem de tamanho CONFIRMADA em escala.** O índice v6/SQ8 é **3.52× menor** que o v5/f32 a 16M
   (8382 MB vs 2382 MB) — confirma o achado M85 (3.5× a 1M) a **16× a escala**. Base mecânica da vantagem
   out-of-RAM: 1/3.5 dos bytes de refine para paginar quando o working set excede o cache.
2. **QPS out-of-RAM DIRECIONAL, não definitivo.** v6 mostra **+21% de cold-QPS a probes=32** (10.2 vs 8.4) — um
   **limite inferior** (a medição cold dá `drop_caches` uma vez por sweep → só a 1ª query é fria; 2–100 aquecem).
   Consistente com a tese (índice menor → menos I/O), mas **não é uma medição limpa de crossover**.
3. **A recall-neutrality do SQ8 NÃO é re-estabelecida aqui.** Ambos medem 0.291 no ponto **degenerado** (clusters
   sintéticos tie-saturados) — artefato de empate, não prova de qualidade do rerank. A recall-neutrality do SQ8
   vem do **M85 (SIFT1M real, ε ≤ 2%)**, não deste run.

## Por que o DoD literal (≥100M/1B) NÃO foi atingido — o teto de memória do build

O DoD do M88 pedia medição a ≥100M/1B. **Não foi alcançado**, e a razão é um achado honesto de escala, não uma
etapa pulada:

- O `theodb_ivfflat` ambuild segura o `AnnIndex` inteiro (~1× base) + uma cópia coletada + os buffers de página
  AQ/refine → **pico ~4× o base em anon-rss**.
- Dois OOM-kills observados a 30M: **47 GB** (python ainda segurando o numpy base) e depois **64 GB** anon-rss
  (base do python liberada → o ambuild sozinho excede os 62 GB usáveis). **16M** (base 8.2 GB, build ~34 GB) foi
  o maior que coube no box de 64 GB. Um **índice genuinamente out-of-RAM** (índice > RAM) não foi construível.

Isto é registrado como **dívida técnica honesta**, não como falha silenciosa (Regra 3).

## Consequências

- A track storage-separation fecha com a vantagem de **memória/tamanho MEDIDA e confirmada em escala** (3.52×,
  posicionamento "classe AlloyDB-in-Postgres" de M85/M87 permanece válido), e a superioridade de **QPS out-of-RAM
  como direcional-não-provada** — mesma disciplina honest-negative de M73/M82.
- **Nenhuma claim** de superioridade de QPS vetorial sobre ScaNN/AlloyDB é feita ou permitida por este ADR — o
  teto de paradigma (~25× vs ivfflat, até ~44× vs hnsw @ 0.99) permanece como medido em M73 (`docs/adr/0035`).
- **Follow-ups recomendados** (para atingir o DoD literal ≥100M), registrados como próxima linhagem, não como
  parte deste fechamento:
  1. **ambuild streaming** — flush incremental das páginas em vez de bufferizar o `AnnIndex` + cópias, para
     derrubar o teto ~4×-base (tornaria 100M+ construível em RAM commodity). Maior alavanca.
  2. **Dados ANN reais bilhão-scale** + **harness cold-cache por-query**, em hardware onde o índice > RAM — o
     setup que transformaria o +21% direcional num número de crossover definitivo.

## Alternativas consideradas (e por que rejeitadas)

- **Insistir a 30M no box de 64 GB** — rejeitado: dois OOM-kills medidos; o build não cabe (anti-sunk-cost).
- **Provisionar um box maior (128 GB+) para forçar 30M+** — rejeitado por ora: o gargalo é o **ambuild ~4×-base**
  (um bug de escala do build, não do query), então a alavanca correta é o build streaming, não mais hardware
  (Esforço ≠ Complexidade — atacar a causa essencial, não comprar RAM para mascarar a ineficiência acidental).
- **Publicar o cold-QPS como vitória out-of-RAM** — rejeitado: a medição cold é um limite inferior e a recall é
  degenerada; seria spin (Regra 5 — performance é claim, não opinião). Daí o token `INCONCLUSIVE`.

## Artefatos

- `docs/benchmarks/m88-billion-scale-verdict.md` / `.json` (run 16M, evidência OOM dmesg, sign-off council-benchmark).
- Phase 1 (build escalável: kmeans-train sampling capado em 1.1M + parallel full-N assignment) — commit `fba16d0`,
  249 pg_tests GREEN, byte-idêntico ≤1M. Foi o que tornou os builds 16M/30M tratáveis.

## Relação com ADRs anteriores

- Estende `0037` (M82 pg_scann-as-AM) e `0035` (M73 teto vetorial) — mesma linhagem de veredito medido honesto.
- Não altera `0002` (mandato LOCKED) nem `0033` (reposicionamento proposto, decisão do owner).
