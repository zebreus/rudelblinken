mod declarations;
mod directives;
mod types;

use chumsky::{prelude::*, span::SimpleSpan, text};

use crate::Span;

use super::model::*;

fn source_span(source: &str, span: SimpleSpan) -> Span {
    Span {
        source: source.to_string(),
        start: span.start,
        end: span.end,
    }
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

    let struct_parser = spanned_declaration_parser!(
        declarations::struct_decl(),
        "struct declaration",
        Struct,
        StructDecl
    );
    let enum_parser = spanned_declaration_parser!(
        declarations::enum_decl(),
        "enum declaration",
        Enum,
        EnumDecl
    );
    let directive_parser = directives::directive_decl()
        .labelled("preprocessor directive")
        .map(ParsedDeclaration::Directive);
    let function_parser = spanned_declaration_parser!(
        declarations::function_decl(),
        "function declaration",
        Function,
        FunctionDecl
    );
    let variable_parser = spanned_declaration_parser!(
        declarations::variable_decl(),
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