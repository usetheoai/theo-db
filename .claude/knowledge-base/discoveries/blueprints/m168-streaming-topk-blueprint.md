# Blueprint — decode O(k) para o top-k de projeção colunar (fecha o DoD 2b do M167)

**Data:** 2026-07-29 · **Origem:** item 2b do DoD do M167 ficou PARCIAL (issue #215)

## O problema, com precisão

`try_swap_topk` → `run_columnar_topk` → `decode_to_batch` decodifica **todas** as linhas da relação
(`{projeção ∪ chaves ∪ colunas de filtro}`) num único `RecordBatch` Arrow, e só então o DataFusion filtra,
ordena e limita. O heap do LIMIT-k é O(k); o decode que o alimenta é O(N). O M167 mitigou com um teto de
tamanho (ADR-4) — teto contra catástrofe, não a propriedade.

## O que o acervo diz (R0.1 — local antes da web)

| Achado | Fonte no acervo | Consequência |
|---|---|---|
| `TopK` do DataFusion tem `insert_batch(&mut self, batch)` + heap limitado + `maybe_compact()` | `references/datafusion/datafusion/physical-plan/src/topk/mod.rs:113,379,449` | A estrutura que eu precisaria escrever já existe |
| **Mas `mod topk;` é PRIVADO** | `references/datafusion/datafusion/physical-plan/src/lib.rs:64` | Não dá para usar direto — a rung 4 da parsimony ladder falha nesse nível |
| `SortExec` com `Some(fetch)` usa TopK internamente e se exibe como `SortExec: TopK(fetch=k)` | `references/datafusion/datafusion/physical-plan/src/sorts/sort.rs:1200-1203` | O caminho público é alimentar `sort().limit(0,k)` com um stream preguiçoso |
| `StreamingTable` é público | `references/datafusion/datafusion/catalog/src/streaming.rs:36` + `catalog/src/lib.rs:42` | É a API para fonte lazy |
| `PartitionStream` é público e exige `Debug + Send + Sync` | `references/datafusion/datafusion/physical-plan/src/streaming.rs:49` | **O risco central** — ver abaixo |

## O que já está do nosso lado

`decode_columns_v2` **já** itera stripe a stripe e chunk-group a chunk-group
(`am/columnar.rs`, `for pl in &plans { for cg in 0..pl.n_chunk_groups {`) — ele acumula em vetores em vez de
transmitir. O laço existe; falta o `yield`.

E o tamanho do batch decodificado **já é computado** em `df_executor.rs:585`
(`batch.get_array_memory_size()`), hoje só usado para dimensionar o pool. Uma linha de trace transforma o
"pico não medido" do M167 § 6 num número — e é o baseline sem o qual nenhuma melhora é demonstrável.

## O risco central: `Send + Sync` sobre ponteiro de relação

`PartitionStream: Debug + Send + Sync`. Um stream que decodifica sob demanda carrega `pg_sys::Relation` —
ponteiro cru, e acesso a relação do PostgreSQL **não é thread-safe**.

O runtime é `tokio::runtime::Builder::new_current_thread()` com `block_on` (`df_executor.rs:573`), e
`with_target_partitions(1)`: os streams são pollados na thread do backend, nunca em outra. Um
`unsafe impl Send + Sync` seria **verdadeiro sob esse runtime** — mas é invariante load-bearing: se alguém
trocar para `new_multi_thread` ou subir `target_partitions`, vira corrupção silenciosa de memória, não erro
de compilação.

Precedente do projeto na mesma classe: o M139 descobriu que "Tantivy chama `Directory` de 4 threads → SPI só
na main". A lição é a mesma e o desfecho lá foi buffer-then-flush.

## ADR-1 — spike antes de implementação (Fase 4 do theodb-evolution)

**Decisão:** a viabilidade do `unsafe impl Send + Sync` é a maior incerteza e decide o design inteiro. Ela
vira spike falsificável com veredito (viável / viável-com-restrições / não-viável), não premissa.

**Alternativas consideradas:**
1. *Heap próprio sobre chunk-groups decodificados* — evita `PartitionStream` e o `Send`. Custo: reimplementar
   comparação multi-chave com semântica de NULL/collation idêntica à do DataFusion, que é exatamente onde o
   M167 achou dois furos de correção. Rejeitada por Regra 9 e por risco.
2. *Duas passagens (decodificar só a chave → achar o corte → decodificar o payload dos k)* — reduz muito para
   `SELECT *` largo (105 colunas → 1), mas continua O(N) na coluna-chave. Não cumpre o DoD; é fallback se o
   spike der não-viável.
3. *`StreamingTable` + `SortExec: TopK`* — usa o operador do próprio DataFusion, O(k) de verdade. Escolhida,
   **condicionada ao spike**.

## Como a correção será provada

O M167 deixou a rede de segurança pronta, e ela é a razão de este trabalho ser tratável:
`m167_hits_topk_ab.sql` (1M × 105 colunas, gate H0 de roteamento + gate final de 15 asserções),
`m158_ec_harness.sql` (20 asserções), `columnar_type_ab.py` (35/35) e quatro controles positivos. Um top-k
errado não passa por eles — foi para isso que foram construídos.

**Além disso, e obrigatório:** o número que define sucesso é o **pico de memória decodificada**, medido
antes e depois pelo mesmo instrumento. Sem esse par, "agora é O(k)" é prosa.

## Limites honestos deste blueprint

- Não sei ainda se `StreamingTable` aceita um stream não-`'static` ou se será preciso `Arc` + interior mutability; isso é parte do spike.
- Não medi o custo por batch de atravessar o DataFusion N vezes em vez de 1. Um decode O(k) que fique 3× mais lento troca um problema por outro, e o benchmark pareado do M167 é quem decide.
- O `with_target_partitions(1)` é hoje o que torna o `unsafe impl` verdadeiro. Se o spike confirmar o design, essa dependência precisa virar asserção em runtime, não comentário.
