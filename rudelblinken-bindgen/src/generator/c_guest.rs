//! C header file generation
//!
//! This module generates C header files from the internal representation.

use crate::generator::*;

fn to_c_parts(ty: &Type) -> (String, String) {
    match ty {
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
            let (prefix, suffix) = to_c_parts(inner);
            (format!("{}*", prefix), suffix)
        }
        Type::Array(inner, size) => {
            let (prefix, suffix) = to_c_parts(inner);
            (prefix, format!("{}[{}]", suffix, size))
        }
    }
}

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
        Directive::Pragma(pragma) => {
            output.push_str("#pragma ");
            output.push_str(pragma);
            output.push('\n');
        }
        Directive::StaticAssert { expr, message } => {
            output.push_str("static_assert(");
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

fn generate_decl_string(type_decl: &Type, name: &str) -> String {
    let (prefix, suffix) = to_c_parts(type_decl);
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

fn write_function_attributes(output: &mut String, function: &Function) {
    if let Some(attributes) = render_standard_attributes(function) {
        output.push_str(&attributes);
        output.push(' ');
    }
    let linkage = render_linkage_attributes(&function.linkage);
    output.push_str(&linkage);
    output.push(' ');
}

fn render_standard_attributes(function: &Function) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(message) = &function.deprecated {
        if let Some(text) = message {
            parts.push(format!("deprecated(\"{}\")", text));
        } else {
            parts.push("deprecated".to_string());
        }
    }
    if let Some(reason) = &function.nodiscard {
        if let Some(text) = reason {
            parts.push(format!("nodiscard(\"{}\")", text));
        } else {
            parts.push("nodiscard".to_string());
        }
    }
    if function.maybe_unused.is_some() {
        parts.push("maybe_unused".to_string());
    }
    if function.noreturn.is_some() {
        parts.push("noreturn".to_string());
    }
    if parts.is_empty() {
        None
    } else {
        Some(format!("[[{}]]", parts.join(", ")))
    }
}

fn render_linkage_attributes(linkage: &Linkage) -> String {
    let mut parts = Vec::new();
    match linkage {
        Linkage::HostImport { module, name } => {
            if module != "env" {
                parts.push(format!("clang::import_module(\"{}\")", module));
            }
            parts.push(format!("clang::import_name(\"{}\")", name));
        }
        Linkage::GuestExport { name } => {
            parts.push(format!("clang::export_name(\"{}\")", name));
        }
    }
    format!("[[{}]]", parts.join(", "))
}

fn generate_function(output: &mut String, func_decl: &Function) {
    generate_comments(output, &func_decl.comment);
    write_function_attributes(output, func_decl);
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
            let (prefix, suffix) = to_c_parts(&param.param_type);
            output.push_str(&prefix);
            if !suffix.is_empty() {
                output.push_str(&suffix);
            }
        }
    }

    output.push(')');
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
            "[[clang::import_name(\"add\")]] int add(int a, int b);\n"
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
            "// Test function\n[[clang::import_name(\"test\")]] void test();\n"
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
            "[[clang::import_module(\"math\"), clang::import_name(\"add\")]] int imported();\n"
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
        assert_eq!(result, "[[clang::import_name(\"log\")]] void log();\n");
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
        assert_eq!(result, "[[clang::export_name(\"run\")]] void run();\n");
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
            "[[deprecated, nodiscard]] [[clang::import_name(\"old_func\")]] int old_func();\n"
        );
    }

    #[test]
    fn test_generate_static_assert_directive() {
        let decls = Declarations {
            structs: vec![],
            functions: vec![],
            variables: vec![],
            enums: vec![],
            directives: vec![Directive::StaticAssert {
                expr: "sizeof(int) == 4".to_string(),
                message: "int needs to be i32".to_string(),
            }],
        };

        let result = generate(&decls);
        assert_eq!(
            result,
            "static_assert(sizeof(int) == 4, \"int needs to be i32\");\n"
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

    #[test]
    fn to_c_parts_primitives() {
        assert_eq!(to_c_parts(&Type::Void), ("void".into(), "".into()));
        assert_eq!(to_c_parts(&Type::Int), ("int".into(), "".into()));
        assert_eq!(to_c_parts(&Type::UnsignedInt), ("unsigned int".into(), "".into()));
        assert_eq!(to_c_parts(&Type::Char), ("char".into(), "".into()));
        assert_eq!(to_c_parts(&Type::UnsignedChar), ("unsigned char".into(), "".into()));
        assert_eq!(to_c_parts(&Type::LongLong), ("long long".into(), "".into()));
        assert_eq!(
            to_c_parts(&Type::UnsignedLongLong),
            ("unsigned long long".into(), "".into())
        );
    }

    #[test]
    fn to_c_parts_pointer_to_pointer_doubles_star() {
        let (prefix, suffix) =
            to_c_parts(&Type::Pointer(Box::new(Type::Pointer(Box::new(Type::Char)))));
        assert_eq!(prefix, "char**");
        assert_eq!(suffix, "");
    }

    #[test]
    fn to_c_parts_pointer_inside_array_nests_correctly() {
        let (prefix, suffix) =
            to_c_parts(&Type::Array(Box::new(Type::Pointer(Box::new(Type::Int))), 4));
        assert_eq!(prefix, "int*");
        assert_eq!(suffix, "[4]");
    }
}
