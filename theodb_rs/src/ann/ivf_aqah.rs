//! M75 (ROADMAP v6, pg_scann Fase 0) — índice IVF-AQ+AH in-memory (spike measurement-first). Compõe (Rule 9) a
//! partição IVF (`IvfflatIndex`), o quantizador anisotrópico (`AqQuantizer`) e o kernel batched AH-LUT já
//! existente (`vec::ah::ah_score_block`, layout transposed block32) num scan de 2 estágios: probe → batched AH
//! scan → rerank full-precision. Domínio puro (sem `pg_sys`, `rules/architecture.md §1`) — testável sem banco.
//! NÃO é o AM pgrx (page format/WAL): isso é M76+. Aqui mede-se o ALGORITMO (a hipótese IVF-AQ+AH do M59/ADR-0019).

use crate::vec::aq::AqQuantizer;
use crate::ann::ivf::IvfflatIndex;
use crate::ann::Metric;
use crate::vec::ah::{ah_score_block, build_lut16};

/// FastScan block width (FAISS bbs=32) — o `ah_score_block` scoreia até 32 códigos por chamada.
const BLOCK: usize = 32;

/// Uma inverted list: códigos AQ no layout TRANSPOSTO block32 (`blocks[b*pairs*32 + p*32 + v]`) + os (id, vetor)
/// originais para o rerank full-precision (stage 2).
struct ListStore {
    blocks: Vec<u8>,        // nblocks * pairs * 32 bytes (padded)
    n: usize,               // nº de códigos reais na lista
    ids: Vec<i64>,          // len n
    vectors: Vec<Vec<f32>>, // len n — original para rerank
}

/// Índice IVF-AQ+AH in-memory (o spike). `pairs = ceil(m/2)` bytes por código.
pub(crate) struct IvfAqahIndex {
    aq: AqQuantizer,
    centroids: Vec<Vec<f32>>,
    lists: Vec<ListStore>,
    pairs: usize,
}

impl IvfAqahIndex {
    /// Constrói: (1) particiona via IVF k-means++; (2) treina o AVQ no corpus inteiro; (3) por lista, encoda cada
    /// vetor e empacota os códigos no layout transposto block32 que `ah_score_block` consome. Métrica L2 (SIFT1M).
    pub(crate) fn build(
        corpus: &[(i64, Vec<f32>)],
        lists: usize,
        m: usize,
        bits: u8,
        aq_threshold: f32,
        seed: u64,
    ) -> Result<Self, String> {
        if corpus.is_empty() {
            return Err("ivf_aqah build: empty corpus".to_string());
        }
        let dim = corpus[0].1.len();
        if dim == 0 || dim % m != 0 {
            return Err(format!("ivf_aqah build: dim {dim} not divisible by m {m} (fail-fast, Rule 8)"));
        }
        let ivf = IvfflatIndex::build(corpus, lists, Metric::L2, seed);
        let centroids: Vec<Vec<f32>> = ivf.centroids().to_vec();
        let all: Vec<Vec<f32>> = corpus.iter().map(|(_, v)| v.clone()).collect();
        let aq = AqQuantizer::train(&all, m, bits, aq_threshold, seed)?;
        let pairs = m.div_ceil(2);

        let entries = ivf.list_entries();
        let mut stores = Vec::with_capacity(entries.len());
        for list in &entries {
            let n = list.len();
            let nblocks = n.div_ceil(BLOCK);
            let mut blocks = vec![0u8; nblocks * pairs * BLOCK];
            let mut ids = Vec::with_capacity(n);
            let mut vectors = Vec::with_capacity(n);
            for (i, (id, v)) in list.iter().enumerate() {
                let code = aq.encode(v); // pairs bytes
                let base = (i / BLOCK) * pairs * BLOCK;
                let v_in_block = i % BLOCK;
                for p in 0..pairs {
                    blocks[base + p * BLOCK + v_in_block] = code[p];
                }
                ids.push(*id);
                vectors.push(v.clone());
            }
            stores.push(ListStore { blocks, n, ids, vectors });
        }
        Ok(IvfAqahIndex { aq, centroids, lists: stores, pairs })
    }

    /// Busca 2-estágios: probe os `nprobe` centroides mais próximos → batched AH scan das listas (lower-bound
    /// approx via LUT) → rerank exato (L2 full-precision) dos `rerank_n` melhores → top-k ids. `nprobe`/`rerank_n`
    /// são o trade-off recall×QPS.
    pub(crate) fn search(&self, q: &[f32], k: usize, nprobe: usize, rerank_n: usize) -> Result<Vec<i64>, String> {
        let lut = build_lut16(q, &self.aq)?;
        // Stage 0 — probe: nprobe centroides mais próximos (exato, barato: nlists distâncias).
        let mut cdist: Vec<(f64, usize)> = self
            .centroids
            .iter()
            .enumerate()
            .map(|(ci, c)| (crate::vec::l2_distance(q, c), ci))
            .collect();
        cdist.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let probe = nprobe.min(cdist.len());

        // Stage 1 — batched AH scan: (score_approx, list_idx, code_idx).
        let mut cands: Vec<(i32, usize, usize)> = Vec::new();
        let mut out = [0i32; BLOCK];
        for &(_, ci) in cdist.iter().take(probe) {
            let store = &self.lists[ci];
            let nblocks = store.n.div_ceil(BLOCK);
            for b in 0..nblocks {
                let cnt = (store.n - b * BLOCK).min(BLOCK);
                let base = b * self.pairs * BLOCK;
                let block = &store.blocks[base..base + self.pairs * BLOCK];
                ah_score_block(&lut, block, cnt, &mut out[..cnt]);
                for (j, &score) in out.iter().enumerate().take(cnt) {
                    cands.push((score, ci, b * BLOCK + j));
                }
            }
        }

        // Stage 2 — rerank exato dos rerank_n melhores por score AH (menor = mais perto).
        let rn = rerank_n.min(cands.len());
        if rn < cands.len() {
            cands.select_nth_unstable_by_key(rn, |c| c.0);
            cands.truncate(rn);
        }
        let mut reranked: Vec<(f64, i64)> = cands
            .iter()
            .map(|&(_, ci, idx)| {
                let store = &self.lists[ci];
                (crate::vec::l2_distance(q, &store.vectors[idx]), store.ids[idx])
            })
            .collect();
        reranked.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        Ok(reranked.into_iter().take(k).map(|(_, id)| id).collect())
    }
}

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use super::*;
    use pgrx::prelude::*;

    /// SplitMix64 determinístico (mesma forma dos testes de `vec::ah`).
    struct TestRng(u64);
    impl TestRng {
        fn new(seed: u64) -> Self {
            TestRng(seed)
        }
        fn next_f32(&mut self) -> f32 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^= z >> 31;
            ((z >> 11) as f64 / (1u64 << 53) as f64) as f32 * 2.0 - 1.0
        }
    }

    fn corpus(rng: &mut TestRng, n: usize, dim: usize) -> Vec<(i64, Vec<f32>)> {
        (0..n).map(|i| (i as i64, (0..dim).map(|_| rng.next_f32()).collect())).collect()
    }

    /// GT exato: top-k por L2 sobre o corpus inteiro.
    fn exact_topk(corpus: &[(i64, Vec<f32>)], q: &[f32], k: usize) -> Vec<i64> {
        let mut d: Vec<(f64, i64)> =
            corpus.iter().map(|(id, v)| (crate::vec::l2_distance(q, v), *id)).collect();
        d.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        d.into_iter().take(k).map(|(_, id)| id).collect()
    }

    fn recall_at(got: &[i64], truth: &[i64]) -> f64 {
        let t: std::collections::HashSet<i64> = truth.iter().copied().collect();
        got.iter().filter(|id| t.contains(id)).count() as f64 / truth.len() as f64
    }

    /// T2.3 — o pipeline IVF-AQ+AH é CORRETO: com nprobe=todas + rerank, recall@10 alto vs GT exato.
    #[pg_test]
    fn ivf_aqah_search_recall_vs_exact() {
        let mut rng = TestRng::new(2026);
        let (n, dim, lists, k) = (2000, 32, 32, 10);
        let corpus = corpus(&mut rng, n, dim);
        let idx = IvfAqahIndex::build(&corpus, lists, 8, 4, 2.0, 42).expect("build");
        let mut sum = 0.0;
        let nq = 20;
        for _ in 0..nq {
            let q: Vec<f32> = (0..dim).map(|_| rng.next_f32()).collect();
            let got = idx.search(&q, k, lists, 200).expect("search"); // nprobe=all + rerank
            sum += recall_at(&got, &exact_topk(&corpus, &q, k));
        }
        let recall = sum / nq as f64;
        assert!(recall >= 0.90, "IVF-AQ+AH recall@10 {recall:.3} < 0.90 (pipeline incorreto)");
    }

    /// T2.3 — recall monotônico (não-decrescente) em nprobe (edge: nprobe=1 << nprobe=all).
    #[pg_test]
    fn ivf_aqah_recall_grows_with_nprobe() {
        let mut rng = TestRng::new(7);
        let (n, dim, lists, k) = (2000, 32, 16, 10);
        let corpus = corpus(&mut rng, n, dim);
        let idx = IvfAqahIndex::build(&corpus, lists, 8, 4, 2.0, 9).expect("build");
        let q: Vec<f32> = (0..dim).map(|_| rng.next_f32()).collect();
        let truth = exact_topk(&corpus, &q, k);
        let r1 = recall_at(&idx.search(&q, k, 1, 200).expect("s1"), &truth);
        let rall = recall_at(&idx.search(&q, k, lists, 200).expect("sall"), &truth);
        assert!(rall >= r1, "recall@nprobe=all {rall:.3} < recall@nprobe=1 {r1:.3} (não-monotônico)");
    }

    /// Negative (Rule 8): dim não divisível por m → typed Err, não panic.
    #[pg_test]
    fn ivf_aqah_build_rejects_bad_dim() {
        let mut rng = TestRng::new(1);
        let corpus = corpus(&mut rng, 50, 7); // dim=7 não divisível por m=4
        assert!(IvfAqahIndex::build(&corpus, 4, 4, 4, 2.0, 1).is_err(), "dim%m!=0 deve retornar Err");
    }

    // ---------- M75 T3.1/T3.2 — medição SIFT1M real (gated por env `M75_SIFT_DIR`) ----------

    /// Lê um arquivo `.fvecs` (TEXMEX): sequência de `[i32 d][d × f32]`. Retorna as `limit` primeiras (ou todas).
    fn read_fvecs(path: &str, limit: usize) -> Result<Vec<Vec<f32>>, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
        let mut out = Vec::new();
        let mut off = 0usize;
        while off + 4 <= bytes.len() && out.len() < limit {
            let d = i32::from_le_bytes(bytes[off..off + 4].try_into().unwrap()) as usize;
            off += 4;
            if off + d * 4 > bytes.len() {
                return Err(format!("truncated fvecs at vec {} (Rule 8)", out.len()));
            }
            let v: Vec<f32> = (0..d)
                .map(|i| f32::from_le_bytes(bytes[off + i * 4..off + i * 4 + 4].try_into().unwrap()))
                .collect();
            off += d * 4;
            out.push(v);
        }
        Ok(out)
    }


    fn now_s() -> f64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64()
    }

    /// Medição real: IVF-AQ+AH vs full-precision IVF em SIFT1M, sweep de nprobe, recall@10 + QPS.
    /// Roda SÓ quando `M75_SIFT_DIR` aponta um dir com `sift_base.fvecs`/`sift_query.fvecs`/`sift_groundtruth.ivecs`.
    /// Emite linhas `M75_RESULT ...` no log (parseadas pelo harness do droplet). NÃO fabrica número.
    #[pg_test]
    fn m75_sift1m_measure() {
        let dir = match std::env::var("M75_SIFT_DIR") {
            Ok(d) => d,
            Err(_) => {
                pgrx::log!("M75_SKIP: M75_SIFT_DIR unset — medição SIFT1M pulada (sem dataset)");
                return;
            }
        };
        let m: usize = std::env::var("M75_M").ok().and_then(|s| s.parse().ok()).unwrap_or(32);
        let lists: usize = std::env::var("M75_LISTS").ok().and_then(|s| s.parse().ok()).unwrap_or(1000);
        let nq: usize = std::env::var("M75_NQ").ok().and_then(|s| s.parse().ok()).unwrap_or(1000);
        let out_path = std::env::var("M75_OUT").unwrap_or_else(|_| "/home/theo/m75_out.txt".to_string());
        let emit = |line: String| {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&out_path) {
                let _ = writeln!(f, "{line}");
            }
            pgrx::log!("{}", line);
        };
        let base_n: usize = std::env::var("M75_N").ok().and_then(|s| s.parse().ok()).unwrap_or(1_000_000);
        let base = read_fvecs(&format!("{dir}/sift_base.fvecs"), base_n).expect("base");
        let queries = read_fvecs(&format!("{dir}/sift_query.fvecs"), nq).expect("query");
        // GT EXATO sobre o base CARREGADO (brute-force top-10) — válido em qualquer escala (o `sift_groundtruth.ivecs`
        // é p/ o 1M completo; com subsample daria recall inválido, Regra 5).
        let gt: Vec<Vec<i64>> = queries
            .iter()
            .map(|q| {
                let mut d: Vec<(f64, i64)> =
                    base.iter().enumerate().map(|(i, v)| (crate::vec::l2_distance(q, v), i as i64)).collect();
                let nth = 9.min(d.len().saturating_sub(1));
                d.select_nth_unstable_by(nth, |a, b| a.0.partial_cmp(&b.0).unwrap());
                d.truncate(10);
                d.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                d.into_iter().map(|(_, id)| id).collect()
            })
            .collect();
        emit(format!("M75_LOADED base={} query={} gt={} m={m} lists={lists}", base.len(), queries.len(), gt.len()));

        let corpus: Vec<(i64, Vec<f32>)> = base.iter().enumerate().map(|(i, v)| (i as i64, v.clone())).collect();
        let t0 = now_s();
        let idx = IvfAqahIndex::build(&corpus, lists, m, 4, 2.0, 42).expect("build aqah");
        let build_aqah = now_s() - t0;
        let t1 = now_s();
        let f32idx = IvfflatIndex::build(&corpus, lists, Metric::L2, 42);
        let build_f32 = now_s() - t1;
        emit(format!("M75_BUILD aqah_s={build_aqah:.1} f32_s={build_f32:.1}"));

        let k = 10;
        let recall10 = |got: &[i64], truth: &[i64]| -> f64 {
            let t: std::collections::HashSet<i64> = truth.iter().take(k).copied().collect();
            got.iter().take(k).filter(|id| t.contains(id)).count() as f64 / k as f64
        };
        // sweep nprobe; ≥3 runs de timing por ponto (mean).
        for &nprobe in &[4usize, 8, 16, 32, 64, 128, 256] {
            // IVF-AQ+AH
            let rerank = (nprobe * 20).max(100);
            let mut rec = 0.0;
            let mut best = f64::INFINITY;
            for _run in 0..3 {
                let ts = now_s();
                let mut r = 0.0;
                for (qi, q) in queries.iter().enumerate() {
                    let got = idx.search(q, k, nprobe, rerank).expect("search aqah");
                    r += recall10(&got, &gt[qi]);
                }
                let el = now_s() - ts;
                best = best.min(el);
                rec = r / queries.len() as f64;
            }
            let qps = queries.len() as f64 / best;
            emit(format!("M75_RESULT engine=ivfaqah nprobe={nprobe} rerank={rerank} recall={rec:.4} qps={qps:.1}"));

            // full-precision IVF baseline
            let mut recf = 0.0;
            let mut bestf = f64::INFINITY;
            for _run in 0..3 {
                let ts = now_s();
                let mut r = 0.0;
                for (qi, q) in queries.iter().enumerate() {
                    let got: Vec<i64> = f32idx.search(q, k, nprobe).into_iter().map(|(id, _)| id).collect();
                    r += recall10(&got, &gt[qi]);
                }
                let el = now_s() - ts;
                bestf = bestf.min(el);
                recf = r / queries.len() as f64;
            }
            let qpsf = queries.len() as f64 / bestf;
            emit(format!("M75_RESULT engine=f32ivf nprobe={nprobe} recall={recf:.4} qps={qpsf:.1}"));
        }
        emit("M75_DONE".to_string());
    }
}
