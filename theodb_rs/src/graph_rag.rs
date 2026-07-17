//! M111 — vector-on-nodes + the vector-entry→bounded-traversal→rerank GraphRAG flow (Phase 4, ADR-0048).
//!
//! The SOTA GraphRAG retrieval path (HippoRAG / LazyGraphRAG) as ONE in-DB, zero-copy operation:
//!   (1) cosine `<=>` top-k ENTRY entities over `graph_nodes.embedding` (reuse the own `public.vector` + AM),
//!   (2) bounded `graph_expand` from the entry node-ids (M108/M109 traversal),
//!   (3) collect `source_chunk_ids` from reached edges, rank by summed edge weight.
//! `theodb.graph_rag_search` takes a PRE-COMPUTED query embedding (ADR-1) → hermetically testable (no embed
//! call in structural tests) and composes with `ai.embed(query)` at the call site. `theodb.graph_embed_nodes`
//! fills the node embeddings via `ai.embed` (reused, Rule 9). The stratified real-embedding recall@k eval is the
//! gate (ADR-3): graph×vector ≥ pure-vector on multi-hop; honest-negative on local-fact is a VALID outcome.
use pgrx::prelude::*;

// M111 — add the node-embedding column to the M110 catalog (nullable; filled by graph_embed_nodes).
extension_sql!(
    r#"
ALTER TABLE theodb.graph_nodes ADD COLUMN IF NOT EXISTS embedding public.vector;
"#,
    name = "theodb_graph_node_embedding",
    requires = ["theodb_graph_extract_schema", "vector_type"],
);

#[pgrx::pg_schema]
mod theodb_rs {
    use pgrx::prelude::*;

    /// `theodb._graph_embed_nodes(ws, coll, model) -> bigint` — embed each not-yet-embedded node's `name` via
    /// `ai.embed` (batched, one round-trip) and store it in `graph_nodes.embedding`. Returns the count embedded.
    #[pg_extern]
    fn _graph_embed_nodes(workspace_id: &str, collection_id: &str, model: Option<&str>) -> i64 {
        let mut ids: Vec<i64> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        Spi::connect(|c| {
            let t = c
                .select(
                    "SELECT id, name FROM theodb.graph_nodes WHERE workspace_id=$1 AND collection_id=$2 AND embedding IS NULL ORDER BY id",
                    None,
                    &[workspace_id.into(), collection_id.into()],
                )
                .unwrap();
            for row in t {
                if let (Some(id), Some(name)) = (row.get::<i64>(1).ok().flatten(), row.get::<String>(2).ok().flatten()) {
                    ids.push(id);
                    names.push(name);
                }
            }
        });
        if ids.is_empty() {
            return 0;
        }
        let refs: Vec<Option<&str>> = names.iter().map(|s| Some(s.as_str())).collect();
        let vectors = crate::embed::run_batch(&refs, model); // vector literals, N-in/N-out
        for (id, v) in ids.iter().zip(vectors.iter()) {
            Spi::run_with_args(
                "UPDATE theodb.graph_nodes SET embedding = ($1)::vector WHERE id = $2",
                &[v.as_str().into(), (*id).into()],
            )
            .unwrap_or_else(|e| crate::pg::err_input(&format!("theodb.graph_embed_nodes: update failed: {e:?}")));
        }
        ids.len() as i64
    }
}

// Public surface: the composed GraphRAG flow (SQL — it orchestrates the vector `<=>` entry, the `graph_expand`
// traversal, and the chunk ranking; KISS, no Rust needed). Assumes `graph_build('theodb.graph_edges',…)` ran.
extension_sql!(
    r#"
CREATE FUNCTION theodb.graph_embed_nodes(workspace_id text, collection_id text, model text DEFAULT NULL)
  RETURNS bigint LANGUAGE sql VOLATILE AS $fn$ SELECT theodb_rs._graph_embed_nodes($1,$2,$3) $fn$;

CREATE FUNCTION theodb.graph_rag_search(query_embedding public.vector, workspace_id text, collection_id text,
                                        k_entry int DEFAULT 5, max_hops int DEFAULT 2)
  RETURNS TABLE(chunk_id text, score float8) LANGUAGE sql STABLE AS $fn$
  WITH entry AS (
    SELECT id FROM theodb.graph_nodes
    WHERE workspace_id = $2 AND collection_id = $3 AND embedding IS NOT NULL
    ORDER BY embedding <=> $1
    LIMIT $4
  ),
  reached AS (
    SELECT t.node_id FROM theodb.graph_expand('theodb.graph_edges',
      (SELECT array_agg(id) FROM entry), $5) AS t(node_id)
  ),
  chunks AS (
    SELECT unnest(e.source_chunk_ids) AS chunk_id, e.weight
    FROM theodb.graph_edges e
    WHERE e.workspace_id = $2 AND e.collection_id = $3
      AND (e.src_id IN (SELECT node_id FROM reached) OR e.dst_id IN (SELECT node_id FROM reached))
  )
  SELECT chunk_id, sum(weight)::float8 AS score
  FROM chunks GROUP BY chunk_id ORDER BY score DESC, chunk_id
  $fn$;

REVOKE ALL ON FUNCTION theodb.graph_embed_nodes(text,text,text) FROM PUBLIC;
REVOKE ALL ON FUNCTION theodb.graph_rag_search(public.vector,text,text,int,int) FROM PUBLIC;
"#,
    name = "theodb_graph_rag_wrappers",
    requires = [_graph_embed_nodes, "theodb_graph_node_embedding", "theodb_graph_extract_schema", "vector_type"],
);

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    // Insert a deterministic hand-crafted graph (4-dim node embeddings, a chain) into ws='ws',coll='c' and build
    // the CSR. A: [1,0,0,0], B:[0,1,0,0], C:[0,0,1,0], D:[0,0,0,1], E:[1,1,0,0] (isolated). Chain edges
    // A-B('kab'), B-C('kbc'), C-D('kcd'). Returns nothing; nodes/edges are queried by normalized_name.
    fn seed_hermetic() {
        for (name, emb) in [("A", "[1,0,0,0]"), ("B", "[0,1,0,0]"), ("C", "[0,0,1,0]"), ("D", "[0,0,0,1]"), ("E", "[1,1,0,0]")] {
            Spi::run(&format!(
                "INSERT INTO theodb.graph_nodes (workspace_id,collection_id,name,normalized_name,embedding) \
                 VALUES ('ws','c','{name}','{}','{emb}'::vector)",
                name.to_lowercase()
            )).unwrap();
        }
        let id = |n: &str| -> i64 {
            Spi::get_one(&format!("SELECT id FROM theodb.graph_nodes WHERE workspace_id='ws' AND normalized_name='{n}'")).unwrap().unwrap()
        };
        for (s, d, chunk) in [("a", "b", "kab"), ("b", "c", "kbc"), ("c", "d", "kcd")] {
            let (si, di) = (id(s), id(d));
            let (lo, hi) = if si <= di { (si, di) } else { (di, si) };
            Spi::run(&format!(
                "INSERT INTO theodb.graph_edges (workspace_id,collection_id,src_id,dst_id,weight,source_chunk_ids) \
                 VALUES ('ws','c',{lo},{hi},1,ARRAY['{chunk}']::text[])"
            )).unwrap();
        }
        Spi::get_one::<i64>("SELECT theodb.graph_build('theodb.graph_edges','src_id','dst_id')").unwrap();
    }

    // M111 (T1.1): the flow — query closest to A, k_entry=1, max_hops=2 → entry {A}, reached {A,B,C}, chunks from
    // edges touching {A,B,C} = {kab, kbc, kcd}. Set comparison (order-independent).
    #[pg_test]
    fn m111_flow_structural_set() {
        seed_hermetic();
        let chunks: Vec<String> = Spi::connect(|c| {
            c.select(
                "SELECT chunk_id FROM theodb.graph_rag_search('[0.9,0.1,0,0]'::vector,'ws','c',1,2) ORDER BY chunk_id",
                None, &[],
            ).unwrap().map(|r| r.get::<String>(1).unwrap().unwrap()).collect()
        });
        assert_eq!(chunks, vec!["kab", "kbc", "kcd"], "entry A + ≤2-hop reaches all three chain chunks");
    }

    // M111 (T1.1): the traversal ADDS recall — the gold chunk 'kcd' belongs to a 2-hop neighbor (C-D), NOT the
    // entry entity A's own chunk. A pure vector-over-ENTITIES search anchored at A would never surface 'kcd';
    // the graph flow does. This is the core value proposition.
    #[pg_test]
    fn m111_flow_multihop_adds_recall() {
        seed_hermetic();
        let has_kcd: bool = Spi::get_one(
            "SELECT EXISTS(SELECT 1 FROM theodb.graph_rag_search('[0.9,0.1,0,0]'::vector,'ws','c',1,3) WHERE chunk_id='kcd')"
        ).unwrap().unwrap();
        assert!(has_kcd, "graph traversal surfaces a distant neighbor's chunk that entry-only vector would miss");
    }

    // M111 (T1.1): max_hops bound. max_hops=0 → entry {A} only → chunks from edges touching A = {kab}.
    #[pg_test]
    fn m111_flow_hops_bound() {
        seed_hermetic();
        let chunks: Vec<String> = Spi::connect(|c| {
            c.select(
                "SELECT chunk_id FROM theodb.graph_rag_search('[0.9,0.1,0,0]'::vector,'ws','c',1,0) ORDER BY chunk_id",
                None, &[],
            ).unwrap().map(|r| r.get::<String>(1).unwrap().unwrap()).collect()
        });
        assert_eq!(chunks, vec!["kab"], "max_hops=0 returns only the entry entity's own edge chunks");
    }

    // M111 (T1.1): empty / wrong workspace → no rows, no panic (tenant-scoped).
    #[pg_test]
    fn m111_flow_empty_and_isolation() {
        seed_hermetic();
        let other: i64 = Spi::get_one("SELECT count(*) FROM theodb.graph_rag_search('[0.9,0.1,0,0]'::vector,'other','c',1,2)").unwrap().unwrap();
        assert_eq!(other, 0, "a workspace with no nodes returns nothing");
    }

    // M111 (T1.1): graph_embed_nodes fills embeddings via ai.embed (deterministic 'parity'-free path is the real
    // embed — SKIP if no endpoint). Here we only assert the count contract on an already-embedded graph = 0 to
    // re-embed (idempotent: embedding IS NULL filter). Full real-embedding path is exercised by the eval below.
    #[pg_test]
    fn m111_embed_nodes_skips_already_embedded() {
        seed_hermetic(); // nodes already have embeddings
        let n: i64 = Spi::get_one("SELECT theodb.graph_embed_nodes('ws','c')").unwrap().unwrap();
        assert_eq!(n, 0, "nodes with a non-NULL embedding are not re-embedded (idempotent)");
    }

    // M111 (T2.1) — the GATE: recall@k on the REAL HotpotQA distractor benchmark (HippoRAG's multi-hop set,
    // HuggingFace hotpotqa/hotpot_qa). Per question: its 10 context paragraphs are the corpus, the 2
    // supporting-fact paragraphs are gold; measure recall@k of (a) pure-vector-over-paragraphs vs (b) the
    // graph×vector flow, with REAL OpenAI embeddings. Reads THEODB_EVAL_HOTPOT_PATH. SKIPs (WARN) unless the key
    // + dataset are present. Reported HONESTLY — graph may win or lose on a real benchmark (no fabricated number).
    #[pg_test]
    fn m111_eval_hotpot() {
        let key = match std::env::var("THEODB_EVAL_OPENAI_KEY") {
            Ok(k) if !k.is_empty() => k,
            _ => {
                pgrx::warning!("M111_EVAL SKIP: THEODB_EVAL_OPENAI_KEY not set (no paid embed calls in the normal suite)");
                return;
            }
        };
        let path = std::env::var("THEODB_EVAL_HOTPOT_PATH").unwrap_or_else(|_| "/tmp/hotpot_eval.json".to_string());
        let data = match std::fs::read_to_string(&path) {
            Ok(d) => d,
            Err(_) => {
                pgrx::warning!("M111_EVAL SKIP: HotpotQA dataset not found at {path}");
                return;
            }
        };
        Spi::run("SET theodb.embedding_endpoint = 'https://api.openai.com/v1/embeddings'").unwrap();
        Spi::run("SET theodb.embedding_model = 'text-embedding-3-small'").unwrap();
        Spi::run(&format!("SET theodb.embedding_api_key = '{}'", key.replace('\'', "''"))).unwrap();
        Spi::run("CREATE TABLE IF NOT EXISTS theodb.chunks_eval(id text, ws text, body text, emb public.vector)").unwrap();

        let recs: serde_json::Value = serde_json::from_str(&data).expect("hotpot eval json");
        let arr = recs.as_array().expect("array");
        let k = 4i64; // retrieve top-4 (gold = 2 supporting paragraphs)
        let n_q = arr.len().min(30);
        let (mut vsum, mut gsum, mut hsum, mut n) = (0.0f64, 0.0f64, 0.0f64, 0i64);
        for (qi, rec) in arr.iter().take(n_q).enumerate() {
            let ws = format!("hp{qi}");
            Spi::run("TRUNCATE theodb.graph_nodes, theodb.graph_edges").unwrap();
            Spi::run("DELETE FROM theodb.chunks_eval").unwrap();
            let q = rec["q"].as_str().unwrap_or("");
            let gold: Vec<String> = rec["gold"].as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
            let paras = match rec["paras"].as_array() { Some(p) => p, None => continue };
            if gold.is_empty() || paras.is_empty() { continue; }
            for p in paras {
                let title = p[0].as_str().unwrap_or("");
                let text = p[1].as_str().unwrap_or("");
                Spi::run_with_args("INSERT INTO theodb.chunks_eval(id,ws,body) VALUES ($1,$2,$3)", &[title.into(), ws.as_str().into(), text.into()]).unwrap();
                let full = format!("{title}. {text}"); // prepend the paragraph's subject entity for the heuristic
                Spi::get_one_with_args::<i64>("SELECT theodb.graph_upsert($1,'c',$2,$3)", &[ws.as_str().into(), title.into(), full.as_str().into()]).unwrap();
            }
            let (ids, bodies): (Vec<String>, Vec<String>) = Spi::connect(|c| {
                let (mut ids, mut bodies) = (Vec::new(), Vec::new());
                for r in c.select("SELECT id, body FROM theodb.chunks_eval WHERE ws=$1", None, &[ws.as_str().into()]).unwrap() {
                    ids.push(r.get::<String>(1).unwrap().unwrap());
                    bodies.push(r.get::<String>(2).unwrap().unwrap());
                }
                (ids, bodies)
            });
            let refs: Vec<Option<&str>> = bodies.iter().map(|s| Some(s.as_str())).collect();
            let cvecs = crate::embed::run_batch(&refs, None);
            for (id, v) in ids.iter().zip(cvecs.iter()) {
                Spi::run_with_args("UPDATE theodb.chunks_eval SET emb=($1)::vector WHERE id=$2 AND ws=$3", &[v.as_str().into(), id.as_str().into(), ws.as_str().into()]).unwrap();
            }
            Spi::get_one_with_args::<i64>("SELECT theodb.graph_embed_nodes($1,'c')", &[ws.as_str().into()]).unwrap();
            Spi::get_one::<i64>("SELECT theodb.graph_build('theodb.graph_edges','src_id','dst_id')").unwrap();
            let qv = crate::embed::run(Some(q), None);
            // Ranked candidate lists (top-10 each) for RRF fusion.
            let vrank: Vec<String> = Spi::connect(|c| {
                c.select(&format!("SELECT id FROM theodb.chunks_eval WHERE ws='{ws}' ORDER BY emb <=> '{}'::vector LIMIT 10", qv.replace('\'', "''")), None, &[]).unwrap()
                    .map(|r| r.get::<String>(1).unwrap().unwrap()).collect()
            });
            let grank: Vec<String> = Spi::connect(|c| {
                c.select(&format!("SELECT chunk_id FROM theodb.graph_rag_search('{}'::vector,'{ws}','c',3,2) LIMIT 10", qv.replace('\'', "''")), None, &[]).unwrap()
                    .map(|r| r.get::<String>(1).unwrap().unwrap()).collect()
            });
            // Reciprocal Rank Fusion (the honest SOTA-fair hybrid): graph AUGMENTS vector, does not replace it.
            let mut rrf: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
            for (rank, c) in vrank.iter().enumerate() { *rrf.entry(c.clone()).or_insert(0.0) += 1.0 / (60.0 + rank as f64); }
            for (rank, c) in grank.iter().enumerate() { *rrf.entry(c.clone()).or_insert(0.0) += 1.0 / (60.0 + rank as f64); }
            let mut fused: Vec<(String, f64)> = rrf.into_iter().collect();
            fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
            let htop: Vec<String> = fused.iter().take(k as usize).map(|(c, _)| c.clone()).collect();
            let vtop: Vec<String> = vrank.iter().take(k as usize).cloned().collect();
            let gtop: Vec<String> = grank.iter().take(k as usize).cloned().collect();
            let recall = |top: &[String]| gold.iter().filter(|g| top.contains(g)).count() as f64 / gold.len() as f64;
            vsum += recall(&vtop);
            gsum += recall(&gtop);
            hsum += recall(&htop);
            n += 1;
        }
        let (vrec, grec, hrec) = (vsum / n as f64, gsum / n as f64, hsum / n as f64);
        let json = format!(
            "{{\"benchmark\":\"HotpotQA distractor (validation), HuggingFace hotpotqa/hotpot_qa\",\"questions\":{n},\"recall_at_k\":{k},\
             \"embed_model\":\"text-embedding-3-small\",\"vector_recall\":{vrec:.4},\"graph_only_recall\":{grec:.4},\
             \"hybrid_rrf_recall\":{hrec:.4},\"note\":\"hybrid = RRF(vector, graph-expanded) — the SOTA-fair comparison where the graph AUGMENTS vector\"}}\n"
        );
        let _ = std::fs::write("/tmp/m111_eval.json", &json);
        pgrx::warning!("M111_EVAL_HOTPOT n={n} recall@{k} vector={vrec:.4} graph_only={grec:.4} hybrid_rrf={hrec:.4}");
        assert!(n > 0, "eval ran over at least one HotpotQA question");
        // NOTE: no hard assert — vector / graph-only / hybrid numbers are reported honestly for the gate decision.
    }

    // M112 — the HippoRAG recipe on real HotpotQA: LLM-based extraction (richer graph than the heuristic) +
    // Personalized PageRank passage ranking, vs pure vector. For each question: LLM-extract each passage
    // (use_llm=true, real chat endpoint) → graph; embed nodes; PPR-seed = the query's entities matched to graph
    // nodes; rank passages by the summed PPR mass of their entities. Compare recall@k vs pure vector + the RRF
    // fusion of (vector, PPR). Needs THEODB_EVAL_OPENAI_KEY + dataset; SKIPs otherwise. Honest — may win or lose.
    #[pg_test]
    fn m112_eval_hotpot_llm_ppr() {
        let key = match std::env::var("THEODB_EVAL_OPENAI_KEY") {
            Ok(k) if !k.is_empty() => k,
            _ => { pgrx::warning!("M112_EVAL SKIP: no THEODB_EVAL_OPENAI_KEY"); return; }
        };
        let path = std::env::var("THEODB_EVAL_HOTPOT_PATH").unwrap_or_else(|_| "/tmp/hotpot_eval.json".to_string());
        let data = match std::fs::read_to_string(&path) { Ok(d) => d, Err(_) => { pgrx::warning!("M112_EVAL SKIP: no dataset"); return; } };
        Spi::run("SET theodb.embedding_endpoint = 'https://api.openai.com/v1/embeddings'").unwrap();
        Spi::run("SET theodb.embedding_model = 'text-embedding-3-small'").unwrap();
        Spi::run(&format!("SET theodb.embedding_api_key = '{}'", key.replace('\'', "''"))).unwrap();
        Spi::run("SET theodb.llm_endpoint = 'https://api.openai.com/v1/chat/completions'").unwrap();
        Spi::run("SET theodb.llm_model = 'gpt-4o-mini'").unwrap();
        Spi::run(&format!("SET theodb.llm_api_key = '{}'", key.replace('\'', "''"))).unwrap();
        Spi::run("CREATE TABLE IF NOT EXISTS theodb.chunks_eval(id text, ws text, body text, emb public.vector)").unwrap();

        let recs: serde_json::Value = serde_json::from_str(&data).expect("json");
        let arr = recs.as_array().expect("array");
        let k = 4i64;
        let n_q = arr.len().min(15); // LLM extraction is ~10 calls/question — cap for cost/latency
        let (mut vsum, mut psum, mut fsum, mut n) = (0.0f64, 0.0f64, 0.0f64, 0i64);
        for (qi, rec) in arr.iter().take(n_q).enumerate() {
            let ws = format!("hp{qi}");
            Spi::run("TRUNCATE theodb.graph_nodes, theodb.graph_edges").unwrap();
            Spi::run("DELETE FROM theodb.chunks_eval").unwrap();
            let q = rec["q"].as_str().unwrap_or("");
            let gold: Vec<String> = rec["gold"].as_array().map(|a| a.iter().filter_map(|t| t.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
            let paras = match rec["paras"].as_array() { Some(p) => p, None => continue };
            if gold.is_empty() || paras.is_empty() { continue; }
            for p in paras {
                let title = p[0].as_str().unwrap_or("");
                let text = p[1].as_str().unwrap_or("");
                Spi::run_with_args("INSERT INTO theodb.chunks_eval(id,ws,body) VALUES ($1,$2,$3)", &[title.into(), ws.as_str().into(), text.into()]).unwrap();
                let full = format!("{title}. {text}");
                // use_llm=true → LLM (OpenIE-style) extraction, richer than the heuristic
                Spi::get_one_with_args::<i64>("SELECT theodb.graph_upsert($1,'c',$2,$3,true)", &[ws.as_str().into(), title.into(), full.as_str().into()]).unwrap();
            }
            // embed passages (vector baseline) + nodes
            let (ids, bodies): (Vec<String>, Vec<String>) = Spi::connect(|c| {
                let (mut i, mut b) = (Vec::new(), Vec::new());
                for r in c.select("SELECT id, body FROM theodb.chunks_eval WHERE ws=$1", None, &[ws.as_str().into()]).unwrap() {
                    i.push(r.get::<String>(1).unwrap().unwrap()); b.push(r.get::<String>(2).unwrap().unwrap());
                }
                (i, b)
            });
            let refs: Vec<Option<&str>> = bodies.iter().map(|s| Some(s.as_str())).collect();
            let cv = crate::embed::run_batch(&refs, None);
            for (id, v) in ids.iter().zip(cv.iter()) {
                Spi::run_with_args("UPDATE theodb.chunks_eval SET emb=($1)::vector WHERE id=$2 AND ws=$3", &[v.as_str().into(), id.as_str().into(), ws.as_str().into()]).unwrap();
            }
            Spi::get_one_with_args::<i64>("SELECT theodb.graph_embed_nodes($1,'c')", &[ws.as_str().into()]).unwrap();
            Spi::get_one::<i64>("SELECT theodb.graph_build('theodb.graph_edges','src_id','dst_id')").unwrap();
            // query entities → seed node ids (heuristic extraction of the question, matched by normalized_name)
            let seeds: Vec<i64> = Spi::connect(|c| {
                c.select(&format!(
                    "SELECT n.id FROM theodb.graph_nodes n JOIN (SELECT normalized_name FROM ai.extract_entities('{}')) e \
                     ON e.normalized_name=n.normalized_name WHERE n.workspace_id='{ws}'", q.replace('\'', "''")
                ), None, &[]).unwrap().map(|r| r.get::<i64>(1).unwrap().unwrap()).collect()
            });
            let qv = crate::embed::run(Some(q), None);
            let vrank: Vec<String> = Spi::connect(|c| {
                c.select(&format!("SELECT id FROM theodb.chunks_eval WHERE ws='{ws}' ORDER BY emb <=> '{}'::vector LIMIT 10", qv.replace('\'', "''")), None, &[]).unwrap()
                    .map(|r| r.get::<String>(1).unwrap().unwrap()).collect()
            });
            // PPR passage ranking: passage score = Σ PPR[node] over nodes appearing in that passage's edges.
            let prank: Vec<String> = if seeds.is_empty() { Vec::new() } else {
                let seed_arr = format!("ARRAY[{}]::bigint[]", seeds.iter().map(|s| s.to_string()).collect::<Vec<_>>().join(","));
                Spi::connect(|c| {
                    c.select(&format!(
                        "WITH ppr AS (SELECT node, score FROM theodb.graph_ppr('theodb.graph_edges', {seed_arr}, 0.5, 20)), \
                         nc AS (SELECT src_id AS node, unnest(source_chunk_ids) AS chunk FROM theodb.graph_edges WHERE workspace_id='{ws}' \
                                UNION ALL SELECT dst_id, unnest(source_chunk_ids) FROM theodb.graph_edges WHERE workspace_id='{ws}') \
                         SELECT nc.chunk FROM nc JOIN ppr p ON p.node=nc.node GROUP BY nc.chunk ORDER BY sum(p.score) DESC LIMIT 10"
                    ), None, &[]).unwrap().map(|r| r.get::<String>(1).unwrap().unwrap()).collect()
                })
            };
            // RRF fusion of vector + PPR
            let mut rrf: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
            for (r, c) in vrank.iter().enumerate() { *rrf.entry(c.clone()).or_insert(0.0) += 1.0 / (60.0 + r as f64); }
            for (r, c) in prank.iter().enumerate() { *rrf.entry(c.clone()).or_insert(0.0) += 1.0 / (60.0 + r as f64); }
            let mut fused: Vec<(String, f64)> = rrf.into_iter().collect();
            fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then(a.0.cmp(&b.0)));
            let recall = |top: &[String]| gold.iter().filter(|g| top.contains(g)).count() as f64 / gold.len() as f64;
            vsum += recall(&vrank.iter().take(k as usize).cloned().collect::<Vec<_>>());
            psum += recall(&prank.iter().take(k as usize).cloned().collect::<Vec<_>>());
            fsum += recall(&fused.iter().take(k as usize).map(|(c, _)| c.clone()).collect::<Vec<_>>());
            n += 1;
        }
        let (vrec, prec, frec) = (vsum / n as f64, psum / n as f64, fsum / n as f64);
        let json = format!(
            "{{\"benchmark\":\"HotpotQA distractor, HuggingFace hotpotqa/hotpot_qa\",\"recipe\":\"HippoRAG: LLM(gpt-4o-mini) extraction + Personalized PageRank\",\
             \"questions\":{n},\"recall_at_k\":{k},\"embed_model\":\"text-embedding-3-small\",\
             \"vector_recall\":{vrec:.4},\"ppr_recall\":{prec:.4},\"hybrid_rrf_recall\":{frec:.4}}}\n"
        );
        let _ = std::fs::write("/tmp/m112_eval.json", &json);
        pgrx::warning!("M112_EVAL_LLM_PPR n={n} recall@{k} vector={vrec:.4} ppr={prec:.4} hybrid={frec:.4}");
        assert!(n > 0, "eval ran");
    }
}
