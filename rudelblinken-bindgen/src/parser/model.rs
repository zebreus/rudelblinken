use crate::Span;

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
    /// Byte-offset span of the whole declaration in the source file
    pub span: Span,
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
    pub span: Span,
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
    /// Byte-offset span of the whole declaration in the source file
    pub span: Span,
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
    /// Byte-offset span of the whole declaration in the source file
    pub span: Span,
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
    /// C23 attribute specifier sequence `[[...]]` if present
    pub c23_attributes: Option<C23Attributes>,
    /// Byte-offset span of the whole declaration in the source file
    pub span: Span,
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
    /// Attribute names that were repeated in this specifier sequence.
    pub duplicate_attributes: Vec<String>,
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
