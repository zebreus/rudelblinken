use chumsky::{prelude::*, text};

// Container for all parsed declarations
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Declarations {
    /// Parsed struct declarations
    pub structs: Vec<StructDecl>,
    /// Parsed function declarations
    pub functions: Vec<FunctionDecl>,
    /// Parsed variable declarations
    pub variables: Vec<VariableDecl>,
    /// Parsed enum declarations
    pub enums: Vec<EnumDecl>,
    /// Parsed typedefs (not yet fully utilized but defined)
    pub typedefs: Vec<TypedefDecl>,
    /// Parsed preprocessor directives (pragma, static_assert, define)
    pub directives: Vec<Directive>,
}

/// C enum declaration: `enum Name { variants... };`
#[derive(Clone, Debug, PartialEq)]
pub struct EnumDecl {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub comment: Vec<String>,
}

/// A variant in a C enum
#[derive(Clone, Debug, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub value: Option<i64>,
    pub comment: Vec<String>,
}

/// A C typedef declaration
#[derive(Clone, Debug, PartialEq)]
pub struct TypedefDecl {
    pub name: String,
    pub target_type: Type,
    pub comment: Vec<String>,
}

/// Preprocessor directives and static asserts
#[derive(Clone, Debug, PartialEq)]
pub enum Directive {
    Pragma(String),
    StaticAssert { expr: String, message: String },
    Define { name: String, value: String },
}

/// C struct declaration: `struct Name { fields... };`
#[derive(Clone, Debug, PartialEq)]
pub struct StructDecl {
    /// The name of the struct
    pub name: String,
    /// The fields defined in the struct
    pub fields: Vec<Field>,
    /// Documentation comments preceding the struct
    pub comment: Vec<String>,
}

/// C function declaration: `return_type name(params);`
#[derive(Clone, Debug, PartialEq)]
pub struct FunctionDecl {
    /// The name of the function
    pub name: String,
    /// The return type of the function
    pub return_type: Type,
    /// The parameters of the function
    pub parameters: Vec<Parameter>,
    /// Documentation comments preceding the function
    pub comment: Vec<String>,
    /// C23 attribute specifier sequence `[[...]]` if present
    pub c23_attributes: Option<C23Attributes>,
}

/// C variable declaration: `type name;`
#[derive(Clone, Debug, PartialEq)]
pub struct VariableDecl {
    /// The name of the variable
    pub name: String,
    /// The type of the variable
    pub var_type: Type,
    /// Documentation comments preceding the variable
    pub comment: Vec<String>,
}

/// A field in a struct: `type name;`
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    /// The name of the field
    pub name: String,
    /// The type of the field
    pub field_type: Type,
    /// Documentation comments preceding the field
    pub comment: Vec<String>,
}

/// A function parameter: `type name` or just `type` for anonymous parameters
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    /// The name of the parameter (None for anonymous parameters)
    pub name: Option<String>,
    /// The type of the parameter
    pub param_type: Type,
}

/// C23 attribute specifier sequence `[[attr1, attr2]]`
///
/// Covers both standard C23 attributes and clang-namespaced WASM linkage
/// attributes (`[[clang::import_module(...)]]`, etc.). GNU-style
/// `__attribute__((...))` is not accepted.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct C23Attributes {
    /// `[[deprecated]]` or `[[deprecated("message")]]`
    pub deprecated: Option<Option<String>>,
    /// `[[nodiscard]]` or `[[nodiscard("reason")]]`
    pub nodiscard: Option<Option<String>>,
    /// `[[maybe_unused]]`
    pub maybe_unused: Option<()>,
    /// `[[noreturn]]`
    pub noreturn: Option<()>,
    /// `[[clang::import_module("module_name")]]`
    pub import_module: Option<String>,
    /// `[[clang::import_name("function_name")]]`
    pub import_name: Option<String>,
    /// `[[clang::export_name("export_name")]]`
    pub export_name: Option<String>,
}

/// C type representation
#[derive(Clone, Debug, PartialEq)]
pub enum Type {
    /// `void`
    Void,
    /// `int`
    Int,
    /// `unsigned int`
    UnsignedInt,
    /// `char`
    Char,
    /// `unsigned char`
    UnsignedChar,
    /// `long long`
    LongLong,
    /// `unsigned long long`
    UnsignedLongLong,
    /// `struct Name`
    Struct(String),
    /// `enum Name`
    Enum(String),
    /// Pointer to another type: `type*`
    Pointer(Box<Type>),
    /// Array of another type: `type[size]`
    Array(Box<Type>, usize),
    /// Named type (typedef or other identifier)
    Named(String),
}

// Parser for C-style comments
fn comment<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
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
fn opt_comment<'src>() -> impl Parser<'src, &'src str, Vec<String>, extra::Err<Rich<'src, char>>> {
    comment().padded().repeated().collect()
}

// Parser for string literals in attributes
fn string_literal<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    just('"')
        .ignore_then(
            none_of('"')
                .repeated()
                .to_slice()
                .map(|s: &str| s.to_string()),
        )
        .then_ignore(just('"'))
}

// Discriminant enum for a single parsed C23 attribute item
#[derive(Clone)]
enum C23AttributeItem {
    Deprecated(Option<String>),
    Nodiscard(Option<String>),
    MaybeUnused,
    Noreturn,
    ImportModule(String),
    ImportName(String),
    ExportName(String),
}

// Parser for a string argument inside an attribute: `("value")`
fn opt_string_arg<'src>()
-> impl Parser<'src, &'src str, Option<String>, extra::Err<Rich<'src, char>>> {
    just('(')
        .padded()
        .ignore_then(string_literal())
        .then_ignore(just(')').padded())
        .or_not()
}

// Parser for individual C23 attribute items (standard and clang-namespaced)
fn c23_attribute_item<'src>()
-> impl Parser<'src, &'src str, C23AttributeItem, extra::Err<Rich<'src, char>>> {
    // Standard C23 attributes (no namespace)
    let deprecated = just("deprecated")
        .padded()
        .ignore_then(opt_string_arg())
        .map(C23AttributeItem::Deprecated);

    let nodiscard = just("nodiscard")
        .padded()
        .ignore_then(opt_string_arg())
        .map(C23AttributeItem::Nodiscard);

    let maybe_unused = just("maybe_unused")
        .padded()
        .to(C23AttributeItem::MaybeUnused);

    let noreturn = just("noreturn").padded().to(C23AttributeItem::Noreturn);

    // clang-namespaced WASM linkage attributes
    let import_module = just("clang::")
        .padded()
        .ignore_then(just("import_module").padded())
        .ignore_then(just('(').padded())
        .ignore_then(string_literal())
        .then_ignore(just(')').padded())
        .map(C23AttributeItem::ImportModule);

    let import_name = just("clang::")
        .padded()
        .ignore_then(just("import_name").padded())
        .ignore_then(just('(').padded())
        .ignore_then(string_literal())
        .then_ignore(just(')').padded())
        .map(C23AttributeItem::ImportName);

    let export_name = just("clang::")
        .padded()
        .ignore_then(just("export_name").padded())
        .ignore_then(just('(').padded())
        .ignore_then(string_literal())
        .then_ignore(just(')').padded())
        .map(C23AttributeItem::ExportName);

    choice((
        deprecated,
        nodiscard,
        maybe_unused,
        noreturn,
        import_module,
        import_name,
        export_name,
    ))
}

// Parser for C23 attribute specifier: [[attr1, attr2, ...]]
fn c23_attribute_specifier<'src>()
-> impl Parser<'src, &'src str, C23Attributes, extra::Err<Rich<'src, char>>> {
    just("[[")
        .padded()
        .ignore_then(
            c23_attribute_item()
                .separated_by(just(',').padded())
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just("]]").padded())
        .map(|items| {
            let mut result = C23Attributes::default();
            for item in items {
                match item {
                    C23AttributeItem::Deprecated(msg) => result.deprecated = Some(msg),
                    C23AttributeItem::Nodiscard(reason) => result.nodiscard = Some(reason),
                    C23AttributeItem::MaybeUnused => result.maybe_unused = Some(()),
                    C23AttributeItem::Noreturn => result.noreturn = Some(()),
                    C23AttributeItem::ImportModule(m) => result.import_module = Some(m),
                    C23AttributeItem::ImportName(n) => result.import_name = Some(n),
                    C23AttributeItem::ExportName(n) => result.export_name = Some(n),
                }
            }
            result
        })
}

// Parser for optional C23 attributes (can have multiple attribute specifiers)
fn opt_c23_attributes<'src>()
-> impl Parser<'src, &'src str, Option<C23Attributes>, extra::Err<Rich<'src, char>>> {
    c23_attribute_specifier()
        .repeated()
        .collect::<Vec<_>>()
        .map(|attrs| {
            if attrs.is_empty() {
                return None;
            }
            let mut result = C23Attributes::default();
            for attr in attrs {
                if attr.deprecated.is_some() {
                    result.deprecated = attr.deprecated;
                }
                if attr.nodiscard.is_some() {
                    result.nodiscard = attr.nodiscard;
                }
                if attr.maybe_unused.is_some() {
                    result.maybe_unused = Some(());
                }
                if attr.noreturn.is_some() {
                    result.noreturn = Some(());
                }
                if attr.import_module.is_some() {
                    result.import_module = attr.import_module;
                }
                if attr.import_name.is_some() {
                    result.import_name = attr.import_name;
                }
                if attr.export_name.is_some() {
                    result.export_name = attr.export_name;
                }
            }
            Some(result)
        })
}

// Parser for C identifiers
fn ident<'src>() -> impl Parser<'src, &'src str, String, extra::Err<Rich<'src, char>>> {
    text::ascii::ident().map(|s: &str| s.to_string()).padded()
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
            .ignore_then(ident())
            .map(Type::Struct),
        just("enum").padded().ignore_then(ident()).map(Type::Enum),
        ident().map(Type::Named),
    ))
    .padded()
}

// Parser for types with pointers (and arrays handled in fields/params because arrays are defined on the identifier `int x[5];`)
// Actually, C syntax puts array brackets on the variable name, not the type name natively (mostly).
// Wait, `type_parser` doesn't handle array brackets next to type name right now.
// I'll adjust the `field` and `parameter` parsers to look for `[N]` after the identifier.
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

// Parser for struct fields
fn field<'src>() -> impl Parser<'src, &'src str, Field, extra::Err<Rich<'src, char>>> {
    opt_comment()
        .then(type_parser())
        .then(ident())
        .then(array_brackets().or_not())
        .then_ignore(just(';').padded())
        .map(|(((comment, mut field_type), name), array_size)| {
            if let Some(size) = array_size {
                field_type = Type::Array(Box::new(field_type), size);
            }
            Field {
                name,
                field_type,
                comment,
            }
        })
}

// Parser for struct declarations
fn struct_decl<'src>() -> impl Parser<'src, &'src str, StructDecl, extra::Err<Rich<'src, char>>> {
    opt_comment()
        .then(just("struct").padded().ignore_then(ident()))
        .then_ignore(just('{').padded())
        .then(field().repeated().collect::<Vec<_>>())
        .then_ignore(just('}').padded())
        .then_ignore(just(';').padded())
        .map(|((comment, name), fields)| StructDecl {
            name,
            fields,
            comment,
        })
}

// Parser for function parameters
fn parameter<'src>() -> impl Parser<'src, &'src str, Parameter, extra::Err<Rich<'src, char>>> {
    type_parser()
        .then(ident().or_not())
        .then(array_brackets().or_not())
        .map(|((mut param_type, name), array_size)| {
            if let Some(size) = array_size {
                param_type = Type::Array(Box::new(param_type), size);
            }
            Parameter { name, param_type }
        })
}

// Parser for function declarations
fn function_decl<'src>() -> impl Parser<'src, &'src str, FunctionDecl, extra::Err<Rich<'src, char>>>
{
    opt_comment()
        .then(opt_c23_attributes())
        .then(type_parser())
        .then(ident())
        .then_ignore(just('(').padded())
        .then(
            parameter()
                .separated_by(just(',').padded())
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(')').padded())
        .then_ignore(just(';').padded())
        .map(|((((comment, c23_attributes), return_type), name), parameters)| FunctionDecl {
            name,
            return_type,
            parameters,
            comment,
            c23_attributes,
        })
}

// Parser for variable declarations
fn variable_decl<'src>() -> impl Parser<'src, &'src str, VariableDecl, extra::Err<Rich<'src, char>>>
{
    opt_comment()
        .then(type_parser())
        .then(ident())
        .then(array_brackets().or_not())
        .then_ignore(just(';').padded())
        .map(|(((comment, mut var_type), name), array_size)| {
            if let Some(size) = array_size {
                var_type = Type::Array(Box::new(var_type), size);
            }
            VariableDecl {
                name,
                var_type,
                comment,
            }
        })
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
}

// Parser for enum declarations
fn enum_decl<'src>() -> impl Parser<'src, &'src str, EnumDecl, extra::Err<Rich<'src, char>>> {
    opt_comment()
        .then(just("enum").padded().ignore_then(ident()))
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

/// Parse C declarations from a string
pub fn parse_declarations(input: &str) -> Result<Declarations, Vec<Rich<'_, char>>> {
    let struct_parser = struct_decl().map(|s| (Some(s), None, None, None, None));
    let enum_parser = enum_decl().map(|e| (None, None, None, Some(e), None));
    let directive_parser = directive_decl().map(|d| (None, None, None, None, Some(d)));
    let function_parser = function_decl().map(|f| (None, Some(f), None, None, None));
    let variable_parser = variable_decl().map(|v| (None, None, Some(v), None, None));

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
    for (s, f, v, e, d) in declarations {
        if let Some(s) = s {
            result.structs.push(s);
        }
        if let Some(f) = f {
            result.functions.push(f);
        }
        if let Some(v) = v {
            result.variables.push(v);
        }
        if let Some(e) = e {
            result.enums.push(e);
        }
        if let Some(d) = d {
            result.directives.push(d);
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_struct() {
        let input = r#"
            struct Point {
                int x;
                int y;
            };
        "#;

        let result = parse_declarations(input).unwrap();
        assert_eq!(result.structs.len(), 1);
        assert_eq!(result.functions.len(), 0);
        assert_eq!(result.variables.len(), 0);

        let s = &result.structs[0];
        assert_eq!(s.name, "Point");
        assert_eq!(s.fields.len(), 2);
        assert_eq!(s.fields[0].name, "x");
        assert_eq!(s.fields[0].field_type, Type::Int);
        assert_eq!(s.fields[1].name, "y");
        assert_eq!(s.fields[1].field_type, Type::Int);
    }

    #[test]
    fn test_parse_function() {
        let input = "int add(int a, int b);";
        let result = parse_declarations(input).unwrap();
        assert_eq!(result.structs.len(), 0);
        assert_eq!(result.functions.len(), 1);
        assert_eq!(result.variables.len(), 0);

        let f = &result.functions[0];
        assert_eq!(f.name, "add");
        assert_eq!(f.return_type, Type::Int);
        assert_eq!(f.parameters.len(), 2);
        assert_eq!(f.parameters[0].name, Some("a".to_string()));
        assert_eq!(f.parameters[0].param_type, Type::Int);
        assert_eq!(f.parameters[1].name, Some("b".to_string()));
        assert_eq!(f.parameters[1].param_type, Type::Int);
    }

    #[test]
    fn test_parse_variable() {
        let input = "unsigned int counter;";
        let result = parse_declarations(input).unwrap();
        assert_eq!(result.structs.len(), 0);
        assert_eq!(result.functions.len(), 0);
        assert_eq!(result.variables.len(), 1);

        let v = &result.variables[0];
        assert_eq!(v.name, "counter");
        assert_eq!(v.var_type, Type::UnsignedInt);
    }

    #[test]
    fn test_parse_pointer_type() {
        let input = "void* ptr;";
        let result = parse_declarations(input).unwrap();
        assert_eq!(result.variables.len(), 1);

        let v = &result.variables[0];
        assert_eq!(v.name, "ptr");
        assert_eq!(v.var_type, Type::Pointer(Box::new(Type::Void)));
    }

    #[test]
    fn test_parse_multiple_pointers() {
        let input = "int** double_ptr;";
        let result = parse_declarations(input).unwrap();
        assert_eq!(result.variables.len(), 1);

        let v = &result.variables[0];
        assert_eq!(v.name, "double_ptr");
        assert_eq!(
            v.var_type,
            Type::Pointer(Box::new(Type::Pointer(Box::new(Type::Int))))
        );
    }

    #[test]
    fn test_parse_struct_type() {
        let input = "struct Node node;";
        let result = parse_declarations(input).unwrap();
        assert_eq!(result.variables.len(), 1);

        let v = &result.variables[0];
        assert_eq!(v.name, "node");
        assert_eq!(v.var_type, Type::Struct("Node".to_string()));
    }

    #[test]
    fn test_parse_function_no_params() {
        let input = "void exit();";
        let result = parse_declarations(input).unwrap();
        assert_eq!(result.functions.len(), 1);

        let f = &result.functions[0];
        assert_eq!(f.name, "exit");
        assert_eq!(f.return_type, Type::Void);
        assert_eq!(f.parameters.len(), 0);
    }

    #[test]
    fn test_parse_multiple_declarations() {
        let input = r#"
            struct Point {
                int x;
                int y;
            };
            
            int add(int a, int b);
            unsigned int counter;
            void* get_pointer();
        "#;

        let result = parse_declarations(input).unwrap();
        assert_eq!(result.structs.len(), 1);
        assert_eq!(result.functions.len(), 2);
        assert_eq!(result.variables.len(), 1);

        assert_eq!(result.structs[0].name, "Point");
        assert_eq!(result.functions[0].name, "add");
        assert_eq!(result.variables[0].name, "counter");
        assert_eq!(result.functions[1].name, "get_pointer");
    }

    #[test]
    fn test_parse_anonymous_parameters() {
        let input = "int process(int, char*);";
        let result = parse_declarations(input).unwrap();
        assert_eq!(result.functions.len(), 1);

        let f = &result.functions[0];
        assert_eq!(f.name, "process");
        assert_eq!(f.return_type, Type::Int);
        assert_eq!(f.parameters.len(), 2);
        assert_eq!(f.parameters[0].name, None);
        assert_eq!(f.parameters[0].param_type, Type::Int);
        assert_eq!(f.parameters[1].name, None);
        assert_eq!(
            f.parameters[1].param_type,
            Type::Pointer(Box::new(Type::Char))
        );
    }

    #[test]
    fn test_parse_with_line_comments() {
        let input = r#"
            // This is a point structure
            struct Point {
                // X coordinate
                int x;
                // Y coordinate
                int y;
            };
            
            // Add two numbers
            int add(int a, int b);
            
            // Global counter
            unsigned int counter;
        "#;

        let result = parse_declarations(input).unwrap();
        assert_eq!(result.structs.len(), 1);
        assert_eq!(result.functions.len(), 1);
        assert_eq!(result.variables.len(), 1);

        let s = &result.structs[0];
        assert_eq!(s.name, "Point");
        assert_eq!(s.fields[0].comment, vec!["X coordinate".to_string()]);
        assert_eq!(s.fields[1].comment, vec!["Y coordinate".to_string()]);

        let f = &result.functions[0];
        assert_eq!(f.name, "add");
        assert_eq!(f.comment, vec!["Add two numbers".to_string()]);

        let v = &result.variables[0];
        assert_eq!(v.name, "counter");
        assert_eq!(v.comment, vec!["Global counter".to_string()]);
    }

    #[test]
    fn test_parse_with_block_comments() {
        let input = r#"
            /* This function calculates sum */
            int sum(int x, int y);
            
            /* A field with description */
            struct Data {
                /* The value */
                int value;
            };
        "#;

        let result = parse_declarations(input).unwrap();
        assert_eq!(result.functions.len(), 1);
        assert_eq!(result.structs.len(), 1);

        let f = &result.functions[0];
        assert_eq!(f.comment, vec!["This function calculates sum".to_string()]);

        let s = &result.structs[0];
        assert_eq!(s.fields[0].comment, vec!["The value".to_string()]);
    }

    #[test]
    fn test_parse_multiple_comments_on_same_declaration() {
        let input = r#"
            // First comment
            // Second comment
            /* Third comment */
            int func();
            
            // Comment one
            /* Comment two */
            struct Test {
                // Field comment 1
                // Field comment 2
                int value;
            };
            
            /* Variable comment 1 */
            // Variable comment 2
            int var;
        "#;

        let result = parse_declarations(input).unwrap();

        let f = &result.functions[0];
        assert_eq!(f.comment.len(), 3);
        assert_eq!(f.comment[0], "First comment");
        assert_eq!(f.comment[1], "Second comment");
        assert_eq!(f.comment[2], "Third comment");

        let s = &result.structs[0];
        assert_eq!(s.comment.len(), 2);
        assert_eq!(s.comment[0], "Comment one");
        assert_eq!(s.comment[1], "Comment two");
        assert_eq!(s.fields[0].comment.len(), 2);
        assert_eq!(s.fields[0].comment[0], "Field comment 1");
        assert_eq!(s.fields[0].comment[1], "Field comment 2");

        let v = &result.variables[0];
        assert_eq!(v.comment.len(), 2);
        assert_eq!(v.comment[0], "Variable comment 1");
        assert_eq!(v.comment[1], "Variable comment 2");
    }

    #[test]
    fn test_parse_declarations_without_comments() {
        let input = r#"
            struct Empty {
                int field;
            };
            
            void no_comment();
            
            int no_comment_var;
        "#;

        let result = parse_declarations(input).unwrap();

        assert_eq!(result.structs[0].comment.len(), 0);
        assert_eq!(result.structs[0].fields[0].comment.len(), 0);
        assert_eq!(result.functions[0].comment.len(), 0);
        assert_eq!(result.variables[0].comment.len(), 0);
    }

    #[test]
    fn test_parse_mixed_comments_and_no_comments() {
        let input = r#"
            // Commented struct
            struct A {
                // Commented field
                int x;
                int y;
            };
            
            void uncommented_func();
            
            // Commented variable
            int var;
        "#;

        let result = parse_declarations(input).unwrap();

        let s = &result.structs[0];
        assert_eq!(s.comment, vec!["Commented struct".to_string()]);
        assert_eq!(s.fields[0].comment, vec!["Commented field".to_string()]);
        assert_eq!(s.fields[1].comment.len(), 0);

        let f = &result.functions[0];
        assert_eq!(f.comment.len(), 0);

        let v = &result.variables[0];
        assert_eq!(v.comment, vec!["Commented variable".to_string()]);
    }

    #[test]
    fn test_parse_struct_with_all_fields_commented() {
        let input = r#"
            // Main structure
            struct AllCommented {
                // First field
                int a;
                /* Second field */
                int b;
                // Third field
                // With multiple lines
                char* c;
            };
        "#;

        let result = parse_declarations(input).unwrap();
        let s = &result.structs[0];

        assert_eq!(s.comment, vec!["Main structure".to_string()]);
        assert_eq!(s.fields.len(), 3);

        assert_eq!(s.fields[0].comment, vec!["First field".to_string()]);
        assert_eq!(s.fields[1].comment, vec!["Second field".to_string()]);
        assert_eq!(s.fields[2].comment.len(), 2);
        assert_eq!(s.fields[2].comment[0], "Third field");
        assert_eq!(s.fields[2].comment[1], "With multiple lines");
    }

    #[test]
    fn test_parse_multiline_block_comment() {
        let input = r#"
            /*
             * This is a multiline
             * block comment for
             * a function
             */
            void documented();
        "#;

        let result = parse_declarations(input).unwrap();
        let f = &result.functions[0];

        assert_eq!(f.comment.len(), 1);
        assert!(f.comment[0].contains("multiline"));
        assert!(f.comment[0].contains("block comment"));
    }

    #[test]
    fn test_parse_empty_comments() {
        let input = r#"
            //
            /* */
            int func();
        "#;

        let result = parse_declarations(input).unwrap();
        let f = &result.functions[0];

        assert_eq!(f.comment.len(), 2);
        assert_eq!(f.comment[0], "");
        assert_eq!(f.comment[1], "");
    }

    #[test]
    fn test_parse_function_with_import_attribute() {
        let input = r#"
            [[clang::import_module("math"), clang::import_name("add")]] int add(int a, int b);
        "#;

        let result = parse_declarations(input).unwrap();
        assert_eq!(result.functions.len(), 1);

        let f = &result.functions[0];
        assert_eq!(f.name, "add");
        assert!(f.c23_attributes.is_some());

        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.import_module, Some("math".to_string()));
        assert_eq!(attrs.import_name, Some("add".to_string()));
    }

    #[test]
    fn test_parse_function_with_only_import_module() {
        let input = r#"
            [[clang::import_module("env")]] void log(char* msg);
        "#;

        let result = parse_declarations(input).unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.import_module, Some("env".to_string()));
        assert_eq!(attrs.import_name, None);
    }

    #[test]
    fn test_parse_function_with_only_import_name() {
        let input = r#"
            [[clang::import_name("print_fn")]] void print();
        "#;

        let result = parse_declarations(input).unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.import_module, None);
        assert_eq!(attrs.import_name, Some("print_fn".to_string()));
    }

    #[test]
    fn test_parse_function_with_export_name() {
        let input = r#"
            [[clang::export_name("guest_run")]] void run();
        "#;

        let result = parse_declarations(input).unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.export_name, Some("guest_run".to_string()));
        assert_eq!(attrs.import_module, None);
        assert_eq!(attrs.import_name, None);
    }

    #[test]
    fn test_parse_function_without_attribute() {
        let input = "int add(int a, int b);";

        let result = parse_declarations(input).unwrap();
        let f = &result.functions[0];
        assert!(f.c23_attributes.is_none());
    }

    #[test]
    fn test_parse_mixed_with_and_without_attributes() {
        let input = r#"
            [[clang::import_module("mod1")]] int func1();
            int func2();
            [[clang::import_name("func_three")]] int func3();
        "#;

        let result = parse_declarations(input).unwrap();
        assert_eq!(result.functions.len(), 3);

        assert!(result.functions[0].c23_attributes.is_some());
        assert!(result.functions[1].c23_attributes.is_none());
        assert!(result.functions[2].c23_attributes.is_some());
    }

    #[test]
    fn test_parse_c23_deprecated_attribute() {
        let input = r#"
            [[deprecated]] int old_func();
        "#;

        let result = parse_declarations(input).unwrap();
        assert_eq!(result.functions.len(), 1);

        let f = &result.functions[0];
        assert_eq!(f.name, "old_func");
        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.deprecated, Some(None));
        assert_eq!(attrs.nodiscard, None);
        assert_eq!(attrs.maybe_unused, None);
        assert_eq!(attrs.noreturn, None);
    }

    #[test]
    fn test_parse_c23_deprecated_with_message() {
        let input = r#"
            [[deprecated("Use new_func instead")]] int old_func();
        "#;

        let result = parse_declarations(input).unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        assert_eq!(
            f.c23_attributes.as_ref().unwrap().deprecated,
            Some(Some("Use new_func instead".to_string()))
        );
    }

    #[test]
    fn test_parse_c23_nodiscard_attribute() {
        let input = r#"
            [[nodiscard]] int get_value();
        "#;

        let result = parse_declarations(input).unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.nodiscard, Some(None));
        assert_eq!(attrs.deprecated, None);
    }

    #[test]
    fn test_parse_c23_nodiscard_with_reason() {
        let input = r#"
            [[nodiscard("Ignoring return value causes memory leak")]] void* allocate();
        "#;

        let result = parse_declarations(input).unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        assert_eq!(
            f.c23_attributes.as_ref().unwrap().nodiscard,
            Some(Some("Ignoring return value causes memory leak".to_string()))
        );
    }

    #[test]
    fn test_parse_c23_maybe_unused_attribute() {
        let input = r#"
            [[maybe_unused]] int helper();
        "#;

        let result = parse_declarations(input).unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.maybe_unused, Some(()));
        assert_eq!(attrs.deprecated, None);
        assert_eq!(attrs.nodiscard, None);
        assert_eq!(attrs.noreturn, None);
    }

    #[test]
    fn test_parse_c23_noreturn_attribute() {
        let input = r#"
            [[noreturn]] void exit_program();
        "#;

        let result = parse_declarations(input).unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.noreturn, Some(()));
        assert_eq!(attrs.deprecated, None);
        assert_eq!(attrs.nodiscard, None);
        assert_eq!(attrs.maybe_unused, None);
    }

    #[test]
    fn test_parse_c23_multiple_attributes_in_one_specifier() {
        let input = r#"
            [[deprecated, nodiscard]] int func();
        "#;

        let result = parse_declarations(input).unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.deprecated, Some(None));
        assert_eq!(attrs.nodiscard, Some(None));
        assert_eq!(attrs.maybe_unused, None);
        assert_eq!(attrs.noreturn, None);
    }

    #[test]
    fn test_parse_c23_multiple_attribute_specifiers() {
        let input = r#"
            [[deprecated]] [[nodiscard]] int func();
        "#;

        let result = parse_declarations(input).unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.deprecated, Some(None));
        assert_eq!(attrs.nodiscard, Some(None));
        assert_eq!(attrs.maybe_unused, None);
        assert_eq!(attrs.noreturn, None);
    }

    #[test]
    fn test_parse_c23_attributes_with_comments() {
        let input = r#"
            // This function is deprecated
            [[deprecated("Use v2")]] int func_v1();
        "#;

        let result = parse_declarations(input).unwrap();
        let f = &result.functions[0];

        assert_eq!(f.comment, vec!["This function is deprecated".to_string()]);
        assert!(f.c23_attributes.is_some());
        assert_eq!(
            f.c23_attributes.as_ref().unwrap().deprecated,
            Some(Some("Use v2".to_string()))
        );
    }

    #[test]
    fn test_parse_c23_attributes_with_wasm_linkage() {
        let input = r#"
            [[nodiscard, clang::import_module("mod"), clang::import_name("get")]] int get();
        "#;

        let result = parse_declarations(input).unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.nodiscard, Some(None));
        assert_eq!(attrs.import_module, Some("mod".to_string()));
        assert_eq!(attrs.import_name, Some("get".to_string()));
    }
}
