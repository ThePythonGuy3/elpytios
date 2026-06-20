use heck::ToSnakeCase;
use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Fields, Ident, ItemEnum, Token, punctuated::Punctuated};

extern crate proc_macro;

#[proc_macro_derive(SyscallTable, attributes(args))]
pub fn derive_syscall_table(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    match execute(input.into()) {
        Ok(result) => result,
        Err(e) => e.into_compile_error(),
    }
    .into()
}

fn execute(input: TokenStream) -> syn::Result<TokenStream> {
    let data = syn::parse2::<ItemEnum>(input)?;
    let mut functions = vec![];

    for variant in &data.variants {
        let Fields::Unit = variant.fields else { Err(syn::Error::new_spanned(&variant, "Only unit-variant is supported!"))? };

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
        let driver = Ident::new(
            match args.len() {
                0 => "syscall0",
                1 => "syscall1",
                2 => "syscall2",
                3 => "syscall3",
                4 => "syscall4",
                5 => "syscall5",
                n => return Err(syn::Error::new_spanned(args, format!("Too many arguments ({n}); maximum is 5!"))),
            },
            Span::call_site(),
        );

        let variant_name = &variant.ident;
        let driver_name = Ident::new(&variant.ident.to_string().to_snake_case(), Span::call_site());

        let args = args.into_iter().collect::<Vec<_>>();
        functions.push(quote! {
            #[inline(always)]
            pub unsafe fn #driver_name(#(#args: usize),*) -> usize {
                crate::userspace::#driver(Self::#variant_name as usize, #(#args),*)
            }
        });
    }

    let data_name = data.ident.clone();
    Ok(quote! {
        #[cfg(target_os = "elpytios")]
        impl #data_name {
            #(#functions)*
        }
    })
}
