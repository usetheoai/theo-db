//! B-001: símbolos do backend PostgreSQL que o binário de teste precisa **resolver** mas nunca **usar**.
//!
//! `cargo pgrx test` compila a crate num executável standalone. Esse executável é só o cliente: os
//! `#[pg_test]` executam dentro de um backend (o harness manda `SELECT "fn"()`), e os `#[test]` puros não
//! tocam PostgreSQL. Mesmo assim o carregador dinâmico exige 16 símbolos — medidos com `ldd -r`, todos de
//! memória e tratamento de erro — porque as crates `pgrx`/`pgrx_pg_sys` os referenciam e **símbolo de dado
//! não pode ser ligado preguiçosamente**: basta ser alcançável para ser exigido no carregamento.
//!
//! Sem isto, a suíte inteira morre em `symbol lookup error: CurrentMemoryContext` antes do primeiro teste.
//!
//! **Os stubs de função abortam com mensagem.** Se algum teste realmente chamar código do PG fora de um
//! backend, ele falha alto e claro em vez de silenciosamente corromper memória — fail-fast (Regra 8) em vez
//! de conveniência. Um stub que "funcionasse" seria pior que o erro que ele substitui.
//!
//! `#[cfg(test)]` mantém tudo isto FORA do `cdylib` que é instalado. A extensão entregue nunca vê estes
//! símbolos; dentro do backend real, os do PostgreSQL é que valem.
#![cfg(test)]

use core::ffi::c_void;
use core::ptr::null_mut;

macro_rules! stub_data {
    ($($name:ident),* $(,)?) => {$(
        #[unsafe(no_mangle)]
        pub static mut $name: *mut c_void = null_mut();
    )*};
}

macro_rules! stub_fn {
    ($($name:ident),* $(,)?) => {$(
        #[unsafe(no_mangle)]
        pub extern "C" fn $name() -> *mut c_void {
            panic!(
                concat!(
                    "B-001: `", stringify!($name), "` do PostgreSQL foi chamado no binario de teste \
                     standalone, onde nao existe backend. Um `#[test]` puro nao pode tocar a API do PG — \
                     use `#[pg_test]`, que executa dentro do backend."
                )
            )
        }
    )*};
}

// Símbolos de DADO — a razão do carregamento falhar (não são ligáveis preguiçosamente).
stub_data!(CurrentMemoryContext, ErrorContext, error_context_stack, PG_exception_stack);

// Símbolos de FUNÇÃO — presentes na mesma lista do `ldd -r`; abortam se alcançados.
stub_fn!(
    CopyErrorData,
    errcode,
    errcontext_msg,
    errdetail,
    errfinish,
    errhint,
    errmsg,
    errstart,
    FlushErrorState,
    FreeErrorData,
    palloc0,
    pfree,
);
