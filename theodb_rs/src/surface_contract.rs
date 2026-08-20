//! Contrato da superfície instalada — o oráculo que vive junto do código que a produz.
//!
//! **Por que este módulo existe (ADR D3 do plano `b031-b030-uma-extensao`).** Antes dele, a superfície
//! da extensão era verificada por fora: `theodb_rs/sql/schema_snapshot.sql` emite dois snapshots
//! (membresia via `pg_depend`, e ACL via `proacl`), e um runner externo comparava a saída contra uma
//! baseline versionada. Esse runner foi removido em `8605677`. O snapshot continuou existindo e parou
//! de ser comparado com qualquer coisa — o que é indistinguível de não existir.
//!
//! Os três testes abaixo convertem aquela comparação em asserção executável, que roda na suíte normal
//! (`cargo pgrx test pg18`) e falha sozinha, sem baseline a versionar nem script a orquestrar.
//!
//! **Escopo (SRP).** Este módulo não testa comportamento de nenhuma função — testa o CONTRATO da
//! extensão instalada: quais objetos existem, quem pode executá-los, e em que linguagem os wrappers
//! públicos foram criados. Tem uma única razão para mudar: o contrato público mudou.
//!
//! Os três eixos são deliberadamente separados, porque falham por motivos diferentes:
//!
//! | Teste | Pergunta | Modo de falha que ele pega |
//! |---|---|---|
//! | `surface_contains_public_api` | o objeto existe? | migração que esquece um objeto; `requires` mal ordenado |
//! | `egress_surface_is_revoked_from_public` | quem pode chamar? | `DROP`+`CREATE` que perde o `REVOKE` (só `CREATE OR REPLACE` preserva ACL) |
//! | `ai_wrappers_are_sql_language` | foi validado no `CREATE`? | wrapper late-bound, cujo corpo não é conferido contra a função que ele chama |

#[cfg(any(test, feature = "pg_test"))]
#[pgrx::pg_schema]
mod tests {
    use pgrx::prelude::*;

    /// Superfície pública que UM ÚNICO `CREATE EXTENSION theodb_rs` deve entregar.
    ///
    /// Cada entrada é a assinatura como `pg_describe_object` a imprime. A lista existe para que a
    /// ausência de um objeto seja um teste vermelho, e não uma descoberta em produção.
    ///
    /// **Estado durante a migração B-030:** os objetos marcados `[umbrella]` ainda vivem na extensão
    /// `theodb` e por isso NÃO aparecem numa base que instalou só o `theodb_rs`. Eles estão listados
    /// de propósito — é o RED que a Fase 3 do plano fecha, tarefa a tarefa.
    const PUBLIC_SURFACE: &[&str] = &[
        // --- já no theodb_rs (devem passar desde já) ---
        "function theodb.embed(text,text)",
        "function theodb.embed_batch(text[],text)",
        "function theodb.chunk(text,text,integer,integer)",
        "function ai._chat(text,text,text)",
        // B-033 — a ordem do tipo `vector`. Entram no contrato porque uma superfície que some sem
        // ninguém perceber é o modo de falha que este módulo existe para fechar: sem `=` o usuário
        // perde `DISTINCT`, `GROUP BY`, `ORDER BY` e chave única, com erro do PostgreSQL que não cita
        // o TheoDB.
        "function theodb_vector_cmp(vector,vector)",
        "function theodb_vector_eq(vector,vector)",
        "function theodb_vector_ne(vector,vector)",
        "function theodb_vector_lt(vector,vector)",
        "function theodb_vector_le(vector,vector)",
        "function theodb_vector_gt(vector,vector)",
        "function theodb_vector_ge(vector,vector)",
        // O formato desta string foi MEDIDO (2026-08-12) contra as opclasses já existentes:
        // `operator class theodb_hnsw_l2_ops for access method theodb_hnsw`. Uma `operator family`
        // NÃO foi incluída: nenhuma aparece como membro da extensão para os AMs atuais, e afirmar um
        // formato não medido é o erro que este ciclo já cometeu com "procedure" vs "function".
        "operator class vector_ops for access method btree",
        // --- [umbrella] absorvidos pela Fase 3 ---
        "function ai.generate(text,text)",                 // T3.1
        "function ai.summarize(text,text)",                // T3.1
        "function ai.agg_summarize(text)",                 // T3.1 (agregado)
        "function ai.nl_query(text,text[],text,integer)",  // T3.2
        "function ai.nl_query_cfg(text,text,integer)",     // T3.2
        "function theodb_ml.create_model(text,text,text)", // T3.3
        "function theodb_ml.apply_model(text)",            // T3.3
        // MEDIDO 2026-08-12: `pg_describe_object` imprime "function" mesmo para `prokind='p'`, então
        // esperar "procedure" aqui testava a REDAÇÃO do PostgreSQL, não a propriedade que importa.
        // A propriedade — ser procedimento, que é o que permite `COMMIT` por lote — é asserida em
        // `import_vectors_chunked_is_a_procedure`, onde ela pertence.
        "function theodb.import_vectors_chunked(regclass,jsonb,integer,text,text,text)",
        "function theodb.htap_refresh(regclass)", // T3.5
        "function theodb.olap(regclass)",         // T3.5
    ];

    /// Funções que fazem **egress HTTP server-side** e, por isso, NUNCA podem ter `EXECUTE` para PUBLIC.
    ///
    /// Conceder qualquer uma delas a PUBLIC abre chamada de rede de saída para todo papel do banco.
    /// A lista é `(schema, nome)` — sem assinatura, porque a asserção vale para TODA sobrecarga: uma
    /// sobrecarga nova que nascesse sem `REVOKE` seria exatamente o defeito a pegar.
    const EGRESS_SURFACE: &[(&str, &str)] = &[
        ("ai", "_chat"),
        ("ai", "generate"),
        ("ai", "summarize"),
        ("ai", "agg_summarize"),
        ("ai", "if"),
        ("ai", "rank"),
        ("ai", "analyze_sentiment"),
        ("ai", "generate_batch"),
        ("theodb", "embed"),
        ("theodb", "embed_batch"),
    ];

    /// Wrappers públicos que devem ser `LANGUAGE sql` — validados contra o corpo em tempo de `CREATE`.
    ///
    /// Enquanto viveram na extensão `theodb`, foram `plpgsql` por necessidade: `ai._chat` é criado pelo
    /// `theodb_rs`, instalado DEPOIS, então um corpo SQL não teria como resolver a referência. Absorvidos
    /// pelo `theodb_rs` com `requires`, a ordem passa a ser garantida e o late-binding deixa de ser preciso.
    const SQL_LANGUAGE_WRAPPERS: &[(&str, &str)] = &[("ai", "generate"), ("ai", "summarize")];

    /// Objetos que são membros da extensão, na forma canônica do `pg_describe_object`.
    ///
    /// Mesma consulta do primeiro bloco de `theodb_rs/sql/schema_snapshot.sql` (Regra 9: reusar o
    /// oráculo existente, não escrever um segundo com semântica ligeiramente diferente).
    fn extension_objects() -> Vec<String> {
        Spi::connect(|client| {
            client
                .select(
                    "SELECT pg_describe_object(d.classid, d.objid, d.objsubid) AS object \
                     FROM pg_depend d \
                     JOIN pg_extension e ON e.oid = d.refobjid \
                     WHERE d.refclassid = 'pg_extension'::regclass \
                       AND d.deptype = 'e' \
                       AND e.extname = 'theodb_rs' \
                     ORDER BY 1",
                    None,
                    &[],
                )
                .unwrap()
                .filter_map(|row| row.get::<String>(1).ok().flatten())
                .collect::<Vec<_>>()
        })
    }

    /// T2.1 — todo objeto declarado em [`PUBLIC_SURFACE`] existe após instalar SÓ o `theodb_rs`.
    ///
    /// Falha listando **todos** os ausentes, não apenas o primeiro: durante a migração o valor está em
    /// ver o conjunto encolher a cada tarefa, e um teste que para no primeiro esconde o progresso.
    #[pg_test]
    fn surface_contains_public_api() {
        let present = extension_objects();
        let missing: Vec<&str> = PUBLIC_SURFACE
            .iter()
            .copied()
            .filter(|expected| !present.iter().any(|got| got == expected))
            .collect();

        assert!(
            missing.is_empty(),
            "objetos ausentes da superfície de `theodb_rs` ({} de {}): {:#?}\n\
             instalados: {:#?}",
            missing.len(),
            PUBLIC_SURFACE.len(),
            missing,
            present,
        );
    }

    /// T2.2 — nenhuma função de egress concede `EXECUTE` a PUBLIC.
    ///
    /// Duas formas de PUBLIC ter o privilégio, e as duas contam:
    ///
    /// 1. `proacl IS NULL` — ACL default. Em PostgreSQL o default de uma função **concede** `EXECUTE`
    ///    a PUBLIC, então ACL ausente é permissivo, não restritivo. É a pegadinha desta verificação.
    /// 2. Um aclitem cujo beneficiário é vazio (`=X/dono`) — concessão explícita a PUBLIC.
    ///
    /// Uma função ausente não é violação: quem responde por presença é [`surface_contains_public_api`].
    #[pg_test]
    fn egress_surface_is_revoked_from_public() {
        let mut exposed: Vec<String> = Vec::new();

        for (schema, name) in EGRESS_SURFACE {
            let leaked = Spi::get_one_with_args::<bool>(
                "SELECT COALESCE(bool_or(p.proacl IS NULL OR EXISTS ( \
                     SELECT 1 FROM unnest(p.proacl) a WHERE a::text ~ '^=[a-zA-Z]*X' \
                 )), false) \
                 FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace \
                 WHERE n.nspname = $1 AND p.proname = $2",
                &[(*schema).into(), (*name).into()],
            )
            .ok()
            .flatten()
            .unwrap_or(false);

            if leaked {
                exposed.push(format!("{schema}.{name}"));
            }
        }

        assert!(
            exposed.is_empty(),
            "funções de egress HTTP com EXECUTE para PUBLIC — todo papel do banco pode disparar \
             chamada de rede de saída: {exposed:#?}",
        );
    }

    /// T2.3 — os wrappers públicos são `LANGUAGE sql`, logo validados em tempo de `CREATE`.
    ///
    /// Um wrapper `plpgsql` late-bound aceita, no `CREATE`, uma chamada a função que não existe ou cuja
    /// assinatura mudou; o erro só aparece quando o usuário chama. `LANGUAGE sql` resolve a referência
    /// na criação — é a garantia que o colapso do umbrella recupera.
    ///
    /// Uma função ausente falha aqui de propósito: enquanto ela não existir no `theodb_rs`, a garantia
    /// não foi recuperada, e um teste que a ignorasse ficaria verde sem nada ter sido entregue.
    #[pg_test]
    fn ai_wrappers_are_sql_language() {
        let mut wrong: Vec<String> = Vec::new();

        for (schema, name) in SQL_LANGUAGE_WRAPPERS {
            // `nspname`/`proname` são do tipo `name`, não `text`. Comparar direto com um parâmetro
            // ligado deixa o planejador sem tipo para resolver; o cast explícito remove a ambiguidade.
            //
            // O `.expect` é deliberado e corrige um defeito da primeira versão deste teste: ela usava
            // `.ok().flatten()`, que colapsa "a consulta falhou" e "não encontrei nada" no MESMO None.
            // Medido em 2026-08-12, isso reportou `ai.generate = AUSENTE` para uma função que estava
            // instalada — e só não virou diagnóstico errado porque `surface_contains_public_api`
            // contradisse. Engolir erro dentro do teste escrito para exigir rigor é a Regra 8 violada
            // no lugar mais caro possível.
            let lang = Spi::get_one_with_args::<String>(
                // `lanname` também é `name`, não `text` — o cast na projeção é tão necessário quanto
                // os do WHERE. Sem ele, o pgrx recusa converter o datum para String e a consulta
                // falha com IncompatibleTypes{name,text}. Foi EXATAMENTE este erro que a versão com
                // `.ok().flatten()` transformava num plácido "AUSENTE".
                "SELECT l.lanname::text FROM pg_proc p \
                 JOIN pg_language l ON l.oid = p.prolang \
                 JOIN pg_namespace n ON n.oid = p.pronamespace \
                 WHERE n.nspname::text = $1 AND p.proname::text = $2 \
                 ORDER BY p.oid LIMIT 1",
                &[(*schema).into(), (*name).into()],
            )
            .unwrap_or_else(|e| panic!("consulta de linguagem falhou para {schema}.{name}: {e:?}"));

            match lang.as_deref() {
                Some("sql") => {}
                Some(other) => wrong.push(format!("{schema}.{name} = {other}")),
                None => wrong.push(format!("{schema}.{name} = AUSENTE")),
            }
        }

        assert!(
            wrong.is_empty(),
            "wrappers públicos que não são `LANGUAGE sql` (logo, não validados no CREATE): {wrong:#?}",
        );
    }

    /// T3.4 — `theodb.import_vectors_chunked` é PROCEDIMENTO, não função.
    ///
    /// A distinção é funcional, não cosmética: só um procedimento pode emitir `COMMIT`, e é isso que
    /// permite importar em lotes com pegada de memória e WAL limitadas. Uma migração feita no olho
    /// converteria em `FUNCTION` sem erro visível — e a importação passaria a ser tudo-ou-nada.
    ///
    /// Vive num teste próprio porque `pg_describe_object` **imprime "function" mesmo para
    /// `prokind='p'`** (medido). Testar a redação do catálogo não prova a propriedade; `prokind` prova.
    #[pg_test]
    fn import_vectors_chunked_is_a_procedure() {
        let kind = Spi::get_one::<i8>(
            "SELECT p.prokind FROM pg_proc p \
             JOIN pg_namespace n ON n.oid = p.pronamespace \
             WHERE n.nspname::text = 'theodb' AND p.proname::text = 'import_vectors_chunked'",
        )
        .unwrap_or_else(|e| panic!("consulta de prokind falhou: {e:?}"));

        assert_eq!(
            kind,
            Some(b'p' as i8),
            "theodb.import_vectors_chunked deveria ser PROCEDURE (prokind='p'); obtido: {kind:?}",
        );
    }
}
