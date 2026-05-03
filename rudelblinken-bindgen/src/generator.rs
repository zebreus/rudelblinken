use crate::Span;
/// Language-neutral data model for code generation.
///
/// This module's types are intentionally separate from the parser types, even
/// where they currently look identical. The parser models *C syntax*; these
/// types model the *generator IR*, which will be consumed by multiple backends
/// (C headers, Rust bindings, …) and will diverge from the C AST as backends
/// grow their own requirements.
///
/// The [`Declarations::lower`] function below is the seam: it validates the C AST
/// produced by the parser and translates it into this IR. All attribute-flattening
/// and linkage resolution happens here — imported in the parser, resolved at the
/// seam, invisible to backends.
use crate::parser;
use std::collections::HashSet;

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
    pub fn lower(decls: parser::Declarations) -> Result<Self, Vec<LoweringError>> {
        let mut errors = Vec::new();
        let mut ordinary_names = HashSet::new();

        for struct_decl in &decls.structs {
            for field in &struct_decl.fields {
                validate_object_type(
                    &field.field_type,
                    &format!("field `{}`", field.name),
                    &struct_decl.span,
                    &mut errors,
                );
                validate_type(&field.field_type, &struct_decl.span, &mut errors);
            }
        }

        for function in &decls.functions {
            validate_unique_ordinary_name(
                &mut ordinary_names,
                &function.name,
                &function.span,
                &mut errors,
            );
            if let Some(attrs) = &function.c23_attributes {
                validate_duplicate_attributes(
                    "function",
                    &function.name,
                    attrs,
                    &function.span,
                    &mut errors,
                );
                if attrs.export_name.is_some()
                    && (attrs.import_module.is_some() || attrs.import_name.is_some())
                {
                    errors.push(LoweringError::at(
                        format!(
                            "function `{}` cannot be both a host import and a guest export",
                            function.name
                        ),
                        function.span.clone(),
                    ));
                }
            }
            validate_type(&function.return_type, &function.span, &mut errors);
            if !is_void_parameter_list(&function.parameters) {
                for parameter in &function.parameters {
                    validate_object_type(
                        &parameter.param_type,
                        &format!(
                            "parameter `{}`",
                            parameter.name.as_deref().unwrap_or("<anonymous>")
                        ),
                        &function.span,
                        &mut errors,
                    );
                    validate_type(&parameter.param_type, &function.span, &mut errors);
                }
            }
        }

        for variable in &decls.variables {
            validate_unique_ordinary_name(
                &mut ordinary_names,
                &variable.name,
                &variable.span,
                &mut errors,
            );
            if let Some(attrs) = &variable.c23_attributes {
                validate_duplicate_attributes(
                    "variable",
                    &variable.name,
                    attrs,
                    &variable.span,
                    &mut errors,
                );
                if has_linkage_attributes(attrs) {
                    errors.push(LoweringError::at(
                        format!(
                            "variable `{}` cannot use Host/Guest Linkage attributes",
                            variable.name
                        ),
                        variable.span.clone(),
                    ));
                }
            }
            validate_object_type(
                &variable.var_type,
                &format!("variable `{}`", variable.name),
                &variable.span,
                &mut errors,
            );
            validate_type(&variable.var_type, &variable.span, &mut errors);
        }

        for enum_decl in &decls.enums {
            for variant in &enum_decl.variants {
                validate_unique_ordinary_name(
                    &mut ordinary_names,
                    &variant.name,
                    &enum_decl.span,
                    &mut errors,
                );
                if let Some(value) = variant.value {
                    if value < i32::MIN as i64 || value > i32::MAX as i64 {
                        errors.push(LoweringError::at(
                            format!(
                                "enum value `{}` is outside the supported i32 range",
                                variant.name
                            ),
                            enum_decl.span.clone(),
                        ));
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(lower_declarations(decls))
        } else {
            Err(errors)
        }
    }
}

fn validate_duplicate_attributes(
    kind: &str,
    name: &str,
    attrs: &parser::C23Attributes,
    span: &Span,
    errors: &mut Vec<LoweringError>,
) {
    for attr in &attrs.duplicate_attributes {
        errors.push(LoweringError::at(
            format!("{} `{}` repeats attribute `{}`", kind, name, attr),
            span.clone(),
        ));
    }
}

fn has_linkage_attributes(attrs: &parser::C23Attributes) -> bool {
    attrs.import_module.is_some() || attrs.import_name.is_some() || attrs.export_name.is_some()
}

fn validate_unique_ordinary_name(
    ordinary_names: &mut HashSet<String>,
    name: &str,
    span: &Span,
    errors: &mut Vec<LoweringError>,
) {
    if !ordinary_names.insert(name.to_string()) {
        errors.push(LoweringError::at(
            format!(
                "declaration `{}` conflicts with an earlier declaration",
                name
            ),
            span.clone(),
        ));
    }
}

fn validate_type(type_decl: &parser::Type, span: &Span, errors: &mut Vec<LoweringError>) {
    match type_decl {
        parser::Type::Named(name) => errors.push(LoweringError::at(
            format!("unsupported named type `{}`", name),
            span.clone(),
        )),
        parser::Type::Pointer(inner) | parser::Type::Array(inner, _) => {
            validate_type(inner, span, errors)
        }
        parser::Type::Void
        | parser::Type::Int
        | parser::Type::UnsignedInt
        | parser::Type::Char
        | parser::Type::UnsignedChar
        | parser::Type::LongLong
        | parser::Type::UnsignedLongLong
        | parser::Type::Struct(_)
        | parser::Type::Enum(_) => {}
    }
}

fn validate_object_type(
    type_decl: &parser::Type,
    subject: &str,
    span: &Span,
    errors: &mut Vec<LoweringError>,
) {
    if matches!(type_decl, parser::Type::Void) {
        errors.push(LoweringError::at(
            format!("{} cannot have type void", subject),
            span.clone(),
        ));
    }
}

fn is_void_parameter_list(parameters: &[parser::Parameter]) -> bool {
    matches!(
        parameters,
        [parser::Parameter {
            name: None,
            param_type: parser::Type::Void,
        }]
    )
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
