use super::{
    Declarations, Directive, Enum, EnumVariant, Field, Function, Linkage, Parameter, Struct, Type,
    Variable,
};
use crate::{Span, parser};
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

/// Parser declarations that have passed semantic validation.
///
/// This wrapper is produced by the validation step after parsing. Validation
/// proves the restricted canonical C subset is meaningful for
/// rudelblinken-bindgen before the lowering step constructs backend-facing
/// declarations.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ValidatedDeclarations {
    declarations: parser::Declarations,
}

impl ValidatedDeclarations {
    fn validate(declarations: parser::Declarations) -> Result<Self, Vec<LoweringError>> {
        let mut validator = Validator::default();
        validator.validate(&declarations)?;
        Ok(Self { declarations })
    }

    fn into_inner(self) -> parser::Declarations {
        self.declarations
    }
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

#[derive(Default)]
struct Validator {
    errors: Vec<LoweringError>,
    ordinary_names: HashSet<String>,
}

impl Validator {
    fn validate(&mut self, declarations: &parser::Declarations) -> Result<(), Vec<LoweringError>> {
        self.validate_structs(&declarations.structs);
        self.validate_functions(&declarations.functions);
        self.validate_variables(&declarations.variables);
        self.validate_enums(&declarations.enums);

        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(std::mem::take(&mut self.errors))
        }
    }

    fn validate_structs(&mut self, structs: &[parser::StructDecl]) {
        for struct_decl in structs {
            for field in &struct_decl.fields {
                self.validate_object_type(
                    &field.field_type,
                    &format!("field `{}`", field.name),
                    &struct_decl.span,
                );
                self.validate_type(&field.field_type, &struct_decl.span);
            }
        }
    }

    fn validate_functions(&mut self, functions: &[parser::FunctionDecl]) {
        for function in functions {
            self.validate_unique_ordinary_name(&function.name, &function.span);
            if let Some(attrs) = &function.c23_attributes {
                self.validate_duplicate_attributes(
                    "function",
                    &function.name,
                    attrs,
                    &function.span,
                );
                if attrs.export_name.is_some()
                    && (attrs.import_module.is_some() || attrs.import_name.is_some())
                {
                    self.error(
                        format!(
                            "function `{}` cannot be both a host import and a guest export",
                            function.name
                        ),
                        function.span.clone(),
                    );
                }
            }
            self.validate_type(&function.return_type, &function.span);
            if !is_void_parameter_list(&function.parameters) {
                for parameter in &function.parameters {
                    self.validate_object_type(
                        &parameter.param_type,
                        &format!(
                            "parameter `{}`",
                            parameter.name.as_deref().unwrap_or("<anonymous>")
                        ),
                        &function.span,
                    );
                    self.validate_type(&parameter.param_type, &function.span);
                }
            }
        }
    }

    fn validate_variables(&mut self, variables: &[parser::VariableDecl]) {
        for variable in variables {
            self.validate_unique_ordinary_name(&variable.name, &variable.span);
            if let Some(attrs) = &variable.c23_attributes {
                self.validate_duplicate_attributes(
                    "variable",
                    &variable.name,
                    attrs,
                    &variable.span,
                );
                if has_linkage_attributes(attrs) {
                    self.error(
                        format!(
                            "variable `{}` cannot use Host/Guest Linkage attributes",
                            variable.name
                        ),
                        variable.span.clone(),
                    );
                }
            }
            self.validate_object_type(
                &variable.var_type,
                &format!("variable `{}`", variable.name),
                &variable.span,
            );
            self.validate_type(&variable.var_type, &variable.span);
        }
    }

    fn validate_enums(&mut self, enums: &[parser::EnumDecl]) {
        for enum_decl in enums {
            for variant in &enum_decl.variants {
                self.validate_unique_ordinary_name(&variant.name, &enum_decl.span);
                if let Some(value) = variant.value {
                    if value < i32::MIN as i64 || value > i32::MAX as i64 {
                        self.error(
                            format!(
                                "enum value `{}` is outside the supported i32 range",
                                variant.name
                            ),
                            enum_decl.span.clone(),
                        );
                    }
                }
            }
        }
    }

    fn validate_duplicate_attributes(
        &mut self,
        kind: &str,
        name: &str,
        attrs: &parser::C23Attributes,
        span: &Span,
    ) {
        for attr in &attrs.duplicate_attributes {
            self.error(
                format!("{} `{}` repeats attribute `{}`", kind, name, attr),
                span.clone(),
            );
        }
    }

    fn validate_unique_ordinary_name(&mut self, name: &str, span: &Span) {
        if !self.ordinary_names.insert(name.to_string()) {
            self.error(
                format!(
                    "declaration `{}` conflicts with an earlier declaration",
                    name
                ),
                span.clone(),
            );
        }
    }

    fn validate_type(&mut self, type_decl: &parser::Type, span: &Span) {
        match type_decl {
            parser::Type::Named(name) => {
                self.error(format!("unsupported named type `{}`", name), span.clone())
            }
            parser::Type::Pointer(inner) | parser::Type::Array(inner, _) => {
                self.validate_type(inner, span)
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

    fn validate_object_type(&mut self, type_decl: &parser::Type, subject: &str, span: &Span) {
        if matches!(type_decl, parser::Type::Void) {
            self.error(format!("{} cannot have type void", subject), span.clone());
        }
    }

    fn error(&mut self, message: String, span: Span) {
        self.errors.push(LoweringError::at(message, span));
    }
}

fn has_linkage_attributes(attrs: &parser::C23Attributes) -> bool {
    attrs.import_module.is_some() || attrs.import_name.is_some() || attrs.export_name.is_some()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(input: &str) -> Result<ValidatedDeclarations, Vec<LoweringError>> {
        let declarations = parser::parse_declarations(input, "<test>").expect("input should parse");
        ValidatedDeclarations::validate(declarations)
    }

    #[test]
    fn rejects_named_types_without_codegen() {
        let errors = validate("rudel_word counter;").expect_err("validation should fail");
        assert!(
            errors
                .iter()
                .any(|err| err.message.contains("unsupported named type `rudel_word`")),
            "errors: {errors:?}"
        );
    }

    #[test]
    fn accepts_void_parameter_list_as_special_case() {
        validate("int main(void);").expect("void parameter list should validate");
    }

    #[test]
    fn rejects_named_void_parameters() {
        let errors = validate("int bad(void value);").expect_err("validation should fail");
        assert!(
            errors.iter().any(|err| err
                .message
                .contains("parameter `value` cannot have type void")),
            "errors: {errors:?}"
        );
    }
}
