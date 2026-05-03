/// Language-neutral data model for code generation.
///
/// This module's types are intentionally separate from the parser types, even
/// where they currently look identical. The parser models *C syntax*; these
/// types model the *generator IR*, which will be consumed by multiple backends
/// (C headers, Rust bindings, …) and will diverge from the C AST as backends
/// grow their own requirements.
///
/// Validation is the first internal seam: it proves the C AST produced by the
/// parser has supported rudelblinken-bindgen semantics. The [`Declarations::lower`]
/// function then translates validated parser IR into this generator IR. All
/// attribute-flattening and linkage resolution happens before backends run —
/// imported in the parser, validated and resolved at the seam, invisible to
/// backends.
use self::validation::{is_void_parameter_list, ValidatedDeclarations};
use crate::parser;
use crate::Span;

/// A semantic validation error found while lowering parser IR into generator IR.
#[derive(Clone, Debug, PartialEq)]
pub struct LoweringError {
    pub message: String,
    /// Source span of the offending declaration, if available.
    pub span: Option<Span>,
}

impl LoweringError {
    fn at(message: String, span: Span) -> Self {
        LoweringError {
            message,
            span: Some(span),
        }
    }
}

/// WASM Host/Guest Linkage direction for a function declaration.
///
/// Every function is definitively one of these two at lowering time.
#[derive(Clone, Debug, PartialEq)]
pub enum Linkage {
    /// Function is implemented in the Host and imported by the Guest.
    /// `module` defaults to `"env"` if no `[[clang::import_module(...)]]` was present.
    /// `name` defaults to the C function name if no `[[clang::import_name(...)]]` was present.
    HostImport { module: String, name: String },
    /// Function is implemented in the Guest and exported to the Host.
    /// `name` is the WASM export symbol name from `[[clang::export_name(...)]]`.
    GuestExport { name: String },
}

/// Collection of declarations ready for code generation
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Declarations {
    /// Struct declarations
    pub structs: Vec<Struct>,
    /// Function declarations
    pub functions: Vec<Function>,
    /// Variable declarations
    pub variables: Vec<Variable>,
    /// Enum declarations
    pub enums: Vec<Enum>,
    /// Preprocessor directives
    pub directives: Vec<Directive>,
}

impl Declarations {
    pub(crate) fn validate(
        decls: parser::Declarations,
    ) -> Result<ValidatedDeclarations, Vec<LoweringError>> {
        ValidatedDeclarations::validate(decls)
    }

    pub(crate) fn lower(decls: ValidatedDeclarations) -> Self {
        lower_declarations(decls.into_inner())
    }
}

/// Struct declaration
#[derive(Clone, Debug, PartialEq)]
pub struct Struct {
    /// The name of the struct
    pub name: String,
    /// The fields defined in the struct
    pub fields: Vec<Field>,
    /// Documentation comments
    pub comment: Vec<String>,
}

/// Function declaration
#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    /// The name of the function
    pub name: String,
    /// The return type of the function
    pub return_type: Type,
    /// The parameters of the function
    pub parameters: Vec<Parameter>,
    /// Documentation comments
    pub comment: Vec<String>,
    /// WASM linkage direction, resolved from C23 attributes
    pub linkage: Linkage,
    /// C23 `[[deprecated]]` or `[[deprecated("message")]]`
    pub deprecated: Option<Option<String>>,
    /// C23 `[[nodiscard]]` or `[[nodiscard("reason")]]`
    pub nodiscard: Option<Option<String>>,
    /// C23 `[[maybe_unused]]`
    pub maybe_unused: Option<()>,
    /// C23 `[[noreturn]]`
    pub noreturn: Option<()>,
}

/// Variable declaration
#[derive(Clone, Debug, PartialEq)]
pub struct Variable {
    /// The name of the variable
    pub name: String,
    /// The type of the variable
    pub var_type: Type,
    /// Documentation comments
    pub comment: Vec<String>,
}

/// Enum declaration
#[derive(Clone, Debug, PartialEq)]
pub struct Enum {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub comment: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumVariant {
    pub name: String,
    pub value: Option<i64>,
    pub comment: Vec<String>,
}

/// Preprocessor directives and static asserts
#[derive(Clone, Debug, PartialEq)]
pub enum Directive {
    Pragma(String),
    StaticAssert { expr: String, message: String },
    Define { name: String, value: String },
}

/// A field in a struct
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    /// The name of the field
    pub name: String,
    /// The type of the field
    pub field_type: Type,
    /// Documentation comments
    pub comment: Vec<String>,
}

/// A function parameter
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    /// The name of the parameter (None for anonymous parameters)
    pub name: Option<String>,
    /// The type of the parameter
    pub param_type: Type,
}

/// IR type representation. Currently mirrors the C type set; backends may
/// extend or map this as needed.
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
}

fn lower_declarations(decls: parser::Declarations) -> Declarations {
    Declarations {
        structs: decls.structs.into_iter().map(lower_struct).collect(),
        functions: decls.functions.into_iter().map(lower_function).collect(),
        variables: decls.variables.into_iter().map(lower_variable).collect(),
        enums: decls.enums.into_iter().map(lower_enum).collect(),
        directives: decls.directives.into_iter().map(lower_directive).collect(),
    }
}

fn lower_struct(struct_decl: parser::StructDecl) -> Struct {
    Struct {
        name: struct_decl.name,
        fields: struct_decl.fields.into_iter().map(lower_field).collect(),
        comment: struct_decl.comment,
    }
}

fn lower_function(func: parser::FunctionDecl) -> Function {
    let c23 = func.c23_attributes.unwrap_or_default();
    let linkage = if let Some(export_name) = c23.export_name {
        Linkage::GuestExport { name: export_name }
    } else {
        Linkage::HostImport {
            module: c23.import_module.unwrap_or_else(|| "env".to_string()),
            name: c23.import_name.unwrap_or_else(|| func.name.clone()),
        }
    };
    Function {
        name: func.name,
        return_type: lower_type(func.return_type),
        parameters: if is_void_parameter_list(&func.parameters) {
            Vec::new()
        } else {
            func.parameters.into_iter().map(lower_parameter).collect()
        },
        comment: func.comment,
        linkage,
        deprecated: c23.deprecated,
        nodiscard: c23.nodiscard,
        maybe_unused: c23.maybe_unused,
        noreturn: c23.noreturn,
    }
}

fn lower_variable(var: parser::VariableDecl) -> Variable {
    Variable {
        name: var.name,
        var_type: lower_type(var.var_type),
        comment: var.comment,
    }
}

fn lower_enum(enum_decl: parser::EnumDecl) -> Enum {
    Enum {
        name: enum_decl.name,
        variants: enum_decl
            .variants
            .into_iter()
            .map(lower_enum_variant)
            .collect(),
        comment: enum_decl.comment,
    }
}

fn lower_enum_variant(variant: parser::EnumVariant) -> EnumVariant {
    EnumVariant {
        name: variant.name,
        value: variant.value,
        comment: variant.comment,
    }
}

fn lower_directive(directive: parser::Directive) -> Directive {
    match directive {
        parser::Directive::Pragma(p) => Directive::Pragma(p),
        parser::Directive::StaticAssert { expr, message } => {
            Directive::StaticAssert { expr, message }
        }
        parser::Directive::Define { name, value } => Directive::Define { name, value },
    }
}

fn lower_field(field: parser::Field) -> Field {
    Field {
        name: field.name,
        field_type: lower_type(field.field_type),
        comment: field.comment,
    }
}

fn lower_parameter(param: parser::Parameter) -> Parameter {
    Parameter {
        name: param.name,
        param_type: lower_type(param.param_type),
    }
}

fn lower_type(parser_type: parser::Type) -> Type {
    match parser_type {
        parser::Type::Void => Type::Void,
        parser::Type::Int => Type::Int,
        parser::Type::UnsignedInt => Type::UnsignedInt,
        parser::Type::Char => Type::Char,
        parser::Type::UnsignedChar => Type::UnsignedChar,
        parser::Type::LongLong => Type::LongLong,
        parser::Type::UnsignedLongLong => Type::UnsignedLongLong,
        parser::Type::Struct(name) => Type::Struct(name),
        parser::Type::Enum(name) => Type::Enum(name),
        parser::Type::Pointer(inner) => Type::Pointer(Box::new(lower_type(*inner))),
        parser::Type::Array(inner, size) => Type::Array(Box::new(lower_type(*inner)), size),
        parser::Type::Named(_) => {
            unreachable!("named types are rejected during lowering validation")
        }
    }
}

pub mod c_guest;
pub mod rust_guest;
mod validation;
