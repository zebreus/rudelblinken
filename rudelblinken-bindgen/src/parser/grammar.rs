use chumsky::{prelude::*, span::SimpleSpan, text};

use crate::Span;

use super::attributes::opt_c23_attributes;
use super::common::{ident, opt_comment, string_literal};
use super::model::*;

fn source_span(source: &str, span: SimpleSpan) -> Span {
    Span {
        source: source.to_string(),
        start: span.start,
        end: span.end,
    }
}

// Parser for base types
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

// Parser for types with pointers (arrays are handled next to declarator names).
fn type_parser<'src>() -> impl Parser<'src, &'src str, Type, extra::Err<Rich<'src, char>>> {
    base_type()
        .then(just('*').padded().repeated().collect::<Vec<_>>())
        .map(|(base, stars)| {
            stars
                .iter()
                .fold(base, |acc, _| Type::Pointer(Box::new(acc)))
        })
}

// Parser for array dimension
fn array_brackets<'src>() -> impl Parser<'src, &'src str, usize, extra::Err<Rich<'src, char>>> {
    just('[')
        .padded()
        .ignore_then(
            text::int(10)
                .try_map(|s: &str, span| s.parse::<usize>().map_err(|e| Rich::custom(span, e))),
        )
        .then_ignore(just(']').padded())
}

fn with_array(type_decl: Type, array_size: Option<usize>) -> Type {
    match array_size {
        Some(size) => Type::Array(Box::new(type_decl), size),
        None => type_decl,
    }
}

// Parser for struct fields
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

// Parser for struct declarations
fn struct_decl<'src>() -> impl Parser<'src, &'src str, StructDecl, extra::Err<Rich<'src, char>>> {
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

// Parser for function parameters
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

// Parser for function declarations
fn function_decl<'src>() -> impl Parser<'src, &'src str, FunctionDecl, extra::Err<Rich<'src, char>>>
{
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

// Parser for variable declarations
fn variable_decl<'src>() -> impl Parser<'src, &'src str, VariableDecl, extra::Err<Rich<'src, char>>>
{
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

// Parser for enum variants
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

// Parser for enum declarations
fn enum_decl<'src>() -> impl Parser<'src, &'src str, EnumDecl, extra::Err<Rich<'src, char>>> {
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

// Parser for preprocessor directives and static asserts
fn directive_decl<'src>() -> impl Parser<'src, &'src str, Directive, extra::Err<Rich<'src, char>>> {
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

enum ParsedDeclaration {
    Struct(StructDecl),
    Function(FunctionDecl),
    Variable(VariableDecl),
    Enum(EnumDecl),
    Directive(Directive),
}

impl ParsedDeclaration {
    fn push_into(self, declarations: &mut Declarations) {
        match self {
            ParsedDeclaration::Struct(decl) => declarations.structs.push(decl),
            ParsedDeclaration::Function(decl) => declarations.functions.push(decl),
            ParsedDeclaration::Variable(decl) => declarations.variables.push(decl),
            ParsedDeclaration::Enum(decl) => declarations.enums.push(decl),
            ParsedDeclaration::Directive(decl) => declarations.directives.push(decl),
        }
    }
}

/// Parse C declarations from a string.
///
/// `source` is the display name of the input (e.g. a filename or `"<stdin>"`).
/// It is embedded in the [`Span`] of every parsed declaration so that
/// error messages can reference the originating file.
pub(super) fn parse_declarations<'src>(
    input: &'src str,
    source: &str,
) -> Result<Declarations, Vec<Rich<'src, char>>> {
    macro_rules! spanned_declaration_parser {
        ($parser:expr, $label:literal, $variant:ident, $decl_type:ident) => {
            $parser.labelled($label).map_with(|decl, extra| {
                ParsedDeclaration::$variant($decl_type {
                    span: source_span(source, extra.span()),
                    ..decl
                })
            })
        };
    }

    let struct_parser =
        spanned_declaration_parser!(struct_decl(), "struct declaration", Struct, StructDecl);
    let enum_parser = spanned_declaration_parser!(enum_decl(), "enum declaration", Enum, EnumDecl);
    let directive_parser = directive_decl()
        .labelled("preprocessor directive")
        .map(ParsedDeclaration::Directive);
    let function_parser = spanned_declaration_parser!(
        function_decl(),
        "function declaration",
        Function,
        FunctionDecl
    );
    let variable_parser = spanned_declaration_parser!(
        variable_decl(),
        "variable declaration",
        Variable,
        VariableDecl
    );

    let parser = text::whitespace()
        .ignore_then(
            choice((
                directive_parser,
                struct_parser,
                enum_parser,
                function_parser,
                variable_parser,
            ))
            .padded()
            .repeated()
            .collect::<Vec<_>>(),
        )
        .then_ignore(end());

    let declarations = parser.parse(input).into_result()?;

    let mut result = Declarations::default();
    for declaration in declarations {
        declaration.push_into(&mut result);
    }

    Ok(result)
}
