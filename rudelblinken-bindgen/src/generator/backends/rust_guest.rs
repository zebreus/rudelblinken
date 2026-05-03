//! Rust guest implementation generation
//!
//! This module generates Rust code for WebAssembly guest implementations.

use crate::generator::*;
use syn::parse_quote;

fn to_syn_type(ty: &Type) -> syn::Type {
    match ty {
        Type::Void => parse_quote! { () },
        Type::Int => parse_quote! { i32 },
        Type::UnsignedInt => parse_quote! { u32 },
        Type::Char => parse_quote! { i8 },
        Type::UnsignedChar => parse_quote! { u8 },
        Type::LongLong => parse_quote! { i64 },
        Type::UnsignedLongLong => parse_quote! { u64 },
        Type::Struct(name) | Type::Enum(name) => {
            let ident = syn::Ident::new(name, proc_macro2::Span::call_site());
            parse_quote! { #ident }
        }
        Type::Pointer(inner) => {
            let inner_ty = to_syn_type(inner);
            parse_quote! { *mut #inner_ty }
        }
        Type::Array(inner, size) => {
            let inner_ty = to_syn_type(inner);
            let size_lit = syn::LitInt::new(&size.to_string(), proc_macro2::Span::call_site());
            parse_quote! { [#inner_ty; #size_lit] }
        }
    }
}

/// Generate a Rust guest implementation from declarations
pub fn generate(declarations: &Declarations) -> String {
    let mut items: Vec<syn::Item> = Vec::new();

    for struct_decl in &declarations.structs {
        items.push(generate_struct_item(struct_decl));
    }

    let mut imports_by_module: std::collections::BTreeMap<String, Vec<&Function>> =
        std::collections::BTreeMap::new();
    for func in &declarations.functions {
        if let Linkage::HostImport { module, .. } = &func.linkage {
            imports_by_module
                .entry(module.clone())
                .or_default()
                .push(func);
        }
    }
    for (module, funcs) in &imports_by_module {
        items.push(generate_extern_block(module, funcs));
    }

    for func in &declarations.functions {
        if matches!(func.linkage, Linkage::GuestExport { .. }) {
            items.push(generate_function_item(func));
        }
    }

    for var in &declarations.variables {
        items.push(generate_variable_item(var));
    }

    let file = syn::File {
        shebang: None,
        attrs: vec![parse_quote! {
            #![doc = " Auto-generated Rust guest bindings"]
        }],
        items,
    };

    prettyplease::unparse(&file)
}

fn generate_doc_comments(comments: &[String]) -> Vec<syn::Attribute> {
    comments
        .iter()
        .map(|comment| {
            let doc_str = if comment.is_empty() {
                String::new()
            } else {
                format!(" {}", comment)
            };
            parse_quote! { #[doc = #doc_str] }
        })
        .collect()
}

fn generate_struct_item(struct_decl: &Struct) -> syn::Item {
    let name = syn::Ident::new(&struct_decl.name, proc_macro2::Span::call_site());
    let doc_attrs = generate_doc_comments(&struct_decl.comment);

    let fields: Vec<syn::Field> = struct_decl
        .fields
        .iter()
        .map(|field| {
            let field_name = syn::Ident::new(&field.name, proc_macro2::Span::call_site());
            let field_type = to_syn_type(&field.field_type);
            let field_doc_attrs = generate_doc_comments(&field.comment);

            parse_quote! {
                #(#field_doc_attrs)*
                pub #field_name: #field_type
            }
        })
        .collect();

    parse_quote! {
        #(#doc_attrs)*
        #[repr(C)]
        pub struct #name {
            #(#fields),*
        }
    }
}

fn generate_extern_block(module: &str, functions: &[&Function]) -> syn::Item {
    let foreign_items: Vec<syn::ForeignItem> = functions
        .iter()
        .map(|func| generate_extern_function_item(func))
        .collect();

    parse_quote! {
        #[link(wasm_import_module = #module)]
        extern "C" {
            #(#foreign_items)*
        }
    }
}

fn generate_extern_function_item(func: &Function) -> syn::ForeignItem {
    let name = syn::Ident::new(&func.name, proc_macro2::Span::call_site());
    let mut attrs: Vec<syn::Attribute> = Vec::new();

    if let Some(msg) = &func.deprecated {
        if let Some(text) = msg {
            attrs.push(parse_quote! { #[deprecated(note = #text)] });
        } else {
            attrs.push(parse_quote! { #[deprecated] });
        }
    }
    if let Some(reason) = &func.nodiscard {
        if let Some(text) = reason {
            attrs.push(parse_quote! { #[must_use = #text] });
        } else {
            attrs.push(parse_quote! { #[must_use] });
        }
    }

    if let Linkage::HostImport {
        name: import_name, ..
    } = &func.linkage
    {
        if import_name != &func.name {
            attrs.push(parse_quote! { #[link_name = #import_name] });
        }
    }

    let inputs: Vec<syn::FnArg> = func
        .parameters
        .iter()
        .enumerate()
        .map(|(i, param)| {
            let param_name = if let Some(name) = &param.name {
                syn::Ident::new(name, proc_macro2::Span::call_site())
            } else {
                syn::Ident::new(&format!("arg{}", i), proc_macro2::Span::call_site())
            };
            let param_type = to_syn_type(&param.param_type);
            parse_quote! { #param_name: #param_type }
        })
        .collect();

    let return_type = to_syn_type(&func.return_type);
    let output: syn::ReturnType = if matches!(func.return_type, Type::Void) {
        parse_quote! {}
    } else {
        parse_quote! { -> #return_type }
    };

    parse_quote! {
        #(#attrs)*
        pub fn #name(#(#inputs),*) #output;
    }
}

fn generate_function_item(func: &Function) -> syn::Item {
    let name = syn::Ident::new(&func.name, proc_macro2::Span::call_site());
    let mut attrs = generate_doc_comments(&func.comment);

    if let Linkage::GuestExport { name: export_name } = &func.linkage {
        attrs.push(parse_quote! { #[unsafe(export_name = #export_name)] });
    }

    if let Some(deprecated) = &func.deprecated {
        if let Some(msg) = deprecated {
            attrs.push(parse_quote! { #[deprecated(note = #msg)] });
        } else {
            attrs.push(parse_quote! { #[deprecated] });
        }
    }

    if let Some(nodiscard) = &func.nodiscard {
        if let Some(reason) = nodiscard {
            attrs.push(parse_quote! { #[must_use = #reason] });
        } else {
            attrs.push(parse_quote! { #[must_use] });
        }
    }

    if func.maybe_unused.is_some() {
        attrs.push(parse_quote! { #[allow(dead_code)] });
    }

    let inputs: Vec<syn::FnArg> = func
        .parameters
        .iter()
        .enumerate()
        .map(|(i, param)| {
            let param_name = if let Some(name) = &param.name {
                syn::Ident::new(name, proc_macro2::Span::call_site())
            } else {
                syn::Ident::new(&format!("arg{}", i), proc_macro2::Span::call_site())
            };
            let param_type = to_syn_type(&param.param_type);
            parse_quote! { #param_name: #param_type }
        })
        .collect();

    let return_type = to_syn_type(&func.return_type);
    let output: syn::ReturnType = if matches!(func.return_type, Type::Void) {
        parse_quote! {}
    } else {
        parse_quote! { -> #return_type }
    };

    parse_quote! {
        #(#attrs)*
        pub fn #name(#(#inputs),*) #output {
            todo!()
        }
    }
}

fn generate_variable_item(var: &Variable) -> syn::Item {
    let name = syn::Ident::new(&var.name, proc_macro2::Span::call_site());
    let var_type = to_syn_type(&var.var_type);
    let attrs = generate_doc_comments(&var.comment);

    parse_quote! {
        #(#attrs)*
        pub static #name: #var_type = todo!();
    }
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
        assert!(result.contains("#[repr(C)]"));
        assert!(result.contains("pub struct Point"));
        assert!(result.contains("pub x: i32"));
        assert!(result.contains("pub y: i32"));
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
        assert!(result.contains("pub fn add(a: i32, b: i32) -> i32;"));
    }

    #[test]
    fn test_generate_extern_function_with_link_module() {
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
        assert!(result.contains("extern \"C\""), "output:\n{result}");
        assert!(
            result.contains(r#"wasm_import_module = "math""#),
            "output:\n{result}"
        );
        assert!(result.contains(r#"link_name = "add""#), "output:\n{result}");
        assert!(
            result.contains("pub fn imported() -> i32;"),
            "output:\n{result}"
        );
    }

    #[test]
    fn test_generate_ext_block_module_env() {
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
        assert!(result.contains("extern \"C\""), "output:\n{result}");
        assert!(
            result.contains(r#"wasm_import_module = "env""#),
            "output:\n{result}"
        );
        assert!(!result.contains("link_name"), "output:\n{result}");
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
        assert!(
            result.contains(r#"#[unsafe(export_name = "run")]"#),
            "output:\n{result}"
        );
        assert!(result.contains("pub fn run()"), "output:\n{result}");
    }

    #[test]
    fn test_generate_with_attributes() {
        let decls = Declarations {
            structs: vec![],
            functions: vec![Function {
                name: "old_func".to_string(),
                return_type: Type::Int,
                parameters: vec![],
                comment: vec![],
                linkage: host_import("old_func"),
                deprecated: Some(Some("Use new_func instead".to_string())),
                nodiscard: Some(None),
                maybe_unused: None,
                noreturn: None,
            }],
            variables: vec![],
            enums: vec![],
            directives: vec![],
        };

        let result = generate(&decls);
        assert!(result.contains("#[deprecated(note = \"Use new_func instead\")]"));
        assert!(result.contains("#[must_use]"));
    }

    #[test]
    fn test_generate_void_return() {
        let decls = Declarations {
            structs: vec![],
            functions: vec![Function {
                name: "do_something".to_string(),
                return_type: Type::Void,
                parameters: vec![],
                comment: vec![],
                linkage: host_import("do_something"),
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
        assert!(result.contains("pub fn do_something()"));
        assert!(!result.contains("-> ()"));
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
        assert!(result.contains("pub static ptr: *mut ()"));
    }

    fn fmt_rust(ty: &Type) -> String {
        let syn_ty = to_syn_type(ty);
        let file: syn::File = parse_quote! { type _T = #syn_ty; };
        let s = prettyplease::unparse(&file);
        s.strip_prefix("type _T = ")
            .and_then(|s| s.strip_suffix(";\n"))
            .unwrap_or(&s)
            .to_string()
    }

    #[test]
    fn to_syn_type_primitives_follow_wasm_c_abi() {
        assert_eq!(fmt_rust(&Type::Void), "()");
        assert_eq!(fmt_rust(&Type::Int), "i32");
        assert_eq!(fmt_rust(&Type::UnsignedInt), "u32");
        assert_eq!(fmt_rust(&Type::Char), "i8");
        assert_eq!(fmt_rust(&Type::UnsignedChar), "u8");
        assert_eq!(fmt_rust(&Type::LongLong), "i64");
        assert_eq!(fmt_rust(&Type::UnsignedLongLong), "u64");
    }
}
