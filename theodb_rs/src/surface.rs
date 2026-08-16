//! Manifesto da superfície SQL absorvida do umbrella `theodb` (B-030).
//!
//! **O que este módulo é.** Uma lista de declarações `extension_sql_file!`, uma por área funcional.
//! O SQL em si mora em `theodb_rs/sql/surface/*.sql` e continua sendo SQL — legível, diffável, com
//! realce de sintaxe, e revisável por quem entende de banco sem ler Rust.
//!
//! **Por que arquivo e não literal embutido (revisão do ADR D4 do plano `b031-b030-uma-extensao`).**
//! O plano previa portar as 471 linhas para dentro de blocos `extension_sql!`, e listava como risco R2
//! que "mover SQL para string literals troca erro de sintaxe em tempo de arquivo por erro em tempo de
//! build". Durante a implementação, o `extension_sql_file!` do pgrx 0.19 apareceu como caminho que
//! **elimina** o risco em vez de mitigá-lo: não há transcrição, então não há o que transcrever errado.
//! É o degrau 3 da parsimony ladder — usar o recurso da plataforma antes de escrever o nosso.
//!
//! A revisão também dissolve a razão original de "um módulo Rust por área": aquele argumento existia
//! para não fazer de `api.rs` um god-module de 471 linhas de SQL. Com o SQL fora do Rust, a separação
//! por área vive nos próprios arquivos `.sql`, e seis módulos de duas linhas cada seriam cerimônia.
//! Este manifesto único é o que resta de necessário.
//!
//! **Ordem de emissão.** O pgrx ordena por grafo de dependência, não por ordem de declaração. Cada
//! bloco declara o que precisa existir antes dele:
//!
//! - `theodb_schema_bootstrap` cria os schemas `theodb` e `ai` (`dtype.rs`) — raiz de tudo.
//! - `theodb_ai_wrappers` cria `ai._chat` (`api.rs`) — os wrappers de texto o chamam no corpo, e é essa
//!   aresta que devolve a validação em tempo de `CREATE` (ver `50-ai-text.sql`).
//! - `theodb_nl_wrappers` cria `ai.nl_to_sql` (`api.rs`) — `ai.nl_query` o invoca.
//!
//! Errar uma dessas arestas não quebra a compilação: quebra o `CREATE EXTENSION`. Por isso o teste
//! `surface_contract::tests::surface_contains_public_api` roda uma instalação de verdade.

use pgrx::extension_sql_file;

// ai.generate / ai.summarize / ai.agg_summarize — wrappers de texto sobre ai._chat.
// `requires` em theodb_ai_wrappers é LOAD-BEARING: os corpos são LANGUAGE sql e referenciam ai._chat,
// então o PostgreSQL resolve a referência no CREATE. Sem a aresta, a instalação falha — que é o
// comportamento correto e a garantia que o colapso do umbrella recupera.
extension_sql_file!(
    "../sql/surface/50-ai-text.sql",
    name = "theodb_surface_ai_text",
    requires = ["theodb_ai_wrappers", "theodb_schema_bootstrap"],
);

// ai.nl_query — gera+valida via ai.nl_to_sql (Rust) e executa o SELECT num sandbox read-only.
// Permanece LANGUAGE plpgsql por necessidade real, não por resíduo: o corpo tem EXECUTE dinâmico,
// set_config de transação e RAISE — lógica procedural que uma função SQL não expressa.
extension_sql_file!(
    "../sql/surface/60-nl.sql",
    name = "theodb_surface_nl",
    requires = ["theodb_nl_wrappers", "theodb_schema_bootstrap"],
);

// Catálogo de configuração do NL: ai.nl_config / ai.nl_templates / ai.nl_value_index + as funções que
// os mantêm. Depende do bloco acima porque ai.nl_query_cfg delega a ai.nl_query.
extension_sql_file!(
    "../sql/surface/61-nl-config.sql",
    name = "theodb_surface_nl_config",
    requires = ["theodb_surface_nl"],
);

// Registry de modelos theodb_ml — cria o PRÓPRIO schema (`theodb_ml`), diferente de theodb/ai.
extension_sql_file!(
    "../sql/surface/70-ml-registry.sql",
    name = "theodb_surface_ml_registry",
    requires = ["theodb_schema_bootstrap"],
);

// theodb.import_vectors_chunked — PROCEDURE (não função): faz COMMIT por lote, o que só um
// procedimento pode. Permanece plpgsql pelo mesmo motivo do 60: laço e transação explícita.
extension_sql_file!(
    "../sql/surface/80-import-vectors.sql",
    name = "theodb_surface_import_vectors",
    requires = ["theodb_schema_bootstrap"],
);

// Superfície HTAP/OLAP — theodb.htap_refresh / htap_register / olap / htap_freshness + o registro de
// snapshots. Depende do writer Parquet own-code (M143, ADR-0057).
extension_sql_file!(
    "../sql/surface/85-htap.sql",
    name = "theodb_surface_htap",
    requires = ["theodb_schema_bootstrap", "parquet_revoke_public"],
);
