use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::parse::Parser;
use syn::{
    Attribute, Expr, Item, ItemFn, ItemMod, Lit, Token, parse_macro_input, punctuated::Punctuated,
};

#[proc_macro_attribute]
pub fn service(args: TokenStream, input: TokenStream) -> TokenStream {
    let args_ts = TokenStream2::from(args);
    let mut module = parse_macro_input!(input as ItemMod);

    let (name, prefix, policy) = match parse_service_args(args_ts) {
        Ok(v) => v,
        Err(err) => return err.to_compile_error().into(),
    };

    let apigate_path = match apigate_crate_path() {
        Ok(p) => p,
        Err(err) => return err.to_compile_error().into(),
    };

    // Собираем RouteDef из функций внутри модуля.
    // Важно: мы удаляем атрибуты #[apigate::get/post/...], чтобы их не пытался расширять компилятор.
    let mut route_defs: Vec<TokenStream2> = Vec::new();

    if let Some((_brace, items)) = &mut module.content {
        for item in items.iter_mut() {
            if let Item::Fn(f) = item {
                if let Some(route) = extract_route_from_fn(&apigate_path, f) {
                    route_defs.push(route);
                }
            }
        }
    } else {
        return syn::Error::new_spanned(
            &module,
            "#[apigate::service] currently requires an inline module body: `mod x { ... }`",
        )
        .to_compile_error()
        .into();
    }

    let routes_ident = syn::Ident::new("__APIGATE_ROUTES", Span::call_site());

    let service_policy = match policy {
        None => quote!(None),
        Some(p) => quote!(Some(#p)),
    };

    let const_ts = quote! {
        #[doc(hidden)]
        pub const #routes_ident: &'static [#apigate_path::RouteDef] = &[
            #(#route_defs),*
        ];
    };

    let fn_ts = quote! {
        pub fn routes() -> #apigate_path::Routes {
            #apigate_path::Routes {
                service: #name,
                prefix: #prefix,
                policy: #service_policy,
                routes: #routes_ident,
            }
        }
    };

    let const_item: syn::Item =
        syn::parse2(const_ts).expect("failed to parse generated const item");
    let fn_item: syn::Item = syn::parse2(fn_ts).expect("failed to parse generated fn item");

    if let Some((_brace, items)) = &mut module.content {
        items.push(const_item);
        items.push(fn_item);
    } else {
        return syn::Error::new_spanned(
            &module,
            "#[apigate::service] requires inline module: `mod x { ... }`",
        )
        .to_compile_error()
        .into();
    }

    quote!(#module).into()
}

fn apigate_crate_path() -> Result<TokenStream2, syn::Error> {
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

fn parse_service_args(args: TokenStream2) -> Result<(String, String, Option<String>), syn::Error> {
    let parser = Punctuated::<Expr, Token![,]>::parse_terminated;
    let exprs: Punctuated<Expr, Token![,]> = parser.parse2(args)?;

    let mut name: Option<String> = None;
    let mut prefix: Option<String> = None;
    let mut policy: Option<String> = None;

    for e in exprs {
        if let Expr::Assign(assign) = e {
            let left = *assign.left;
            let right = *assign.right;

            let key = match left {
                Expr::Path(p) => p.path.segments.last().unwrap().ident.to_string(),
                _ => continue,
            };

            let val = match right {
                Expr::Lit(lit) => match lit.lit {
                    Lit::Str(s) => s.value(),
                    _ => continue,
                },
                _ => continue,
            };

            match key.as_str() {
                "name" => name = Some(val),
                "prefix" => prefix = Some(val),
                "policy" => policy = Some(val),
                _ => {}
            }
        }
    }

    let name =
        name.ok_or_else(|| syn::Error::new(Span::call_site(), "missing `name = \"...\"`"))?;
    let prefix =
        prefix.ok_or_else(|| syn::Error::new(Span::call_site(), "missing `prefix = \"...\"`"))?;

    Ok((name, prefix, policy))
}

/// Если функция имеет атрибут #[apigate::get/post/...], возвращает TokenStream для RouteDef.
/// Также удаляет этот атрибут из функции.
fn extract_route_from_fn(apigate_path: &TokenStream2, f: &mut ItemFn) -> Option<TokenStream2> {
    // ищем атрибут apigate::get/post/...
    let mut found: Option<(MethodKind, RouteArgs, usize)> = None;

    for (idx, attr) in f.attrs.iter().enumerate() {
        if let Some((kind, args)) = parse_route_attr(attr) {
            found = Some((kind, args, idx));
            break;
        }
    }

    let (kind, args, idx) = found?;

    // удалить атрибут, чтобы компилятор не пытался его расширять дальше
    f.attrs.remove(idx);

    let method = match kind {
        MethodKind::Get => quote!(#apigate_path::Method::Get),
        MethodKind::Post => quote!(#apigate_path::Method::Post),
        MethodKind::Put => quote!(#apigate_path::Method::Put),
        MethodKind::Delete => quote!(#apigate_path::Method::Delete),
        MethodKind::Patch => quote!(#apigate_path::Method::Patch),
        MethodKind::Head => quote!(#apigate_path::Method::Head),
        MethodKind::Options => quote!(#apigate_path::Method::Options),
    };

    let path = args.path;
    let to = match args.to {
        None => quote!(None),
        Some(t) => quote!(Some(#t)),
    };
    let policy = match args.policy {
        None => quote!(None),
        Some(p) => quote!(Some(#p)),
    };

    Some(quote! {
        #apigate_path::RouteDef {
            method: #method,
            path: #path,
            to: #to,
            policy: #policy,
        }
    })
}

#[derive(Clone, Copy)]
enum MethodKind {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

struct RouteArgs {
    path: String,
    to: Option<String>,
    policy: Option<String>,
}

/// Парсим #[apigate::get("/x", to="/y", ...)]
fn parse_route_attr(attr: &Attribute) -> Option<(MethodKind, RouteArgs)> {
    let last = attr.path().segments.last()?.ident.to_string();

    let kind = match last.as_str() {
        "get" => MethodKind::Get,
        "post" => MethodKind::Post,
        "put" => MethodKind::Put,
        "delete" => MethodKind::Delete,
        "patch" => MethodKind::Patch,
        "head" => MethodKind::Head,
        "options" => MethodKind::Options,
        _ => return None,
    };

    // args как список Expr: первый аргумент обычно строка "/path"
    let exprs: Punctuated<Expr, Token![,]> =
        attr.parse_args_with(Punctuated::parse_terminated).ok()?;

    let mut path: Option<String> = None;
    let mut to: Option<String> = None;
    let mut policy: Option<String> = None;

    for (i, e) in exprs.into_iter().enumerate() {
        match e {
            // вариант: "/ping"
            Expr::Lit(lit) if i == 0 => {
                if let Lit::Str(s) = lit.lit {
                    path = Some(s.value());
                }
            }
            // вариант: path = "/ping"
            Expr::Assign(assign) => {
                let key = match *assign.left {
                    Expr::Path(p) => p.path.segments.last().unwrap().ident.to_string(),
                    _ => continue,
                };
                let val = match *assign.right {
                    Expr::Lit(l) => match l.lit {
                        Lit::Str(s) => s.value(),
                        _ => continue,
                    },
                    _ => continue,
                };

                match key.as_str() {
                    "path" => path = Some(val),
                    "to" => to = Some(val),
                    "policy" => policy = Some(val),
                    _ => {}
                }
            }
            _ => {}
        }
    }

    let path = path?;
    Some((kind, RouteArgs { path, to, policy }))
}

// --- optional: заглушки, чтобы если кто-то случайно использует #[apigate::get] вне #[apigate::service],
// он получил понятную ошибку. В нормальном сценарии эти атрибуты "съедаются" макросом service.

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
