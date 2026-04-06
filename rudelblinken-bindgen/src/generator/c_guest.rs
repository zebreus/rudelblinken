//! C header file generation
//!
//! This module generates C header files from the internal representation.

use crate::generator::*;

/// Generate a C header file from declarations
pub fn generate(declarations: &Declarations) -> String {
    let mut output = String::new();

    for directive in &declarations.directives {
        generate_directive(&mut output, directive);
    }

    if !declarations.directives.is_empty()
        && (!declarations.enums.is_empty()
            || !declarations.structs.is_empty()
            || !declarations.functions.is_empty())
    {
        output.push('\n');
    }

    for (i, enum_decl) in declarations.enums.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        generate_enum(&mut output, enum_decl);
    }

    for (i, struct_decl) in declarations.structs.iter().enumerate() {
        if i > 0 || !declarations.enums.is_empty() {
            output.push('\n');
        }
        generate_struct(&mut output, struct_decl);
    }

    for (i, func_decl) in declarations.functions.iter().enumerate() {
        if i > 0 || !declarations.structs.is_empty() || !declarations.enums.is_empty() {
            output.push('\n');
        }
        generate_function(&mut output, func_decl);
    }

    for (i, var_decl) in declarations.variables.iter().enumerate() {
        if i > 0
            || !declarations.functions.is_empty()
            || !declarations.structs.is_empty()
            || !declarations.enums.is_empty()
        {
            output.push('\n');
        }
        generate_variable(&mut output, var_decl);
    }

    output
}

fn generate_directive(output: &mut String, directive: &Directive) {
    match directive {
        Directive::Pragma(p) => {
            output.push_str("#pragma ");
            output.push_str(p);
            output.push('\n');
        }
        Directive::StaticAssert { expr, message } => {
            output.push_str("_Static_assert(");
            output.push_str(expr);
            output.push_str(", \"");
            output.push_str(message);
            output.push_str("\");\n");
        }
        Directive::Define { name, value } => {
            output.push_str("#define ");
            output.push_str(name);
            output.push(' ');
            output.push_str(value);
            output.push('\n');
        }
    }
}

fn generate_comments(output: &mut String, comments: &[String]) {
    for comment in comments {
        if comment.is_empty() {
            output.push_str("//\n");
        } else if comment.starts_with('/') {
            output.push_str("//");
            output.push_str(comment);
            output.push('\n');
        } else {
            output.push_str("// ");
            output.push_str(comment);
            output.push('\n');
        }
    }
}

fn generate_type_parts(type_decl: &Type) -> (String, String) {
    match type_decl {
        Type::Void => ("void".to_string(), "".to_string()),
        Type::Int => ("int".to_string(), "".to_string()),
        Type::UnsignedInt => ("unsigned int".to_string(), "".to_string()),
        Type::Char => ("char".to_string(), "".to_string()),
        Type::UnsignedChar => ("unsigned char".to_string(), "".to_string()),
        Type::LongLong => ("long long".to_string(), "".to_string()),
        Type::UnsignedLongLong => ("unsigned long long".to_string(), "".to_string()),
        Type::Struct(name) => (format!("struct {}", name), "".to_string()),
        Type::Enum(name) => (format!("enum {}", name), "".to_string()),
        Type::Pointer(inner) => {
            let (prefix, suffix) = generate_type_parts(inner);
            (format!("{}*", prefix), suffix)
        }
        Type::Array(inner, size) => {
            let (prefix, suffix) = generate_type_parts(inner);
            (prefix, format!("{}[{}]", suffix, size))
        }
        Type::Named(name) => (name.clone(), "".to_string()),
    }
}

fn generate_decl_string(type_decl: &Type, name: &str) -> String {
    let (prefix, suffix) = generate_type_parts(type_decl);
    if name.is_empty() && suffix.is_empty() {
        prefix
    } else if suffix.is_empty() {
        format!("{} {}", prefix, name)
    } else if name.is_empty() {
        format!("{}{}", prefix, suffix)
    } else {
        if suffix.starts_with('[') {
            format!("{} {}{}", prefix, name, suffix)
        } else {
            format!("{} {}{}", prefix, name, suffix)
        }
    }
}

fn generate_gnu_attribute(
    output: &mut String,
    import_module: &Option<String>,
    import_name: &Option<String>,
) {
    let mut attr_parts = Vec::new();

    if let Some(module) = import_module {
        attr_parts.push(format!("import_module(\"{}\")", module));
    }
    if let Some(name) = import_name {
        attr_parts.push(format!("import_name(\"{}\")", name));
    }

    if !attr_parts.is_empty() {
        output.push_str(" __attribute__((");
        output.push_str(&attr_parts.join(", "));
        output.push_str("))");
    }
}

fn generate_c23_attributes(
    output: &mut String,
    deprecated: &Option<Option<String>>,
    nodiscard: &Option<Option<String>>,
    maybe_unused: &Option<()>,
    noreturn: &Option<()>,
) {
    let mut attr_parts = Vec::new();

    if let Some(msg) = deprecated {
        if let Some(text) = msg {
            attr_parts.push(format!("deprecated(\"{}\")", text));
        } else {
            attr_parts.push("deprecated".to_string());
        }
    }

    if let Some(reason) = nodiscard {
        if let Some(text) = reason {
            attr_parts.push(format!("nodiscard(\"{}\")", text));
        } else {
            attr_parts.push("nodiscard".to_string());
        }
    }

    if maybe_unused.is_some() {
        attr_parts.push("maybe_unused".to_string());
    }

    if noreturn.is_some() {
        attr_parts.push("noreturn".to_string());
    }

    if !attr_parts.is_empty() {
        output.push_str("[[");
        output.push_str(&attr_parts.join(", "));
        output.push_str("]] ");
    }
}

fn has_gnu_attributes(import_module: &Option<String>, import_name: &Option<String>) -> bool {
    import_module.is_some() || import_name.is_some()
}

fn has_c23_attributes(
    deprecated: &Option<Option<String>>,
    nodiscard: &Option<Option<String>>,
    maybe_unused: &Option<()>,
    noreturn: &Option<()>,
) -> bool {
    deprecated.is_some() || nodiscard.is_some() || maybe_unused.is_some() || noreturn.is_some()
}

fn generate_struct(output: &mut String, struct_decl: &Struct) {
    generate_comments(output, &struct_decl.comment);

    output.push_str("struct ");
    output.push_str(&struct_decl.name);
    output.push_str(" {\n");

    for field in &struct_decl.fields {
        generate_comments(output, &field.comment);
        output.push_str("    ");
        output.push_str(&generate_decl_string(&field.field_type, &field.name));
        output.push_str(";\n");
    }

    output.push_str("};\n");
}

fn generate_function(output: &mut String, func_decl: &Function) {
    generate_comments(output, &func_decl.comment);

    if has_c23_attributes(
        &func_decl.deprecated,
        &func_decl.nodiscard,
        &func_decl.maybe_unused,
        &func_decl.noreturn,
    ) {
        generate_c23_attributes(
            output,
            &func_decl.deprecated,
            &func_decl.nodiscard,
            &func_decl.maybe_unused,
            &func_decl.noreturn,
        );
    }

    output.push_str(&generate_decl_string(
        &func_decl.return_type,
        &func_decl.name,
    ));
    output.push('(');

    for (i, param) in func_decl.parameters.iter().enumerate() {
        if i > 0 {
            output.push_str(", ");
        }
        if let Some(name) = &param.name {
            output.push_str(&generate_decl_string(&param.param_type, name));
        } else {
            let (prefix, suffix) = generate_type_parts(&param.param_type);
            output.push_str(&prefix);
            if !suffix.is_empty() {
                output.push_str(&suffix);
            }
        }
    }

    output.push(')');

    if has_gnu_attributes(&func_decl.import_module, &func_decl.import_name) {
        generate_gnu_attribute(output, &func_decl.import_module, &func_decl.import_name);
    }

    output.push_str(";\n");
}

fn generate_variable(output: &mut String, var_decl: &Variable) {
    generate_comments(output, &var_decl.comment);

    output.push_str(&generate_decl_string(&var_decl.var_type, &var_decl.name));

    if has_gnu_attributes(&var_decl.import_module, &var_decl.import_name) {
        generate_gnu_attribute(output, &var_decl.import_module, &var_decl.import_name);
    }

    output.push_str(";\n");
}

fn generate_enum(output: &mut String, enum_decl: &Enum) {
    generate_comments(output, &enum_decl.comment);
    output.push_str("enum ");
    output.push_str(&enum_decl.name);
    output.push_str(" {\n");
    for variant in &enum_decl.variants {
        generate_comments(output, &variant.comment);
        output.push_str("    ");
        output.push_str(&variant.name);
        if let Some(val) = variant.value {
            output.push_str(" = ");
            output.push_str(&val.to_string());
        }
        output.push_str(",\n");
    }
    output.push_str("};\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_simple_struct() {
        let decls = Declarations {
            structs: vec![Struct {
                name: "Point".to_string(),
                fields: vec![
                    Field {
                        name: "x".to_string(),
                        field_type: Type::Int,
                        comment: vec![],
                    },
                    Field {
                        name: "y".to_string(),
                        field_type: Type::Int,
                        comment: vec![],
                    },
                ],
                comment: vec![],
            }],
            functions: vec![],
            variables: vec![],
            enums: vec![],
            directives: vec![],
        };

        let result = generate(&decls);
        assert_eq!(result, "struct Point {\n    int x;\n    int y;\n};\n");
    }

    #[test]
    fn test_generate_simple_function() {
        let decls = Declarations {
            structs: vec![],
            functions: vec![Function {
                name: "add".to_string(),
                return_type: Type::Int,
                parameters: vec![
                    Parameter {
                        name: Some("a".to_string()),
                        param_type: Type::Int,
                    },
                    Parameter {
                        name: Some("b".to_string()),
                        param_type: Type::Int,
                    },
                ],
                comment: vec![],
                import_module: None,
                import_name: None,
                deprecated: None,
                nodiscard: None,
                maybe_unused: None,
                noreturn: None,
            }],
            variables: vec![],
            enums: vec![],
            directives: vec![],
        };

        let result = generate(&decls);
        assert_eq!(result, "int add(int a, int b);\n");
    }

    #[test]
    fn test_generate_with_comments() {
        let decls = Declarations {
            structs: vec![],
            functions: vec![Function {
                name: "test".to_string(),
                return_type: Type::Void,
                parameters: vec![],
                comment: vec!["Test function".to_string()],
                import_module: None,
                import_name: None,
                deprecated: None,
                nodiscard: None,
                maybe_unused: None,
                noreturn: None,
            }],
            variables: vec![],
            enums: vec![],
            directives: vec![],
        };

        let result = generate(&decls);
        assert_eq!(result, "// Test function\nvoid test();\n");
    }

    #[test]
    fn test_generate_with_gnu_attribute() {
        let decls = Declarations {
            structs: vec![],
            functions: vec![Function {
                name: "imported".to_string(),
                return_type: Type::Int,
                parameters: vec![],
                comment: vec![],
                import_module: Some("math".to_string()),
                import_name: Some("add".to_string()),
                deprecated: None,
                nodiscard: None,
                maybe_unused: None,
                noreturn: None,
            }],
            variables: vec![],
            enums: vec![],
            directives: vec![],
        };

        let result = generate(&decls);
        assert_eq!(
            result,
            "int imported() __attribute__((import_module(\"math\"), import_name(\"add\")));\n"
        );
    }

    #[test]
    fn test_generate_with_c23_attributes() {
        let decls = Declarations {
            structs: vec![],
            functions: vec![Function {
                name: "old_func".to_string(),
                return_type: Type::Int,
                parameters: vec![],
                comment: vec![],
                import_module: None,
                import_name: None,
                deprecated: Some(None),
                nodiscard: Some(None),
                maybe_unused: None,
                noreturn: None,
            }],
            variables: vec![],
            enums: vec![],
            directives: vec![],
        };

        let result = generate(&decls);
        assert_eq!(result, "[[deprecated, nodiscard]] int old_func();\n");
    }

    #[test]
    fn test_generate_pointer_type() {
        let decls = Declarations {
            structs: vec![],
            functions: vec![],
            variables: vec![Variable {
                name: "ptr".to_string(),
                var_type: Type::Pointer(Box::new(Type::Void)),
                comment: vec![],
                import_module: None,
                import_name: None,
            }],
            enums: vec![],
            directives: vec![],
        };

        let result = generate(&decls);
        assert_eq!(result, "void* ptr;\n");
    }

    #[test]
    fn test_generate_array_type() {
        let decls = Declarations {
            structs: vec![],
            functions: vec![],
            variables: vec![Variable {
                name: "arr".to_string(),
                var_type: Type::Array(Box::new(Type::Int), 16),
                comment: vec![],
                import_module: None,
                import_name: None,
            }],
            enums: vec![],
            directives: vec![],
        };

        let result = generate(&decls);
        assert_eq!(result, "int arr[16];\n");
    }

    #[test]
    fn test_generate_enum_type() {
        let decls = Declarations {
            structs: vec![],
            functions: vec![],
            variables: vec![Variable {
                name: "color".to_string(),
                var_type: Type::Enum("Color".to_string()),
                comment: vec![],
                import_module: None,
                import_name: None,
            }],
            enums: vec![],
            directives: vec![],
        };

        let result = generate(&decls);
        assert_eq!(result, "enum Color color;\n");
    }
}
