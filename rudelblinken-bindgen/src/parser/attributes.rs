use chumsky::prelude::*;

use super::common::string_literal;
use super::model::C23Attributes;

// Discriminant enum for a single parsed C23 attribute item
#[derive(Clone)]
enum C23AttributeItem {
    Deprecated(Option<String>),
    Nodiscard(Option<String>),
    MaybeUnused,
    Noreturn,
    ImportModule(String),
    ImportName(String),
    ExportName(String),
}

impl C23AttributeItem {
    fn apply_to(self, attributes: &mut C23Attributes) {
        match self {
            C23AttributeItem::Deprecated(message) => set_once(
                &mut attributes.deprecated,
                message,
                &mut attributes.duplicate_attributes,
                "deprecated",
            ),
            C23AttributeItem::Nodiscard(reason) => set_once(
                &mut attributes.nodiscard,
                reason,
                &mut attributes.duplicate_attributes,
                "nodiscard",
            ),
            C23AttributeItem::MaybeUnused => set_once(
                &mut attributes.maybe_unused,
                (),
                &mut attributes.duplicate_attributes,
                "maybe_unused",
            ),
            C23AttributeItem::Noreturn => set_once(
                &mut attributes.noreturn,
                (),
                &mut attributes.duplicate_attributes,
                "noreturn",
            ),
            C23AttributeItem::ImportModule(module) => set_once(
                &mut attributes.import_module,
                module,
                &mut attributes.duplicate_attributes,
                "clang::import_module",
            ),
            C23AttributeItem::ImportName(name) => set_once(
                &mut attributes.import_name,
                name,
                &mut attributes.duplicate_attributes,
                "clang::import_name",
            ),
            C23AttributeItem::ExportName(name) => set_once(
                &mut attributes.export_name,
                name,
                &mut attributes.duplicate_attributes,
                "clang::export_name",
            ),
        }
    }
}

fn set_once<T>(slot: &mut Option<T>, value: T, duplicates: &mut Vec<String>, name: &str) {
    if slot.is_some() {
        duplicates.push(name.to_string());
    }
    *slot = Some(value);
}

// Parser for a string argument inside an attribute: `("value")`
fn opt_string_arg<'src>()
-> impl Parser<'src, &'src str, Option<String>, extra::Err<Rich<'src, char>>> {
    just('(')
        .padded()
        .ignore_then(string_literal())
        .then_ignore(just(')').padded())
        .or_not()
}

// Parser for individual C23 attribute items (standard and clang-namespaced)
fn c23_attribute_item<'src>()
-> impl Parser<'src, &'src str, C23AttributeItem, extra::Err<Rich<'src, char>>> {
    // Standard C23 attributes (no namespace)
    let deprecated = string_arg_attribute("deprecated", C23AttributeItem::Deprecated);
    let nodiscard = string_arg_attribute("nodiscard", C23AttributeItem::Nodiscard);
    let maybe_unused = marker_attribute("maybe_unused", C23AttributeItem::MaybeUnused);
    let noreturn = marker_attribute("noreturn", C23AttributeItem::Noreturn);

    // clang-namespaced WASM linkage attributes
    let import_module = clang_string_attribute("import_module", C23AttributeItem::ImportModule);
    let import_name = clang_string_attribute("import_name", C23AttributeItem::ImportName);
    let export_name = clang_string_attribute("export_name", C23AttributeItem::ExportName);

    choice((
        deprecated,
        nodiscard,
        maybe_unused,
        noreturn,
        import_module,
        import_name,
        export_name,
    ))
    .labelled("attribute name")
}

fn string_arg_attribute<'src>(
    name: &'static str,
    make_item: fn(Option<String>) -> C23AttributeItem,
) -> impl Parser<'src, &'src str, C23AttributeItem, extra::Err<Rich<'src, char>>> {
    just(name)
        .padded()
        .ignore_then(opt_string_arg())
        .map(make_item)
}

fn marker_attribute<'src>(
    name: &'static str,
    item: C23AttributeItem,
) -> impl Parser<'src, &'src str, C23AttributeItem, extra::Err<Rich<'src, char>>> {
    just(name).padded().to(item)
}

fn clang_string_attribute<'src>(
    name: &'static str,
    make_item: fn(String) -> C23AttributeItem,
) -> impl Parser<'src, &'src str, C23AttributeItem, extra::Err<Rich<'src, char>>> {
    just("clang::")
        .padded()
        .ignore_then(just(name).padded())
        .ignore_then(just('(').padded())
        .ignore_then(string_literal())
        .then_ignore(just(')').padded())
        .map(make_item)
}

// Parser for C23 attribute specifier: [[attr1, attr2, ...]]
fn c23_attribute_specifier<'src>()
-> impl Parser<'src, &'src str, Vec<C23AttributeItem>, extra::Err<Rich<'src, char>>> {
    just("[[")
        .padded()
        .ignore_then(
            c23_attribute_item()
                .separated_by(just(',').padded())
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just("]]").padded())
}

// Parser for optional C23 attributes (can have multiple attribute specifiers)
pub(super) fn opt_c23_attributes<'src>()
-> impl Parser<'src, &'src str, Option<C23Attributes>, extra::Err<Rich<'src, char>>> {
    c23_attribute_specifier()
        .repeated()
        .collect::<Vec<_>>()
        .map(|specifiers| {
            let mut attributes = C23Attributes::default();
            let mut found_any = false;
            for item in specifiers.into_iter().flatten() {
                found_any = true;
                item.apply_to(&mut attributes);
            }
            found_any.then_some(attributes)
        })
}
