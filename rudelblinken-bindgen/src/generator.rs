/// Language-neutral data model for code generation.
///
/// This module's types are intentionally separate from the parser types, even
/// where they currently look identical. The parser models *C syntax*; these
/// types model the *generator IR*, which will be consumed by multiple backends
/// (C headers, Rust bindings, …) and will diverge from the C AST as backends
/// grow their own requirements.

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

pub mod backends;
mod lowering;
