//! O analisador lexical nomeado do TheoDB — stemming e stopwords sobre a cadeia do Tantivy.
//!
//! # Por que um nome próprio, e não redefinir `"default"`
//!
//! O Tantivy serializa o **nome** do tokenizer dentro do schema do campo
//! (`TextFieldIndexing { tokenizer: TokenizerName }`), e o schema vai para o `meta.json` do índice. Então
//! todo índice construído antes deste módulo carrega `"default"` no próprio schema, para sempre.
//!
//! Isso torna a escolha do nome uma decisão de compatibilidade, não de estilo:
//!
//! * **Nome novo** (o que este módulo faz): índices antigos continuam resolvendo `"default"`, que o
//!   `TokenizerManager` do Tantivy sempre registra — comportamento deles **intacto**, sem migração. Índices
//!   novos nascem com a cadeia nova.
//! * **Redefinir `"default"`**: mudaria a semântica de busca de toda instalação existente em silêncio —
//!   consulta stemizada contra índice não stemizado degrada recall sem erro nenhum.
//!
//! O risco de migração some por construção. É por isso que não há script de rebuild neste ciclo.
//!
//! # Idioma
//!
//! Inglês fixo nesta versão. O idioma é propriedade **do índice**, não da sessão — um GUC permitiria
//! construir em inglês e consultar em francês, que é a divergência de analisador que o nome próprio existe
//! para impedir. O sufixo `_en` reserva o espaço: um `theodb_pt` futuro convive sem ambiguidade e sem
//! tocar índice nenhum.

use tantivy::Index;
use tantivy::tokenizer::{
    Language, LowerCaser, RemoveLongFilter, SimpleTokenizer, Stemmer, StopWordFilter, TextAnalyzer,
};

/// O nome sob o qual a cadeia é registrada e gravada no schema dos índices novos.
///
/// **Nunca** pode ser `"default"` — ver o módulo. Um teste garante isso, porque a distância entre os dois
/// é uma linha e a consequência é silenciosa.
pub(crate) const ANALYZER_NAME: &str = "theodb_en";

/// Idioma da cadeia. Constante por ora (ver o módulo); nomeado para que a troca seja um ponto, não uma busca.
const ANALYZER_LANGUAGE: Language = Language::English;

/// Limite de comprimento de token, em bytes. Herda o valor que o `"default"` do Tantivy usa, para que a
/// única diferença mensurável entre as duas cadeias seja stopwords + stemming — e não um corte distinto de
/// tokens longos que confundiria a leitura do benchmark.
const MAX_TOKEN_BYTES: usize = 40;

/// Constrói a cadeia: tokenização simples → corte de tokens longos → minúsculas → stopwords → stemmer.
///
/// A ordem importa. O `StopWordFilter` compara contra uma lista em minúsculas e **antes** do stemmer:
/// depois dele, `the` já teria virado outro radical e não casaria a lista.
pub(crate) fn build_analyzer() -> TextAnalyzer {
    let stop_words = StopWordFilter::new(ANALYZER_LANGUAGE)
        .expect("tantivy não traz lista de stopwords para o idioma configurado");
    TextAnalyzer::builder(SimpleTokenizer::default())
        .filter(RemoveLongFilter::limit(MAX_TOKEN_BYTES))
        .filter(LowerCaser)
        .filter(stop_words)
        .filter(Stemmer::new(ANALYZER_LANGUAGE))
        .build()
}

/// Registra a cadeia no índice, sob o nome próprio.
///
/// Precisa rodar nos **dois** pontos de abertura — `Index::create` na indexação e `Index::open` na busca.
/// O `QueryParser` resolve o tokenizer pelo nome que está no schema do campo e o procura neste registro; se
/// não achar, devolve `UnknownTokenizer`. Registrar de um lado só produziria indexação e consulta com
/// analisadores diferentes, que é recall degradado sem erro.
pub(crate) fn register(index: &Index) {
    index.tokenizers().register(ANALYZER_NAME, build_analyzer());
}

#[cfg(any(test, feature = "pg_test"))]
mod tests {
    use super::*;

    /// Tokens produzidos pela cadeia, para asserção direta.
    fn tokens(text: &str) -> Vec<String> {
        let mut analyzer = build_analyzer();
        let mut stream = analyzer.token_stream(text);
        let mut out = Vec::new();
        while stream.advance() {
            out.push(stream.token().text.clone());
        }
        out
    }

    #[test]
    fn stems_english_inflections_to_the_same_root() {
        // O sintoma que abriu o item: `jumping` não casava `jumps`.
        assert_eq!(tokens("jumping"), tokens("jumps"));
        assert_eq!(tokens("jumping"), vec!["jump".to_string()]);
    }

    #[test]
    fn removes_english_stopwords() {
        assert_eq!(tokens("the lazy dog"), vec!["lazi".to_string(), "dog".to_string()]);
    }

    #[test]
    fn a_query_of_only_stopwords_produces_no_tokens() {
        // Consequência legítima: a busca devolve vazio, e isso NÃO é erro.
        assert!(tokens("the of and").is_empty());
    }

    #[test]
    fn lowercases_and_splits_on_punctuation() {
        assert_eq!(tokens("LAZY, Dog!"), vec!["lazi".to_string(), "dog".to_string()]);
    }

    #[test]
    fn analyzer_name_is_never_the_tantivy_default() {
        // Redefinir "default" mudaria a semântica de todo índice existente em silêncio.
        assert_ne!(ANALYZER_NAME, "default");
    }

    #[test]
    fn register_makes_the_analyzer_resolvable_by_name() {
        use tantivy::schema::{STORED, Schema, TEXT};
        let mut sb = Schema::builder();
        sb.add_text_field("body", TEXT | STORED);
        let index = Index::create_in_ram(sb.build());
        assert!(index.tokenizers().get(ANALYZER_NAME).is_none(), "não deveria existir antes");
        register(&index);
        assert!(index.tokenizers().get(ANALYZER_NAME).is_some(), "não resolveu após register");
    }
}
