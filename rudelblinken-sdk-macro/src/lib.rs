//! Mark a function to be executed with rudelblinken

use quote::quote;
use syn::{spanned::Spanned, FnArg, ItemFn};

fn process_on_advertisement(
    input: proc_macro::TokenStream,
) -> Result<proc_macro::TokenStream, syn::Error> {
    let synput: ItemFn = syn::parse(input)?;

    if let Some(constness) = synput.sig.constness {
        return Err(syn::Error::new(
            constness.span(),
            "on_advertisement function cannot be const",
        ));
    }
    if let Some(asyncness) = synput.sig.asyncness {
        return Err(syn::Error::new(
            asyncness.span(),
            "on_advertisement function cannot be async (for now)",
        ));
    }
    if let Some(unsafety) = synput.sig.unsafety {
        return Err(syn::Error::new(
            unsafety.span(),
            "on_advertisement function cannot be unsafe",
        ));
    }
    if let Some(abi) = synput.sig.abi {
        return Err(syn::Error::new(
            abi.span(),
            "on_advertisement function cannot have an ABI (for now)",
        ));
    }

    if synput.sig.ident.to_string() != "on_advertisement" {
        return Err(syn::Error::new(
            synput.sig.ident.span(),
            "on_advertisement function must be named `on_advertisement`",
        ));
    }
    if synput.sig.generics.params.len() > 0 {
        return Err(syn::Error::new(
            synput.sig.generics.span(),
            "on_advertisement function cannot have generics",
        ));
    }
    if let Some(variadic) = synput.sig.variadic {
        return Err(syn::Error::new(
            variadic.span(),
            "on_advertisement cannot have variadic arguments",
        ));
    }
    if let syn::ReturnType::Type(_, _) = synput.sig.output {
        return Err(syn::Error::new(
            synput.sig.output.span(),
            "on_advertisement cannot return a value",
        ));
    }

    let _ = match synput.sig.inputs.first() {
        Some(FnArg::Typed(input)) => input.clone(),
        None => {
            return Err(syn::Error::new(
                synput.sig.span(),
                "on_advertisement function must have at least one argument",
            ))
        }
        Some(FnArg::Receiver(input)) => {
            return Err(syn::Error::new(
                input.span(),
                "on_advertisement function needs to take a advertisement as its parameter",
            ))
        }
    };
    if synput.sig.inputs.len() != 1 {
        return Err(syn::Error::new(
            synput.sig.inputs.first().span(),
            "on_advertisement takes exactly one argument",
        ));
    }

    // let mut inputs = Punctuated::<FnArg, Comma>::new();
    // inputs.push(FnArg::Typed(PatType {
    //     attrs: Vec::new(),
    //     pat: first_input.pat,
    //     colon_token: first_input.colon_token,
    //     ty: Box::new(syn::Type::Verbatim(
    //         quote! { ::rudelblinken_sdk::Advertisement },
    //     )),
    // }));

    let on_advertisement_impl = syn::ImplItemFn {
        attrs: synput.attrs,
        vis: syn::Visibility::Inherited,
        defaultness: None,
        sig: synput.sig.clone(),
        block: *synput.block,
    };

    let stream = quote!(
        impl ::rudelblinken_sdk::BleGuest for RudelblinkenMain {
            #on_advertisement_impl
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
        const HEAP_SIZE: usize = 36624;
        static mut HEAP: [u8; HEAP_SIZE] = [0u8; HEAP_SIZE];

        #[global_allocator]
        static ALLOCATOR: ::talc::Talck<::spin::Mutex<()>, ::talc::ClaimOnOom> =
            ::talc::Talc::new(unsafe {
                ::talc::ClaimOnOom::new(::talc::Span::from_array((&raw const HEAP).cast_mut()))
            })
            .lock();

        #vis struct RudelblinkenMain;


        /// You need a main function to be able to `cargo run` this project
        #[allow(dead_code)]
        fn main() {}

        mod _rdl {
            /// Fallback trait with `False` for `IMPLS` if the type does not
            /// implement the given trait.
            #[allow(dead_code)]
            trait OnAdvertismentNotImplemented {
                const NO_BLE_GUEST: () = panic!("You also need to mark a function with `#[rudelblinken_sdk::on_advertisement]`");
            }
            impl<T: ?Sized> OnAdvertismentNotImplemented for T {}

            /// Concrete type with `True` for `IMPLS` if the type implements the
            /// given trait. Otherwise, it falls back to `DoesNotImpl`.
            struct Wrapper<T: ?Sized>(core::marker::PhantomData<T>);

            #[allow(dead_code)]
            impl<T: ?Sized + ::rudelblinken_sdk::BleGuest> Wrapper<T> {
                const NO_BLE_GUEST: () = ();
            }
            const _: () = Wrapper::<RudelblinkenMain>::NO_BLE_GUEST;

            /// Main is required for `cargo run`
            #[allow(dead_code)]
            fn main() {
            }

            impl ::rudelblinken_sdk::Guest for RudelblinkenMain {
                #main_impl
            }

            use super::RudelblinkenMain;
            use ::rudelblinken_sdk::{export, exports};
            ::rudelblinken_sdk::export! {RudelblinkenMain}
        }
    );

    return Ok(stream.into());
}

#[proc_macro_attribute]
pub fn main(
    _args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    // let args2: proc_macro2::TokenStream = args.into();
    // let input2: proc_macro2::TokenStream = input.into();
    let result = match process_main(input) {
        Ok(stream) => stream,
        Err(err) => err.to_compile_error().into(),
    };

    return result.into();
}

#[proc_macro_attribute]
pub fn on_advertisement(
    _args: proc_macro::TokenStream,
    input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
    let result = match process_on_advertisement(input) {
        Ok(stream) => stream,
        Err(err) => err.to_compile_error().into(),
    };

    return result.into();
}
