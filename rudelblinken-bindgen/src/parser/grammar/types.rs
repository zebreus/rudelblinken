use chumsky::{prelude::*, text};

use super::super::common::ident;
use super::super::model::Type;

fn base_type<'src>() -> impl Parser<'src, &'src str, Type, extra::Err<Rich<'src, char>>> {
    choice((
        just("unsigned")
            .padded()
            .then(just("long").padded())
            .then(just("long"))
            .to(Type::UnsignedLongLong),
        just("unsigned").padded().ignore_then(choice((
            just("int").to(Type::UnsignedInt),
            just("char").to(Type::UnsignedChar),
        ))),
        just("long").padded().then(just("long")).to(Type::LongLong),
        just("void").to(Type::Void),
        just("int").to(Type::Int),
        just("char").to(Type::Char),
        just("struct")
            .padded()
            .ignore_then(ident().labelled("struct type name"))
            .map(Type::Struct),
        just("enum")
            .padded()
            .ignore_then(ident().labelled("enum type name"))
            .map(Type::Enum),
        ident().map(Type::Named),
    ))
    .padded()
}

pub(super) fn type_parser<'src>() -> impl Parser<'src, &'src str, Type, extra::Err<Rich<'src, char>>>
{
    base_type()
        .then(just('*').padded().repeated().collect::<Vec<_>>())
        .map(|(base, stars)| {
            stars
                .iter()
                .fold(base, |acc, _| Type::Pointer(Box::new(acc)))
        })
}

pub(super) fn array_brackets<'src>()
-> impl Parser<'src, &'src str, usize, extra::Err<Rich<'src, char>>> {
    just('[')
        .padded()
        .ignore_then(
            text::int(10)
                .try_map(|s: &str, span| s.parse::<usize>().map_err(|e| Rich::custom(span, e))),
        )
        .then_ignore(just(']').padded())
}

pub(super) fn with_array(type_decl: Type, array_size: Option<usize>) -> Type {
    match array_size {
        Some(size) => Type::Array(Box::new(type_decl), size),
        None => type_decl,
    }
}
