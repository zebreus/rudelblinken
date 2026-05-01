//! Rust guest implementation generation
//!
//! This module generates Rust code for WebAssembly guest implementations.

use crate::generator::*;
use syn::parse_quote;

/// Generate a Rust guest implementation from declarations
pub fn generate(declarations: &Declarations) -> String {
    let mut items: Vec<syn::Item> = Vec::new();

    // Generate struct definitions
    for struct_decl in &declarations.structs {
        items.push(generate_struct_item(struct_decl));
    }

    // Generate extern block for imported functions
    let imported_functions: Vec<_> = declarations
        .functions
        .iter()
        .filter(|f| f.import_module.is_some() || f.import_name.is_some())
        .collect();

    if !imported_functions.is_empty() {
        items.push(generate_extern_block(&imported_functions));
    }

    // Generate regular function declarations
    for func in &declarations.functions {
        if func.import_module.is_none() && func.import_name.is_none() {
            items.push(generate_function_item(func));
        }
    }

    // Generate static variables
    for var in &declarations.variables {
        items.push(generate_variable_item(var));
    }

    // Build the file
    let file = syn::File {
        shebang: None,
        attrs: vec![parse_quote! {
            #![doc = " Auto-generated Rust guest bindings"]
        }],
        items,
    };

    prettyplease::unparse(&file)
}

fn parse_type(type_decl: &Type) -> syn::Type {
    match type_decl {
        Type::Void => parse_quote! { () },
        Type::Bool => parse_quote! { bool },
        Type::Char => parse_quote! { i8 },
        Type::SignedChar => parse_quote! { i8 },
        Type::UnsignedChar => parse_quote! { u8 },
        Type::Short => parse_quote! { i16 },
        Type::UnsignedShort => parse_quote! { u16 },
        Type::Int => parse_quote! { i32 },
        Type::UnsignedInt => parse_quote! { u32 },
        Type::Long => parse_quote! { i64 },
        Type::UnsignedLong => parse_quote! { u64 },
        Type::LongLong => parse_quote! { i64 },
        Type::UnsignedLongLong => parse_quote! { u64 },
        Type::Float => parse_quote! { f32 },
        Type::Double => parse_quote! { f64 },
        Type::Struct(name) => {
            let ident = syn::Ident::new(name, proc_macro2::Span::call_site());
            parse_quote! { #ident }
        }
        Type::Pointer(inner) => {
            let inner_ty = parse_type(inner);
            parse_quote! { *mut #inner_ty }
        }
        Type::Named(name) => {
            let ident = syn::Ident::new(name, proc_macro2::Span::call_site());
            parse_quote! { #ident }
        }
    }
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
            let field_type = parse_type(&field.field_type);
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

fn generate_extern_block(functions: &[&Function]) -> syn::Item {
    let foreign_items: Vec<syn::ForeignItem> = functions
        .iter()
        .map(|func| generate_extern_function_item(func))
        .collect();

    parse_quote! {
        extern "C" {
            #(#foreign_items)*
        }
    }
}

fn generate_extern_function_item(func: &Function) -> syn::ForeignItem {
    let name = syn::Ident::new(&func.name, proc_macro2::Span::call_site());
    let mut attrs: Vec<syn::Attribute> = Vec::new();

    // Add link_name attribute if present
    if let Some(import_name) = &func.import_name {
        attrs.push(parse_quote! { #[link_name = #import_name] });
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
            let param_type = parse_type(&param.param_type);
            parse_quote! { #param_name: #param_type }
        })
        .collect();

    let return_type = parse_type(&func.return_type);
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

    // Add deprecation attribute if present
    if let Some(deprecated) = &func.deprecated {
        if let Some(msg) = deprecated {
            attrs.push(parse_quote! { #[deprecated(note = #msg)] });
        } else {
            attrs.push(parse_quote! { #[deprecated] });
        }
    }

    // Add must_use attribute for nodiscard
    if let Some(nodiscard) = &func.nodiscard {
        if let Some(reason) = nodiscard {
            attrs.push(parse_quote! { #[must_use = #reason] });
        } else {
            attrs.push(parse_quote! { #[must_use] });
        }
    }

    // Add allow(dead_code) for maybe_unused
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
            let param_type = parse_type(&param.param_type);
            parse_quote! { #param_name: #param_type }
        })
        .collect();

    let return_type = parse_type(&func.return_type);
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
    let var_type = parse_type(&var.var_type);
    let attrs = generate_doc_comments(&var.comment);

    parse_quote! {
        #(#attrs)*
        pub static #name: #var_type = todo!();
    }
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
                import_module: None,
                import_name: None,
                deprecated: None,
                nodiscard: None,
                maybe_unused: None,
                noreturn: None,
            }],
            variables: vec![],
        };

        let result = generate(&decls);
        assert!(result.contains("pub fn add(a: i32, b: i32) -> i32"));
        assert!(result.contains("todo!()"));
    }

    #[test]
    fn test_generate_extern_function() {
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
        };

        let result = generate(&decls);
        assert!(result.contains("extern \"C\""));
        assert!(result.contains("#[link_name = \"add\"]"));
        assert!(result.contains("pub fn imported() -> i32;"));
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
                import_module: None,
                import_name: None,
                deprecated: Some(Some("Use new_func instead".to_string())),
                nodiscard: Some(None),
                maybe_unused: None,
                noreturn: None,
            }],
            variables: vec![],
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
                import_module: None,
                import_name: None,
                deprecated: None,
                nodiscard: None,
                maybe_unused: None,
                noreturn: None,
            }],
            variables: vec![],
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
                import_module: None,
                import_name: None,
            }],
        };

        let result = generate(&decls);
        assert!(result.contains("pub static ptr: *mut ()"));
    }
}
