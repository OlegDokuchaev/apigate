mod codegen;
mod expand;
mod parse;
mod route;
mod service;
mod template;

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::{Item, ItemMod, LitStr, parse_macro_input};

use expand::expand_fn_params;
use route::expand_route_from_fn;
use service::ServiceArgs;

#[proc_macro_attribute]
pub fn service(args: TokenStream, input: TokenStream) -> TokenStream {
    expand_service(args, input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Expands `#[apigate::service(name = "...", prefix = "...")]` on a module:
/// iterates functions, expands routes, and injects a `routes()` entrypoint.
fn expand_service(args: TokenStream, input: TokenStream) -> syn::Result<TokenStream2> {
    let args = syn::parse::<ServiceArgs>(args)?;
    let ServiceArgs {
        name,
        prefix,
        policy,
    } = args;
    let prefix = prefix.unwrap_or_else(|| LitStr::new("", Span::call_site()));

    let mut module = syn::parse::<ItemMod>(input)?;
    let apigate_path = apigate_crate_path()?;

    let Some((_, items)) = module.content.as_mut() else {
        return Err(syn::Error::new_spanned(
            &module,
            "#[apigate::service] requires an inline module body: `mod x { ... }`",
        ));
    };

    let mut route_defs = Vec::new();
    let mut generated_items = Vec::new();

    for item in items.iter_mut() {
        if let Item::Fn(f) = item {
            if let Some(extracted) = expand_route_from_fn(&apigate_path, f)? {
                route_defs.push(extracted.route_def);
                generated_items.extend(extracted.generated_items);
            }
        }
    }

    // NOTE: We intentionally generate a hidden const with all routes,
    // so it can be referenced without recomputing at runtime
    let routes_ident = syn::Ident::new("__APIGATE_ROUTES", Span::call_site());

    let service_policy = match &policy {
        None => quote!(None),
        Some(p) => quote!(Some(#p)),
    };

    items.extend(generated_items);
    items.push(syn::parse_quote! {
        #[doc(hidden)]
        pub const #routes_ident: &'static [#apigate_path::RouteDef] = &[
            #(#route_defs),*
        ];
    });

    items.push(syn::parse_quote! {
        pub fn routes() -> #apigate_path::Routes {
            #apigate_path::Routes {
                service: #name,
                prefix: #prefix,
                policy: #service_policy,
                routes: #routes_ident,
            }
        }
    });

    Ok(quote!(#module))
}

#[proc_macro_attribute]
pub fn hook(_args: TokenStream, input: TokenStream) -> TokenStream {
    expand_fn_params(input, "hook", false)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

#[proc_macro_attribute]
pub fn map(_args: TokenStream, input: TokenStream) -> TokenStream {
    expand_fn_params(input, "map", true)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Resolves the path to the `apigate` crate for use in generated code.
pub(crate) fn apigate_crate_path() -> Result<TokenStream2, syn::Error> {
    use proc_macro_crate::{FoundCrate, crate_name};

    match crate_name("apigate") {
        Ok(FoundCrate::Itself) => Ok(quote!(::apigate)),
        Ok(FoundCrate::Name(n)) => {
            let ident = syn::Ident::new(&n, Span::call_site());
            Ok(quote!(::#ident))
        }
        Err(_) => Ok(quote!(::apigate)),
    }
}

macro_rules! route_stub {
    ($name:ident) => {
        #[proc_macro_attribute]
        pub fn $name(_args: TokenStream, input: TokenStream) -> TokenStream {
            let item = parse_macro_input!(input as syn::Item);
            syn::Error::new_spanned(
                item,
                concat!(
                    "`#[apigate::",
                    stringify!($name),
                    "]` must be used inside a `#[apigate::service] mod ... {}` module"
                ),
            )
            .to_compile_error()
            .into()
        }
    };
}

route_stub!(get);
route_stub!(post);
route_stub!(put);
route_stub!(delete);
route_stub!(patch);
route_stub!(head);
route_stub!(options);
