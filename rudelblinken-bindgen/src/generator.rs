/// Language-neutral data model for code generation.
///
/// This module's types are intentionally separate from the parser types, even
/// where they currently look identical. The parser models *C syntax*; these
/// types model the *generator IR*, which will be consumed by multiple backends
/// (C headers, Rust bindings, …) and will diverge from the C AST as backends
/// grow their own requirements.
///
/// The [`From`] impls below are the seam: they translate from the C AST produced
/// by the parser into this IR. All attribute-flattening (GNU / C23 → direct
/// fields) happens here.
use crate::parser;

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
    /// GNU-style `__attribute__((import_module("module_name")))`
    pub import_module: Option<String>,
    /// GNU-style `__attribute__((import_name("function_name")))`
    pub import_name: Option<String>,
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
    /// GNU-style `__attribute__((import_module("module_name")))`
    pub import_module: Option<String>,
    /// GNU-style `__attribute__((import_name("function_name")))`
    pub import_name: Option<String>,
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
    /// Named type (typedef or other identifier)
    Named(String),
}

// Conversion from parser types to generator types

impl From<parser::Declarations> for Declarations {
    fn from(decls: parser::Declarations) -> Self {
        Declarations {
            structs: decls.structs.into_iter().map(Into::into).collect(),
            functions: decls.functions.into_iter().map(Into::into).collect(),
            variables: decls.variables.into_iter().map(Into::into).collect(),
            enums: decls.enums.into_iter().map(Into::into).collect(),
            directives: decls.directives.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<parser::StructDecl> for Struct {
    fn from(struct_decl: parser::StructDecl) -> Self {
        Struct {
            name: struct_decl.name,
            fields: struct_decl.fields.into_iter().map(Into::into).collect(),
            comment: struct_decl.comment,
        }
    }
}

impl From<parser::FunctionDecl> for Function {
    fn from(func: parser::FunctionDecl) -> Self {
        let gnu_attr = func.attribute.unwrap_or_default();
        let c23_attr = func.c23_attributes.unwrap_or_default();
        Function {
            name: func.name,
            return_type: func.return_type.into(),
            parameters: func.parameters.into_iter().map(Into::into).collect(),
            comment: func.comment,
            import_module: gnu_attr.import_module,
            import_name: gnu_attr.import_name,
            deprecated: c23_attr.deprecated,
            nodiscard: c23_attr.nodiscard,
            maybe_unused: c23_attr.maybe_unused,
            noreturn: c23_attr.noreturn,
        }
    }
}

impl From<parser::VariableDecl> for Variable {
    fn from(var: parser::VariableDecl) -> Self {
        let gnu_attr = var.attribute.unwrap_or_default();
        Variable {
            name: var.name,
            var_type: var.var_type.into(),
            comment: var.comment,
            import_module: gnu_attr.import_module,
            import_name: gnu_attr.import_name,
        }
    }
}

impl From<parser::EnumDecl> for Enum {
    fn from(enum_decl: parser::EnumDecl) -> Self {
        Enum {
            name: enum_decl.name,
            variants: enum_decl.variants.into_iter().map(Into::into).collect(),
            comment: enum_decl.comment,
        }
    }
}

impl From<parser::EnumVariant> for EnumVariant {
    fn from(variant: parser::EnumVariant) -> Self {
        EnumVariant {
            name: variant.name,
            value: variant.value,
            comment: variant.comment,
        }
    }
}

impl From<parser::Directive> for Directive {
    fn from(directive: parser::Directive) -> Self {
        match directive {
            parser::Directive::Pragma(p) => Directive::Pragma(p),
            parser::Directive::StaticAssert { expr, message } => {
                Directive::StaticAssert { expr, message }
            }
            parser::Directive::Define { name, value } => Directive::Define { name, value },
        }
    }
}

impl From<parser::Field> for Field {
    fn from(field: parser::Field) -> Self {
        Field {
            name: field.name,
            field_type: field.field_type.into(),
            comment: field.comment,
        }
    }
}

impl From<parser::Parameter> for Parameter {
    fn from(param: parser::Parameter) -> Self {
        Parameter {
            name: param.name,
            param_type: param.param_type.into(),
        }
    }
}

impl From<parser::Type> for Type {
    fn from(parser_type: parser::Type) -> Self {
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
            parser::Type::Pointer(inner) => Type::Pointer(Box::new((*inner).into())),
            parser::Type::Array(inner, size) => Type::Array(Box::new((*inner).into()), size),
            parser::Type::Named(name) => Type::Named(name),
        }
    }
}

pub mod c_guest;
pub mod rust_guest;
