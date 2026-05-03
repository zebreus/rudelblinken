use chumsky::{prelude::*, text};

// Parser for C-style comments
pub(super) fn comment<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>>
{
    choice((
        // Line comment: // comment
        just("//").ignore_then(
            any()
                .and_is(just('\n').not())
                .repeated()
                .to_slice()
                .map(|s: &str| s.trim().to_string()),
        ),
        // Block comment: /* comment */
        just("/*")
            .ignore_then(
                any()
                    .and_is(just("*/").not())
                    .repeated()
                    .to_slice()
                    .map(|s: &str| s.trim().to_string()),
            )
            .then_ignore(just("*/")),
    ))
}

// Parser for optional comments before a declaration
pub(super) fn opt_comment<'src>()
-> impl Parser<'src, &'src str, Vec<String>, extra::Err<Rich<'src, char>>> {
    comment().padded().repeated().collect()
}

// Parser for string literals in attributes and directives
pub(super) fn string_literal<'src>()
-> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    just('"')
        .ignore_then(
            none_of('"')
                .repeated()
                .to_slice()
                .map(|s: &str| s.to_string()),
        )
        .then_ignore(just('"'))
}

// Parser for C identifiers
pub(super) fn ident<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    text::ascii::ident().map(|s: &str| s.to_string()).padded()
}
