//! Generate the boilerplate for a rudelblinken program written in Rust.
//!
//! # Example
//!
//! ```rust
//! use rudelblinken_sdk_macro::{main, on_event};
//!
//! #[main]
//! pub fn main() {
//!     println!("Hello, world!");
//! }
//!
//! #[on_event]
//! fn on_event(_: rudelblinken_sdk::BleEvent) {
//!     // Do something with the BLE event
//!     println!("Got an BLE event!");
//! }
//! ```
//!
//! This expands to something roughly like this:
//!
//! ```rust
//! // Setup a custom allocator, because we can only use one page of
//! // memory but that is not supported by the default allocator
//! const HEAP_SIZE: usize = 36624;
//! static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];
//! #[global_allocator]
//! static ALLOCATOR: ::talc::Talck<::spin::Mutex<()>, ::talc::ClaimOnOom> =
//!     ::talc::Talc::new(unsafe {
//!         ::talc::ClaimOnOom::new(::talc::Span::from_array((&raw const HEAP).cast_mut()))
//!     })
//!     .lock();
//!
//! // We need a main function to be able to `cargo run` this project
//! #[allow(dead_code)]
//! fn main() {}
//!
//! // Define the struct that will implement the `Guest` and `BleGuest` traits
//! struct RudelblinkenMain;
//!
//! // Generate WASM exports for the `RudelblinkenMain` struct
//! mod _generated_exports {
//!     use super::RudelblinkenMain;
//!     use ::rudelblinken_sdk::{export, exports};
//!     ::rudelblinken_sdk::export! {RudelblinkenMain}
//! }
//!
//! // Implement the `Guest` trait for the `RudelblinkenMain` struct
//! impl ::rudelblinken_sdk::Guest for RudelblinkenMain {
//!     fn run() {
//!         println!("Hello, world!");
//!     }
//! }
//!
//! // Implement the `BleGuest` trait for the `RudelblinkenMain` struct
//! impl rudelblinken_sdk::BleGuest for RudelblinkenMain {
//!     fn on_event(_: rudelblinken_sdk::BleEvent) {
//!         // Do something with the BLE event
//!         println!("Got an BLE event!");
//!     }
//! }
//! ```
//!
//! ### Other languages
//!
//! If you want more control over the generated code, you can also use the
//!
use quote::quote;
use syn::{spanned::Spanned, FnArg, ItemFn};

fn process_on_event(input: proc_macro::TokenStream) -> Result<proc_macro::TokenStream, syn::Error> {
    let synput: ItemFn = syn::parse(input)?;

    if let Some(constness) = synput.sig.constness {
        return Err(syn::Error::new(
            constness.span(),
            "on_event function cannot be const",
        ));
    }
    if let Some(asyncness) = synput.sig.asyncness {
        return Err(syn::Error::new(
            asyncness.span(),
            "on_event function cannot be async (for now)",
        ));
    }
    if let Some(unsafety) = synput.sig.unsafety {
        return Err(syn::Error::new(
            unsafety.span(),
            "on_event function cannot be unsafe",
        ));
    }
    if let Some(abi) = synput.sig.abi {
        return Err(syn::Error::new(
            abi.span(),
            "on_event function cannot have an ABI (for now)",
        ));
    }

    if synput.sig.ident.to_string() != "on_event" {
        return Err(syn::Error::new(
            synput.sig.ident.span(),
            "on_event function must be named `on_event`",
        ));
    }
    if synput.sig.generics.params.len() > 0 {
        return Err(syn::Error::new(
            synput.sig.generics.span(),
            "on_event function cannot have generics",
        ));
    }
    if let Some(variadic) = synput.sig.variadic {
        return Err(syn::Error::new(
            variadic.span(),
            "on_event cannot have variadic arguments",
        ));
    }
    if let syn::ReturnType::Type(_, _) = synput.sig.output {
        return Err(syn::Error::new(
            synput.sig.output.span(),
            "on_event cannot return a value",
        ));
    }

    let _ = match synput.sig.inputs.first() {
        Some(FnArg::Typed(input)) => input.clone(),
        None => {
            return Err(syn::Error::new(
                synput.sig.span(),
                "on_event function must have at least one argument",
            ))
        }
        Some(FnArg::Receiver(input)) => {
            return Err(syn::Error::new(
                input.span(),
                "The first parameter of on_event needs to be an BleEvent",
            ))
        }
    };
    if synput.sig.inputs.len() != 1 {
        return Err(syn::Error::new(
            synput.sig.inputs.first().span(),
            "on_event takes exactly one argument",
        ));
    }

    // let mut inputs = Punctuated::<FnArg, Comma>::new();
    // inputs.push(FnArg::Typed(PatType {
    //     attrs: Vec::new(),
    //     pat: first_input.pat,
    //     colon_token: first_input.colon_token,
    //     ty: Box::new(syn::Type::Verbatim(
    //         quote! { ::rudelblinken_sdk::BleEvent },
    //     )),
    // }));

    let on_event_impl = syn::ImplItemFn {
        attrs: synput.attrs,
        vis: syn::Visibility::Inherited,
        defaultness: None,
        sig: synput.sig.clone(),
        block: *synput.block,
    };

    let stream = quote!(
        impl ::rudelblinken_sdk::BleGuest for RudelblinkenMain {
            #on_event_impl
        }
    );
    // println!("args2: {:?}", args2);
    // println!("input2: {:?}", stream.to_string());

    return Ok(stream.into());
}

fn process_main(input: proc_macro::TokenStream) -> Result<proc_macro::TokenStream, syn::Error> {
    let synput: ItemFn = syn::parse(input)?;

    if let Some(constness) = synput.sig.constness {
        return Err(syn::Error::new(
            constness.span(),
            "main function cannot be const",
        ));
    }
    if let Some(asyncness) = synput.sig.asyncness {
        return Err(syn::Error::new(
            asyncness.span(),
            "main function cannot be async (for now)",
        ));
    }
    if let Some(unsafety) = synput.sig.unsafety {
        return Err(syn::Error::new(
            unsafety.span(),
            "main function cannot be unsafe",
        ));
    }
    if let Some(abi) = synput.sig.abi {
        return Err(syn::Error::new(
            abi.span(),
            "main function cannot have an ABI (for now)",
        ));
    }

    if synput.sig.ident.to_string() != "main" {
        return Err(syn::Error::new(
            synput.sig.ident.span(),
            "main function must be named `main`",
        ));
    }
    if synput.sig.generics.params.len() > 0 {
        return Err(syn::Error::new(
            synput.sig.generics.span(),
            "main function cannot have generics",
        ));
    }
    if synput.sig.inputs.len() > 0 {
        return Err(syn::Error::new(
            synput.sig.inputs.first().span(),
            "main function cannot have generics",
        ));
    }
    if let Some(variadic) = synput.sig.variadic {
        return Err(syn::Error::new(
            variadic.span(),
            "main function cannot have variadic arguments",
        ));
    }
    if let syn::ReturnType::Type(_, _) = synput.sig.output {
        return Err(syn::Error::new(
            synput.sig.output.span(),
            "main function cannot return a value",
        ));
    }

    let vis = synput.vis;

    let main_impl = syn::ImplItemFn {
        attrs: synput.attrs,
        vis: syn::Visibility::Inherited,
        defaultness: None,
        sig: syn::Signature {
            constness: None,
            asyncness: None,
            unsafety: None,
            abi: None,
            fn_token: synput.sig.fn_token,
            ident: syn::Ident::new("run", synput.sig.ident.span()),
            generics: syn::Generics::default(),
            paren_token: synput.sig.paren_token,
            inputs: synput.sig.inputs,
            variadic: None,
            output: syn::ReturnType::Default,
        },
        block: *synput.block,
    };

    let stream = quote!(
        // Use a custom allocator, because we can only use one page of
        // memory but that is not supported by the default allocator
        const HEAP_SIZE: usize = 36624;
        static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];
        #[global_allocator]
        static ALLOCATOR: ::talc::Talck<::spin::Mutex<()>, ::talc::ClaimOnOom> =
            ::talc::Talc::new(unsafe {
                ::talc::ClaimOnOom::new(::talc::Span::from_array((&raw const HEAP).cast_mut()))
            })
            .lock();

        #vis struct RudelblinkenMain;

        impl ::rudelblinken_sdk::Guest for RudelblinkenMain {
            #main_impl
        }

        // We need a main function to be able to `cargo run` this project
        #[allow(dead_code)]
        fn main() {}

        // Export the RudelblinkenMain struct
        mod _generated_exports {
            use super::RudelblinkenMain;
            use ::rudelblinken_sdk::{export, exports};
            ::rudelblinken_sdk::export! {RudelblinkenMain}
        }

        // Attempt to print a somewhat helpful error message if the user
        // forgot to use `on_event`.
        mod _rudelblinken_internal {
            use super::RudelblinkenMain;
            #[allow(dead_code)]
            trait OnEventNotImplemented {
                const NO_BLE_GUEST: () = panic!("You also need to mark a function with `#[rudelblinken_sdk::on_event]`");
            }
            impl<T: ?Sized> OnEventNotImplemented for T {}
            struct Wrapper<T: ?Sized>(core::marker::PhantomData<T>);
            #[allow(dead_code)]
            impl<T: ?Sized + ::rudelblinken_sdk::BleGuest> Wrapper<T> {
                const NO_BLE_GUEST: () = ();
            }
            const _: () = Wrapper::<RudelblinkenMain>::NO_BLE_GUEST;
        }
    );

    return Ok(stream.into());
}

#[proc_macro_attribute]
pub fn main(
    _args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let result = match process_main(input) {
        Ok(stream) => stream,
        Err(err) => err.to_compile_error().into(),
    };

    return result.into();
}

#[proc_macro_attribute]
pub fn on_event(
    _args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let result = match process_on_event(input) {
        Ok(stream) => stream,
        Err(err) => err.to_compile_error().into(),
    };

    return result.into();
}
