use proc_macro::TokenStream;
use quote::quote;
use syn::Stmt;

use pluraless_impl::{xenglish_plural, PluralizedWord};

// pluralized_let!{let theses = n}
//
//   ->
//
// let theses = {
//     const TMP123: PluralizedWord = PluralizedWord { plural: "theses", singular: "thesis" };
//     TMP123.n($n)
// };

#[proc_macro]
pub fn pluralized_let(input: TokenStream) -> TokenStream {
    let ast: Stmt = syn::parse(input).expect("can't parse as Rust code");

    if let Stmt::Local(local) = ast {
        let var_ident = match &local.pat {
            syn::Pat::Ident(s) => &s.ident,
            _ => panic!("expecting a plain variable name after `let`"),
        };
        let rhs_expr = &local
            .init
            .expect("missing right hand side of `let` expression")
            .expr;

        let var_name = var_ident.to_string();
        let PluralizedWord { plural, singular } = xenglish_plural(&var_name);

        let code = quote! {
            #[allow(non_snake_case)]
            let #var_ident = {
                // Relying on pluraless re-exporting these identifiers
                // from pluraless_impl
                const GEN123: pluraless::PluralizedWord = pluraless::PluralizedWord {
                    plural: #plural,
                    singular: #singular,
                };
                GEN123.n(#rhs_expr)
            };
        };
        code.into()
    } else {
        panic!("expecting `let` statement");
    }
}
