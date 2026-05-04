use chumsky::prelude::*;

use crate::Span;

use super::super::attributes::opt_c23_attributes;
use super::super::common::{ident, opt_comment};
use super::super::model::*;
use super::types::{array_brackets, type_parser, with_array};

fn field<'src>() -> impl Parser<'src, &'src str, Field, extra::Err<Rich<'src, char>>> {
    opt_comment()
        .then(type_parser())
        .then(ident().labelled("field name"))
        .then(array_brackets().or_not())
        .then_ignore(just(';').padded())
        .map(|(((comment, field_type), name), array_size)| Field {
            name,
            field_type: with_array(field_type, array_size),
            comment,
        })
        .labelled("struct field")
}

pub(super) fn struct_decl<'src>()
-> impl Parser<'src, &'src str, StructDecl, extra::Err<Rich<'src, char>>> {
    opt_comment()
        .then(
            just("struct")
                .padded()
                .ignore_then(ident().labelled("struct name")),
        )
        .then_ignore(just('{').padded())
        .then(field().repeated().collect::<Vec<_>>())
        .then_ignore(just('}').padded())
        .then_ignore(just(';').padded())
        .map(|((comment, name), fields)| StructDecl {
            name,
            fields,
            comment,
            span: Span::default(),
        })
}

fn parameter<'src>() -> impl Parser<'src, &'src str, Parameter, extra::Err<Rich<'src, char>>> {
    type_parser()
        .then(ident().or_not())
        .then(array_brackets().or_not())
        .map(|((param_type, name), array_size)| Parameter {
            name,
            param_type: with_array(param_type, array_size),
        })
        .labelled("function parameter")
}

pub(super) fn function_decl<'src>()
-> impl Parser<'src, &'src str, FunctionDecl, extra::Err<Rich<'src, char>>> {
    opt_comment()
        .then(opt_c23_attributes())
        .then(type_parser())
        .then(ident().labelled("function name"))
        .then_ignore(just('(').padded())
        .then(
            parameter()
                .separated_by(just(',').padded())
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(')').padded())
        .then_ignore(just(';').padded())
        .map(
            |((((comment, c23_attributes), return_type), name), parameters)| FunctionDecl {
                name,
                return_type,
                parameters,
                comment,
                c23_attributes,
                span: Span::default(),
            },
        )
}

pub(super) fn variable_decl<'src>()
-> impl Parser<'src, &'src str, VariableDecl, extra::Err<Rich<'src, char>>> {
    opt_comment()
        .then(opt_c23_attributes())
        .then(type_parser())
        .then(ident().labelled("variable name"))
        .then(array_brackets().or_not())
        .then_ignore(just(';').padded())
        .map(
            |((((comment, c23_attributes), var_type), name), array_size)| VariableDecl {
                name,
                var_type: with_array(var_type, array_size),
                comment,
                c23_attributes,
                span: Span::default(),
            },
        )
}

fn enum_variant<'src>() -> impl Parser<'src, &'src str, EnumVariant, extra::Err<Rich<'src, char>>> {
    opt_comment()
        .then(ident())
        .then(
            just('=')
                .padded()
                .ignore_then(
                    text::int(10).try_map(|s: &str, span| {
                        s.parse::<i64>().map_err(|e| Rich::custom(span, e))
                    }),
                )
                .or_not(),
        )
        .map(|((comment, name), value)| EnumVariant {
            name,
            value,
            comment,
        })
        .labelled("enum variant")
}

pub(super) fn enum_decl<'src>()
-> impl Parser<'src, &'src str, EnumDecl, extra::Err<Rich<'src, char>>> {
    opt_comment()
        .then(
            just("enum")
                .padded()
                .ignore_then(ident().labelled("enum name")),
        )
        .then_ignore(just('{').padded())
        .then(
            enum_variant()
                .separated_by(just(',').padded())
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
        .then_ignore(just('}').padded())
        .then_ignore(just(';').padded())
        .map(|((comment, name), variants)| EnumDecl {
            name,
            variants,
            comment,
            span: Span::default(),
        })
}