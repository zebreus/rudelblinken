mod attributes;
mod common;
mod grammar;
mod model;

#[cfg(test)]
use chumsky::prelude::Rich;

pub use model::*;

use crate::Span;

/// Parse C declarations from a string, returning errors as owned `(Span, message)` pairs.
///
/// This is the parser-facing owned-error wrapper around the declaration grammar.
/// It converts chumsky's internal error type into owned data used by the
/// higher-level bindgen pipeline.
pub fn parse_declarations_checked(
    input: &str,
    source: &str,
) -> Result<Declarations, Vec<(Span, String)>> {
    grammar::parse_declarations(input, source).map_err(|errs| {
        errs.into_iter()
            .map(|err| {
                let span = err.span();
                (
                    Span {
                        source: source.to_string(),
                        start: span.start,
                        end: span.end,
                    },
                    format!("{}", err.reason()),
                )
            })
            .collect()
    })
}

/// Parse C declarations from a string.
///
/// `source` is the display name of the input (e.g. a filename or `"<stdin>"`).
/// It is embedded in the [`Span`] of every parsed declaration so that
/// error messages can reference the originating file.
#[cfg(test)]
pub fn parse_declarations<'src>(
    input: &'src str,
    source: &str,
) -> Result<Declarations, Vec<Rich<'src, char>>> {
    grammar::parse_declarations(input, source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_struct() {
        let input = r#"
            struct Point {
                int x;
                int y;
            };
        "#;

        let result = parse_declarations(input, "").unwrap();
        assert_eq!(result.structs.len(), 1);
        assert_eq!(result.functions.len(), 0);
        assert_eq!(result.variables.len(), 0);

        let s = &result.structs[0];
        assert_eq!(s.name, "Point");
        assert_eq!(s.fields.len(), 2);
        assert_eq!(s.fields[0].name, "x");
        assert_eq!(s.fields[0].field_type, Type::Int);
        assert_eq!(s.fields[1].name, "y");
        assert_eq!(s.fields[1].field_type, Type::Int);
    }

    #[test]
    fn test_parse_function() {
        let input = "int add(int a, int b);";
        let result = parse_declarations(input, "").unwrap();
        assert_eq!(result.structs.len(), 0);
        assert_eq!(result.functions.len(), 1);
        assert_eq!(result.variables.len(), 0);

        let f = &result.functions[0];
        assert_eq!(f.name, "add");
        assert_eq!(f.return_type, Type::Int);
        assert_eq!(f.parameters.len(), 2);
        assert_eq!(f.parameters[0].name, Some("a".to_string()));
        assert_eq!(f.parameters[0].param_type, Type::Int);
        assert_eq!(f.parameters[1].name, Some("b".to_string()));
        assert_eq!(f.parameters[1].param_type, Type::Int);
    }

    #[test]
    fn test_parse_variable() {
        let input = "unsigned int counter;";
        let result = parse_declarations(input, "").unwrap();
        assert_eq!(result.structs.len(), 0);
        assert_eq!(result.functions.len(), 0);
        assert_eq!(result.variables.len(), 1);

        let v = &result.variables[0];
        assert_eq!(v.name, "counter");
        assert_eq!(v.var_type, Type::UnsignedInt);
    }

    #[test]
    fn test_parse_pointer_type() {
        let input = "void* ptr;";
        let result = parse_declarations(input, "").unwrap();
        assert_eq!(result.variables.len(), 1);

        let v = &result.variables[0];
        assert_eq!(v.name, "ptr");
        assert_eq!(v.var_type, Type::Pointer(Box::new(Type::Void)));
    }

    #[test]
    fn test_parse_multiple_pointers() {
        let input = "int** double_ptr;";
        let result = parse_declarations(input, "").unwrap();
        assert_eq!(result.variables.len(), 1);

        let v = &result.variables[0];
        assert_eq!(v.name, "double_ptr");
        assert_eq!(
            v.var_type,
            Type::Pointer(Box::new(Type::Pointer(Box::new(Type::Int))))
        );
    }

    #[test]
    fn test_parse_struct_type() {
        let input = "struct Node node;";
        let result = parse_declarations(input, "").unwrap();
        assert_eq!(result.variables.len(), 1);

        let v = &result.variables[0];
        assert_eq!(v.name, "node");
        assert_eq!(v.var_type, Type::Struct("Node".to_string()));
    }

    #[test]
    fn test_parse_function_no_params() {
        let input = "void exit();";
        let result = parse_declarations(input, "").unwrap();
        assert_eq!(result.functions.len(), 1);

        let f = &result.functions[0];
        assert_eq!(f.name, "exit");
        assert_eq!(f.return_type, Type::Void);
        assert_eq!(f.parameters.len(), 0);
    }

    #[test]
    fn test_parse_multiple_declarations() {
        let input = r#"
            struct Point {
                int x;
                int y;
            };
            
            int add(int a, int b);
            unsigned int counter;
            void* get_pointer();
        "#;

        let result = parse_declarations(input, "").unwrap();
        assert_eq!(result.structs.len(), 1);
        assert_eq!(result.functions.len(), 2);
        assert_eq!(result.variables.len(), 1);

        assert_eq!(result.structs[0].name, "Point");
        assert_eq!(result.functions[0].name, "add");
        assert_eq!(result.variables[0].name, "counter");
        assert_eq!(result.functions[1].name, "get_pointer");
    }

    #[test]
    fn test_parse_anonymous_parameters() {
        let input = "int process(int, char*);";
        let result = parse_declarations(input, "").unwrap();
        assert_eq!(result.functions.len(), 1);

        let f = &result.functions[0];
        assert_eq!(f.name, "process");
        assert_eq!(f.return_type, Type::Int);
        assert_eq!(f.parameters.len(), 2);
        assert_eq!(f.parameters[0].name, None);
        assert_eq!(f.parameters[0].param_type, Type::Int);
        assert_eq!(f.parameters[1].name, None);
        assert_eq!(
            f.parameters[1].param_type,
            Type::Pointer(Box::new(Type::Char))
        );
    }

    #[test]
    fn test_parse_with_line_comments() {
        let input = r#"
            // This is a point structure
            struct Point {
                // X coordinate
                int x;
                // Y coordinate
                int y;
            };
            
            // Add two numbers
            int add(int a, int b);
            
            // Global counter
            unsigned int counter;
        "#;

        let result = parse_declarations(input, "").unwrap();
        assert_eq!(result.structs.len(), 1);
        assert_eq!(result.functions.len(), 1);
        assert_eq!(result.variables.len(), 1);

        let s = &result.structs[0];
        assert_eq!(s.name, "Point");
        assert_eq!(s.fields[0].comment, vec!["X coordinate".to_string()]);
        assert_eq!(s.fields[1].comment, vec!["Y coordinate".to_string()]);

        let f = &result.functions[0];
        assert_eq!(f.name, "add");
        assert_eq!(f.comment, vec!["Add two numbers".to_string()]);

        let v = &result.variables[0];
        assert_eq!(v.name, "counter");
        assert_eq!(v.comment, vec!["Global counter".to_string()]);
    }

    #[test]
    fn test_parse_with_block_comments() {
        let input = r#"
            /* This function calculates sum */
            int sum(int x, int y);
            
            /* A field with description */
            struct Data {
                /* The value */
                int value;
            };
        "#;

        let result = parse_declarations(input, "").unwrap();
        assert_eq!(result.functions.len(), 1);
        assert_eq!(result.structs.len(), 1);

        let f = &result.functions[0];
        assert_eq!(f.comment, vec!["This function calculates sum".to_string()]);

        let s = &result.structs[0];
        assert_eq!(s.fields[0].comment, vec!["The value".to_string()]);
    }

    #[test]
    fn test_parse_multiple_comments_on_same_declaration() {
        let input = r#"
            // First comment
            // Second comment
            /* Third comment */
            int func();
            
            // Comment one
            /* Comment two */
            struct Test {
                // Field comment 1
                // Field comment 2
                int value;
            };
            
            /* Variable comment 1 */
            // Variable comment 2
            int var;
        "#;

        let result = parse_declarations(input, "").unwrap();

        let f = &result.functions[0];
        assert_eq!(f.comment.len(), 3);
        assert_eq!(f.comment[0], "First comment");
        assert_eq!(f.comment[1], "Second comment");
        assert_eq!(f.comment[2], "Third comment");

        let s = &result.structs[0];
        assert_eq!(s.comment.len(), 2);
        assert_eq!(s.comment[0], "Comment one");
        assert_eq!(s.comment[1], "Comment two");
        assert_eq!(s.fields[0].comment.len(), 2);
        assert_eq!(s.fields[0].comment[0], "Field comment 1");
        assert_eq!(s.fields[0].comment[1], "Field comment 2");

        let v = &result.variables[0];
        assert_eq!(v.comment.len(), 2);
        assert_eq!(v.comment[0], "Variable comment 1");
        assert_eq!(v.comment[1], "Variable comment 2");
    }

    #[test]
    fn test_parse_declarations_without_comments() {
        let input = r#"
            struct Empty {
                int field;
            };
            
            void no_comment();
            
            int no_comment_var;
        "#;

        let result = parse_declarations(input, "").unwrap();

        assert_eq!(result.structs[0].comment.len(), 0);
        assert_eq!(result.structs[0].fields[0].comment.len(), 0);
        assert_eq!(result.functions[0].comment.len(), 0);
        assert_eq!(result.variables[0].comment.len(), 0);
    }

    #[test]
    fn test_parse_mixed_comments_and_no_comments() {
        let input = r#"
            // Commented struct
            struct A {
                // Commented field
                int x;
                int y;
            };
            
            void uncommented_func();
            
            // Commented variable
            int var;
        "#;

        let result = parse_declarations(input, "").unwrap();

        let s = &result.structs[0];
        assert_eq!(s.comment, vec!["Commented struct".to_string()]);
        assert_eq!(s.fields[0].comment, vec!["Commented field".to_string()]);
        assert_eq!(s.fields[1].comment.len(), 0);

        let f = &result.functions[0];
        assert_eq!(f.comment.len(), 0);

        let v = &result.variables[0];
        assert_eq!(v.comment, vec!["Commented variable".to_string()]);
    }

    #[test]
    fn test_parse_struct_with_all_fields_commented() {
        let input = r#"
            // Main structure
            struct AllCommented {
                // First field
                int a;
                /* Second field */
                int b;
                // Third field
                // With multiple lines
                char* c;
            };
        "#;

        let result = parse_declarations(input, "").unwrap();
        let s = &result.structs[0];

        assert_eq!(s.comment, vec!["Main structure".to_string()]);
        assert_eq!(s.fields.len(), 3);

        assert_eq!(s.fields[0].comment, vec!["First field".to_string()]);
        assert_eq!(s.fields[1].comment, vec!["Second field".to_string()]);
        assert_eq!(s.fields[2].comment.len(), 2);
        assert_eq!(s.fields[2].comment[0], "Third field");
        assert_eq!(s.fields[2].comment[1], "With multiple lines");
    }

    #[test]
    fn test_parse_multiline_block_comment() {
        let input = r#"
            /*
             * This is a multiline
             * block comment for
             * a function
             */
            void documented();
        "#;

        let result = parse_declarations(input, "").unwrap();
        let f = &result.functions[0];

        assert_eq!(f.comment.len(), 1);
        assert!(f.comment[0].contains("multiline"));
        assert!(f.comment[0].contains("block comment"));
    }

    #[test]
    fn test_parse_empty_comments() {
        let input = r#"
            //
            /* */
            int func();
        "#;

        let result = parse_declarations(input, "").unwrap();
        let f = &result.functions[0];

        assert_eq!(f.comment.len(), 2);
        assert_eq!(f.comment[0], "");
        assert_eq!(f.comment[1], "");
    }

    #[test]
    fn test_parse_function_with_import_attribute() {
        let input = r#"
            [[clang::import_module("math"), clang::import_name("add")]] int add(int a, int b);
        "#;

        let result = parse_declarations(input, "").unwrap();
        assert_eq!(result.functions.len(), 1);

        let f = &result.functions[0];
        assert_eq!(f.name, "add");
        assert!(f.c23_attributes.is_some());

        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.import_module, Some("math".to_string()));
        assert_eq!(attrs.import_name, Some("add".to_string()));
    }

    #[test]
    fn test_parse_function_with_only_import_module() {
        let input = r#"
            [[clang::import_module("env")]] void log(char* msg);
        "#;

        let result = parse_declarations(input, "").unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.import_module, Some("env".to_string()));
        assert_eq!(attrs.import_name, None);
    }

    #[test]
    fn test_parse_function_with_only_import_name() {
        let input = r#"
            [[clang::import_name("print_fn")]] void print();
        "#;

        let result = parse_declarations(input, "").unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.import_module, None);
        assert_eq!(attrs.import_name, Some("print_fn".to_string()));
    }

    #[test]
    fn test_parse_function_with_export_name() {
        let input = r#"
            [[clang::export_name("guest_run")]] void run();
        "#;

        let result = parse_declarations(input, "").unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.export_name, Some("guest_run".to_string()));
        assert_eq!(attrs.import_module, None);
        assert_eq!(attrs.import_name, None);
    }

    #[test]
    fn test_parse_function_without_attribute() {
        let input = "int add(int a, int b);";

        let result = parse_declarations(input, "").unwrap();
        let f = &result.functions[0];
        assert!(f.c23_attributes.is_none());
    }

    #[test]
    fn test_parse_mixed_with_and_without_attributes() {
        let input = r#"
            [[clang::import_module("mod1")]] int func1();
            int func2();
            [[clang::import_name("func_three")]] int func3();
        "#;

        let result = parse_declarations(input, "").unwrap();
        assert_eq!(result.functions.len(), 3);

        assert!(result.functions[0].c23_attributes.is_some());
        assert!(result.functions[1].c23_attributes.is_none());
        assert!(result.functions[2].c23_attributes.is_some());
    }

    #[test]
    fn test_parse_c23_deprecated_attribute() {
        let input = r#"
            [[deprecated]] int old_func();
        "#;

        let result = parse_declarations(input, "").unwrap();
        assert_eq!(result.functions.len(), 1);

        let f = &result.functions[0];
        assert_eq!(f.name, "old_func");
        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.deprecated, Some(None));
        assert_eq!(attrs.nodiscard, None);
        assert_eq!(attrs.maybe_unused, None);
        assert_eq!(attrs.noreturn, None);
    }

    #[test]
    fn test_parse_c23_deprecated_with_message() {
        let input = r#"
            [[deprecated("Use new_func instead")]] int old_func();
        "#;

        let result = parse_declarations(input, "").unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        assert_eq!(
            f.c23_attributes.as_ref().unwrap().deprecated,
            Some(Some("Use new_func instead".to_string()))
        );
    }

    #[test]
    fn test_parse_c23_nodiscard_attribute() {
        let input = r#"
            [[nodiscard]] int get_value();
        "#;

        let result = parse_declarations(input, "").unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.nodiscard, Some(None));
        assert_eq!(attrs.deprecated, None);
    }

    #[test]
    fn test_parse_c23_nodiscard_with_reason() {
        let input = r#"
            [[nodiscard("Ignoring return value causes memory leak")]] void* allocate();
        "#;

        let result = parse_declarations(input, "").unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        assert_eq!(
            f.c23_attributes.as_ref().unwrap().nodiscard,
            Some(Some("Ignoring return value causes memory leak".to_string()))
        );
    }

    #[test]
    fn test_parse_c23_maybe_unused_attribute() {
        let input = r#"
            [[maybe_unused]] int helper();
        "#;

        let result = parse_declarations(input, "").unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.maybe_unused, Some(()));
        assert_eq!(attrs.deprecated, None);
        assert_eq!(attrs.nodiscard, None);
        assert_eq!(attrs.noreturn, None);
    }

    #[test]
    fn test_parse_c23_noreturn_attribute() {
        let input = r#"
            [[noreturn]] void exit_program();
        "#;

        let result = parse_declarations(input, "").unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.noreturn, Some(()));
        assert_eq!(attrs.deprecated, None);
        assert_eq!(attrs.nodiscard, None);
        assert_eq!(attrs.maybe_unused, None);
    }

    #[test]
    fn test_parse_c23_multiple_attributes_in_one_specifier() {
        let input = r#"
            [[deprecated, nodiscard]] int func();
        "#;

        let result = parse_declarations(input, "").unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.deprecated, Some(None));
        assert_eq!(attrs.nodiscard, Some(None));
        assert_eq!(attrs.maybe_unused, None);
        assert_eq!(attrs.noreturn, None);
    }

    #[test]
    fn test_parse_c23_multiple_attribute_specifiers() {
        let input = r#"
            [[deprecated]] [[nodiscard]] int func();
        "#;

        let result = parse_declarations(input, "").unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.deprecated, Some(None));
        assert_eq!(attrs.nodiscard, Some(None));
        assert_eq!(attrs.maybe_unused, None);
        assert_eq!(attrs.noreturn, None);
    }

    #[test]
    fn test_parse_c23_attributes_with_comments() {
        let input = r#"
            // This function is deprecated
            [[deprecated("Use v2")]] int func_v1();
        "#;

        let result = parse_declarations(input, "").unwrap();
        let f = &result.functions[0];

        assert_eq!(f.comment, vec!["This function is deprecated".to_string()]);
        assert!(f.c23_attributes.is_some());
        assert_eq!(
            f.c23_attributes.as_ref().unwrap().deprecated,
            Some(Some("Use v2".to_string()))
        );
    }

    #[test]
    fn test_parse_c23_attributes_with_wasm_linkage() {
        let input = r#"
            [[nodiscard, clang::import_module("mod"), clang::import_name("get")]] int get();
        "#;

        let result = parse_declarations(input, "").unwrap();
        let f = &result.functions[0];

        assert!(f.c23_attributes.is_some());
        let attrs = f.c23_attributes.as_ref().unwrap();
        assert_eq!(attrs.nodiscard, Some(None));
        assert_eq!(attrs.import_module, Some("mod".to_string()));
        assert_eq!(attrs.import_name, Some("get".to_string()));
    }
}
