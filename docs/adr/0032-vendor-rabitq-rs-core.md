# ADR-0032 — Vendorizar o core do `rabitq-rs` (Apache-2.0) para o índice quantizado IVF-RaBitQ

- **Status:** Accepted (2026-07-10)
- **Contexto estratégico:** ataque ao Gap 2 do pilar vetorial P0 (theodb → superioridade de QPS vs ScaNN/AlloyDB), Roadmap v5.
- **Decisão do owner:** copiar (vendor) para dentro do projeto em vez de depender do crate, para poder evoluir e por segurança de supply-chain.

## Contexto

O head-to-head M33 mediu ~25× de gap de QPS vs ScaNN (algoritmo do AlloyDB), cuja vantagem é de **paradigma**: IVF +
quantização + Asymmetric Hashing (FastScan LUT SIMD), não grafo full-precision. M57 (SBQ) e M59 (anisotrópico+AH)
tentaram quantização no **carrier HNSW** e foram **refutados por medição**. O deep research (R0) mostrou que:
1. O SOTA permissivo hoje é **RaBitQ** (Gao & Long, arXiv:2405.12497) — quantização 1-bit, **sem treino de
   codebook**, com **bound de erro provado** (pode dispensar rerank a recall alto). Adotado por Milvus/Faiss/
   Elasticsearch via a `VectorDB-NTU/RaBitQ-Library` (Apache-2.0, C++, canônica).
2. Existe uma implementação **pura-Rust Apache-2.0**: `lqhl/rabitq-rs` (IVF+RaBitQ+FHT+FastScan+SIMD).
3. O TheoDB **já tem** a metade cara: IVFFlat próprio (`ann/ivf.rs`, k-means++, inverted lists) + o AM
   `theodb_ivfflat` + storage page-native + WAL. O carrier certo (IVF) já é nosso.

## Decisão

1. **Vendorizar (copiar para dentro) apenas o CORE do algoritmo** do `rabitq-rs` (commit `10b9a4e`, Apache-2.0):
   `quantizer.rs`, `rotation.rs`, `fastscan.rs`, `fastscan_kernel.rs`, `simd.rs`, `math.rs` → `theodb_rs/src/rabitq/vendor/`.
   **NÃO** vendorizar a camada de storage/índice do upstream (file/mmap-based `ivf.rs`, `mstg/*`, `io.rs`,
   `python_bindings.rs`) — ela é substituída pela nossa infra de AM page-native.
2. **Wiring:** o core quantiza os vetores; a **nossa** IVF (partição + páginas + WAL) armazena os códigos; o scan
   de cada inverted list usa a **FastScan** vendorizada sobre os códigos comprimidos = **IVF-RaBitQ**.
3. **Vendoring, não dependência**, por: (a) o storage do upstream é incompatível com um AM do Postgres — teremos
   que integrar o core de qualquer jeito; (b) controle/evolução (adaptar ao nosso tipo `vector` e páginas sem
   esperar upstream); (c) supply-chain (rabitq-rs é v0.9.0, repo individual, bug em ARM64 — congelamos um
   commit auditado, x86_64); (d) Apache-2.0 → Apache-2.0 permite copiar-com-atribuição.

## Alternativas rejeitadas

- **Dependência do crate `rabitq-rs`:** não podemos usar a camada de índice dele (file/mmap ≠ páginas PG);
  acoplar a stack a um crate v0.9.0 individual com bug ARM64 é risco. Vendoring do core resolve.
- **Reimplementar RaBitQ do zero:** viola a Regra 9 (não reinventar) — o algoritmo é sutil (bound de erro,
  rotação FHT, packing de códigos); há impl permissiva pronta + a canônica NTU como oráculo.
- **Adotar o AQ anisotrópico do ScaNN:** possível patente + treino de codebook complexo; RaBitQ é permissivo,
  training-free, com bound.
- **Quantizar de novo sobre HNSW:** refutado (M57/M59). O carrier tem que ser IVF (FastScan batch-scan).

## Consequências

- **Positivas:** de-risca o Gap 2 de "aposta de meses do zero" para "integrar um core permissivo no nosso IVF";
  o lever é o **não-refutado** que o M74 exige; controle total para evoluir.
- **Custos/obrigações (D1):** preservar `LICENSE` + `VENDORED.md` (atribuição + provenance) no diretório
  vendorizado; toda modificação rastreada em git; `loop-check-licence` deve passar. Manutenção do core é nossa
  (aceitável — íamos modificar de qualquer forma).
- **Gate D3 antes do AM completo:** um spike local (recall/velocidade do IVF-RaBitQ) deve mostrar caminho viável
  de fechar fração significativa do gap antes do investimento de integração completa. Honest-negative aceito.

## Cross-references

- Referências (Apache-2.0, `knowledge-base/references/`): `rabitq-rs`, `RaBitQ-Library`.
- Root blocker: `docs/benchmarks/p0-vector-superiority-root-blocker.md`
- Licença/North Star: `docs/adr/0006` (D1), `docs/adr/0002` (North Star)
- Nossa IVF: `theodb_rs/src/ann/ivf.rs`, AM `theodb_ivfflat`
- Refutados: `docs/adr/0018` (M57 SBQ), M59 (anisotrópico+AH)
