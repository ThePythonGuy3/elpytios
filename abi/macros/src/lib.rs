use heck::ToSnakeCase;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Fields, Ident, ItemEnum, LitInt, Token, punctuated::Punctuated};

extern crate proc_macro;

#[proc_macro_derive(SyscallTable, attributes(args, max_entries))]
pub fn derive_syscall_table(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match execute(input.into()) {
        Ok(result) => result,
        Err(e) => e.into_compile_error(),
    }
    .into()
}

fn execute(input: TokenStream) -> syn::Result<TokenStream> {
    let data = syn::parse2::<ItemEnum>(input)?;
    let mut entries = vec![];
    let mut userspace_functions = vec![];

    let mut max_entries = None;
    for attr in &data.attrs {
        if attr.path().is_ident("max_entries") && max_entries.replace(attr.parse_args::<LitInt>()?.base10_parse::<usize>()?).is_some() {
            return Err(syn::Error::new_spanned(attr, "Duplicate `max_entries(..)`!"))
        }
    }
    let max_entries = max_entries.ok_or_else(|| syn::Error::new_spanned(&data, "Missing `max_entries(..)`!"))?;
    let mut test_discriminants = vec![];

    for variant in &data.variants {
        let Fields::Unit = variant.fields else { Err(syn::Error::new_spanned(&variant, "Only unit-variant is supported!"))? };

        if let Some((.., disc)) = &variant.discriminant {
            test_discriminants.push(quote! {
                assert!(#disc < #max_entries);
            });
        } else {
            return Err(syn::Error::new_spanned(variant, "Missing discriminant!"))
        }

        let mut args = None;
        for attr in &variant.attrs {
            if attr.path().is_ident("args")
                && args
                    .replace(attr.parse_args_with(Punctuated::<Ident, Token![,]>::parse_terminated)?)
                    .is_some()
            {
                return Err(syn::Error::new_spanned(&variant, "Duplicate `args(..)`!"))
            }
        }

        let args = args.unwrap_or_default();
        let [driver, fn_type] = match args.len() {
            0 => ["syscall0", "Syscall0Fn"],
            1 => ["syscall1", "Syscall1Fn"],
            2 => ["syscall2", "Syscall2Fn"],
            3 => ["syscall3", "Syscall3Fn"],
            4 => ["syscall4", "Syscall4Fn"],
            5 => ["syscall5", "Syscall5Fn"],
            n => return Err(syn::Error::new_spanned(args, format!("Too many arguments ({n}); maximum is 5!"))),
        };
        let driver = Ident::new(driver, Span::call_site());
        let fn_type = Ident::new(fn_type, Span::call_site());

        let variant_name = &variant.ident;
        let driver_name = Ident::new(&variant.ident.to_string().to_snake_case(), Span::call_site());

        let args = args.into_iter().collect::<Vec<_>>();
        entries.push(quote! {
            pub #driver_name: crate::kernel::#fn_type
        });
        userspace_functions.push(quote! {
            #[inline(always)]
            pub unsafe fn #driver_name(#(#args: usize),*) -> usize {
                crate::userspace::#driver(Self::#variant_name as usize, #(#args),*)
            }
        });
    }

    let data_name = data.ident.clone();
    let entry_name = Ident::new(&format!("{data_name}Entry"), Span::call_site());

    Ok(quote! {
        const _: () = {
            #(#test_discriminants)*
        };

        impl #data_name {
            pub const MAX_ENTRIES: usize = #max_entries;
            pub const INVALID: usize = usize::MAX;

            #(#userspace_functions)*
        }

        #[derive(Copy, Clone)]
        pub union #entry_name {
            missing: usize,
            #(#entries),*
        }

        impl #entry_name {
            pub const MISSING: Self = Self { missing: 0 };
        }
    })
}
