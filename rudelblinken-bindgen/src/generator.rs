/// Internal representation for code generation
///
/// This module provides types that are optimized for generating code in various formats.
/// It transforms the parser's AST into a more generation-friendly structure.
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
    /// `struct Name`
    Struct(String),
    /// Pointer to another type: `type*`
    Pointer(Box<Type>),
    /// Named type (typedef or other identifier)
    Named(String),
}

// Conversion from parser types to generator types

impl From<parser::Declarations> for Declarations {
    fn from(parser_decls: parser::Declarations) -> Self {
        Declarations {
            structs: parser_decls.structs.into_iter().map(Into::into).collect(),
            functions: parser_decls.functions.into_iter().map(Into::into).collect(),
            variables: parser_decls.variables.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<parser::StructDecl> for Struct {
    fn from(parser_struct: parser::StructDecl) -> Self {
        Struct {
            name: parser_struct.name,
            fields: parser_struct.fields.into_iter().map(Into::into).collect(),
            comment: parser_struct.comment,
        }
    }
}

impl From<parser::FunctionDecl> for Function {
    fn from(parser_func: parser::FunctionDecl) -> Self {
        let gnu_attr = parser_func.attribute.unwrap_or_default();
        let c23_attr = parser_func.c23_attributes.unwrap_or_default();
        Function {
            name: parser_func.name,
            return_type: parser_func.return_type.into(),
            parameters: parser_func.parameters.into_iter().map(Into::into).collect(),
            comment: parser_func.comment,
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
    fn from(parser_var: parser::VariableDecl) -> Self {
        let gnu_attr = parser_var.attribute.unwrap_or_default();
        Variable {
            name: parser_var.name,
            var_type: parser_var.var_type.into(),
            comment: parser_var.comment,
            import_module: gnu_attr.import_module,
            import_name: gnu_attr.import_name,
        }
    }
}

impl From<parser::Field> for Field {
    fn from(parser_field: parser::Field) -> Self {
        Field {
            name: parser_field.name,
            field_type: parser_field.field_type.into(),
            comment: parser_field.comment,
        }
    }
}

impl From<parser::Parameter> for Parameter {
    fn from(parser_param: parser::Parameter) -> Self {
        Parameter {
            name: parser_param.name,
            param_type: parser_param.param_type.into(),
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
            parser::Type::Struct(name) => Type::Struct(name),
            parser::Type::Pointer(inner) => Type::Pointer(Box::new((*inner).into())),
            parser::Type::Named(name) => Type::Named(name),
        }
    }
}

pub mod c_header;
