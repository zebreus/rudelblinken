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

fn generate_c23_attr_block(output: &mut String, func: &Function) {
    let mut parts: Vec<String> = Vec::new();

    // Standard C23 attributes (no namespace needed)
    if let Some(msg) = &func.deprecated {
        if let Some(text) = msg {
            parts.push(format!("deprecated(\"{}\")", text));
        } else {
            parts.push("deprecated".to_string());
        }
    }
    if let Some(reason) = &func.nodiscard {
        if let Some(text) = reason {
            parts.push(format!("nodiscard(\"{}\")", text));
        } else {
            parts.push("nodiscard".to_string());
        }
    }
    if func.maybe_unused.is_some() {
        parts.push("maybe_unused".to_string());
    }
    if func.noreturn.is_some() {
        parts.push("noreturn".to_string());
    }

    if !parts.is_empty() {
        output.push_str("[[");
        output.push_str(&parts.join(", "));
        output.push_str("]] ");
    }
}

fn generate_gnu_linkage_attribute(output: &mut String, func: &Function) {
    let mut parts: Vec<String> = Vec::new();

    match &func.linkage {
        crate::generator::Linkage::HostImport { module, name } => {
            // Omit import_module when it is the default "env" for readability
            if module != "env" {
                parts.push(format!("import_module(\"{}\")", module));
            }
            parts.push(format!("import_name(\"{}\")", name));
        }
        crate::generator::Linkage::GuestExport { name } => {
            parts.push(format!("export_name(\"{}\")", name));
        }
    }

    if !parts.is_empty() {
        output.push_str(" __attribute__((");
        output.push_str(&parts.join(", "));
        output.push_str("))");
    }
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
    generate_c23_attr_block(output, func_decl);
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
    generate_gnu_linkage_attribute(output, func_decl);
    output.push_str(";\n");
}

fn generate_variable(output: &mut String, var_decl: &Variable) {
    generate_comments(output, &var_decl.comment);
    output.push_str(&generate_decl_string(&var_decl.var_type, &var_decl.name));
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
    use crate::generator::Linkage;

    fn host_import(name: &str) -> Linkage {
        Linkage::HostImport {
            module: "env".to_string(),
            name: name.to_string(),
        }
    }

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
                linkage: host_import("add"),
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
            "int add(int a, int b) __attribute__((import_name(\"add\")));\n"
        );
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
                linkage: host_import("test"),
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
            "// Test function\nvoid test() __attribute__((import_name(\"test\")));\n"
        );
    }

    #[test]
    fn test_generate_with_non_env_module() {
        let decls = Declarations {
            structs: vec![],
            functions: vec![Function {
                name: "imported".to_string(),
                return_type: Type::Int,
                parameters: vec![],
                comment: vec![],
                linkage: Linkage::HostImport {
                    module: "math".to_string(),
                    name: "add".to_string(),
                },
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
    fn test_generate_with_env_module_omitted() {
        // module "env" is omitted in output for readability
        let decls = Declarations {
            structs: vec![],
            functions: vec![Function {
                name: "log".to_string(),
                return_type: Type::Void,
                parameters: vec![],
                comment: vec![],
                linkage: Linkage::HostImport {
                    module: "env".to_string(),
                    name: "log".to_string(),
                },
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
            "void log() __attribute__((import_name(\"log\")));\n"
        );
    }

    #[test]
    fn test_generate_guest_export() {
        let decls = Declarations {
            structs: vec![],
            functions: vec![Function {
                name: "run".to_string(),
                return_type: Type::Void,
                parameters: vec![],
                comment: vec![],
                linkage: Linkage::GuestExport {
                    name: "run".to_string(),
                },
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
            "void run() __attribute__((export_name(\"run\")));\n"
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
                linkage: host_import("old_func"),
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
        assert_eq!(
            result,
            "[[deprecated, nodiscard]] int old_func() __attribute__((import_name(\"old_func\")));\n"
        );
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
            }],
            enums: vec![],
            directives: vec![],
        };

        let result = generate(&decls);
        assert_eq!(result, "enum Color color;\n");
    }
}
