use chumsky::{prelude::*, text};

use super::super::common::{ident, string_literal};
use super::super::model::Directive;

pub(super) fn directive_decl<'src>()
-> impl Parser<'src, &'src str, Directive, extra::Err<Rich<'src, char>>> {
    choice((
        just("#pragma")
            .padded()
            .ignore_then(just("once").padded())
            .map(|_| Directive::Pragma("once".to_string())),
        just("#define")
            .padded()
            .ignore_then(ident().padded())
            .then(text::ident().padded())
            .map(|(k, v)| Directive::Define {
                name: k,
                value: v.to_string(),
            }),
        just("static_assert")
            .padded()
            .ignore_then(just('(').padded())
            .ignore_then(
                none_of(',')
                    .repeated()
                    .to_slice()
                    .map(|s: &str| s.trim().to_string()),
            )
            .then_ignore(just(',').padded())
            .then(string_literal())
            .then_ignore(just(')').padded())
            .then_ignore(just(';').padded())
            .map(|(expr, message)| Directive::StaticAssert { expr, message }),
    ))
}
