use std::collections::HashMap;

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::parse::Parser;
use syn::{
    Attribute, Expr, Item, ItemFn, ItemMod, Lit, Path, Token, Type, TypePath, parse_macro_input,
    punctuated::Punctuated,
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
    let mut generated_items: Vec<Item> = Vec::new();

    if let Some((_brace, items)) = &mut module.content {
        for item in items.iter_mut() {
            if let Item::Fn(f) = item {
                match extract_route_from_fn(&apigate_path, f) {
                    Ok(Some(extracted)) => {
                        route_defs.push(extracted.route_def);
                        generated_items.extend(extracted.generated_items);
                    }
                    Ok(None) => {}
                    Err(err) => return err.to_compile_error().into(),
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

    let const_item: Item = syn::parse2(const_ts).expect("failed to parse generated const item");
    let fn_item: Item = syn::parse2(fn_ts).expect("failed to parse generated fn item");

    if let Some((_brace, items)) = &mut module.content {
        items.extend(generated_items);
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

#[proc_macro_attribute]
pub fn hook(_args: TokenStream, input: TokenStream) -> TokenStream {
    // Пока hook — identity macro.
    // Позже сюда можно добавить валидацию сигнатуры.
    input
}

#[proc_macro_attribute]
pub fn map(_args: TokenStream, input: TokenStream) -> TokenStream {
    input
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

struct ExtractedRoute {
    route_def: TokenStream2,
    generated_items: Vec<Item>,
}

/// Если функция имеет атрибут #[apigate::get/post/...], возвращает RouteDef + сгенерированные helper items.
/// Также удаляет этот атрибут из функции.
fn extract_route_from_fn(
    apigate_path: &TokenStream2,
    f: &mut ItemFn,
) -> Result<Option<ExtractedRoute>, syn::Error> {
    let mut found: Option<(MethodKind, RouteArgs, usize)> = None;

    for (idx, attr) in f.attrs.iter().enumerate() {
        if let Some((kind, args)) = parse_route_attr(attr)? {
            found = Some((kind, args, idx));
            break;
        }
    }

    let Some((kind, args, idx)) = found else {
        return Ok(None);
    };

    // удалить route-атрибут, чтобы компилятор не пытался его расширять дальше
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

    let path = &args.path;
    let policy = match args.policy {
        None => quote!(None),
        Some(p) => quote!(Some(#p)),
    };

    let mut generated_items = Vec::new();

    let rewrite_spec = match &args.to {
        None => quote!(#apigate_path::RewriteSpec::StripPrefix),
        Some(t) if !t.contains('{') => quote!(#apigate_path::RewriteSpec::Static(#t)),
        Some(t) => {
            let compiled = compile_rewrite_template(apigate_path, path, t)
                .map_err(|msg| syn::Error::new(Span::call_site(), msg))?;

            let route_fn_ident = &f.sig.ident;
            let template_ident = format_ident!("__APIGATE_REWRITE_{}", route_fn_ident);
            let src_tokens = &compiled.src_tokens;
            let dst_tokens = &compiled.dst_tokens;
            let static_len = compiled.static_len;

            let template_static = quote! {
                #[doc(hidden)]
                static #template_ident: #apigate_path::RewriteTemplate = #apigate_path::RewriteTemplate {
                    src: &[#(#src_tokens),*],
                    dst: &[#(#dst_tokens),*],
                    static_len: #static_len,
                };
            };

            let template_item: Item = syn::parse2(template_static)?;
            generated_items.push(template_item);

            quote!(#apigate_path::RewriteSpec::Template(&#template_ident))
        }
    };

    let before = generate_before_wrapper(apigate_path, f, &args.before, &mut generated_items)?;
    let map = generate_map_wrapper(
        apigate_path,
        f,
        &args.data,
        args.map.as_ref(),
        &mut generated_items,
    )?;

    let route_def = quote! {
        #apigate_path::RouteDef {
            method: #method,
            path: #path,
            rewrite: #rewrite_spec,
            policy: #policy,
            before: #before,
            map: #map,
        }
    };

    Ok(Some(ExtractedRoute {
        route_def,
        generated_items,
    }))
}

fn generate_before_wrapper(
    apigate_path: &TokenStream2,
    f: &ItemFn,
    hooks: &[Path],
    generated_items: &mut Vec<Item>,
) -> Result<TokenStream2, syn::Error> {
    if hooks.is_empty() {
        return Ok(quote!(None));
    }

    let route_fn_ident = &f.sig.ident;
    let before_ident = format_ident!("__apigate_before_{}", route_fn_ident);

    let hook_calls = hooks.iter().map(|hook_path| {
        quote! {
            #hook_path(&mut ctx).await?;
        }
    });

    let before_fn_ts = quote! {
        #[doc(hidden)]
        fn #before_ident<'a>(
            mut ctx: #apigate_path::PartsCtx<'a>,
        ) -> #apigate_path::BeforeFuture<'a> {
            ::std::boxed::Box::pin(async move {
                #(#hook_calls)*
                Ok(())
            })
        }
    };

    let before_fn_item: Item = syn::parse2(before_fn_ts)?;
    generated_items.push(before_fn_item);

    Ok(quote!(Some(#before_ident as #apigate_path::BeforeFn)))
}

fn generate_map_wrapper(
    apigate_path: &TokenStream2,
    f: &ItemFn,
    data: &DataKind,
    map_fn: Option<&Path>,
    generated_items: &mut Vec<Item>,
) -> Result<TokenStream2, syn::Error> {
    let Some(map_fn) = map_fn else {
        return Ok(quote!(None));
    };

    let route_fn_ident = &f.sig.ident;
    let wrapper_ident = format_ident!("__apigate_map_{}", route_fn_ident);

    let wrapper_ts = match data {
        DataKind::Json(ty) => generate_json_map_wrapper(apigate_path, &wrapper_ident, ty, map_fn),
        DataKind::Query(ty) => generate_query_map_wrapper(apigate_path, &wrapper_ident, ty, map_fn),
        DataKind::Form(ty) => generate_form_map_wrapper(apigate_path, &wrapper_ident, ty, map_fn),
        DataKind::Multipart => {
            return Err(syn::Error::new_spanned(
                route_fn_ident,
                "`map` is not supported with `multipart`",
            ));
        }
        DataKind::None => {
            return Err(syn::Error::new_spanned(
                route_fn_ident,
                "`map` requires one of `query = T`, `json = T`, or `form = T`",
            ));
        }
    };

    let wrapper_item: Item = syn::parse2(wrapper_ts)?;
    generated_items.push(wrapper_item);

    Ok(quote!(Some(#wrapper_ident as #apigate_path::MapFn)))
}

fn generate_json_map_wrapper(
    apigate_path: &TokenStream2,
    wrapper_ident: &syn::Ident,
    ty: &Type,
    map_fn: &Path,
) -> TokenStream2 {
    quote! {
        #[doc(hidden)]
        fn #wrapper_ident<'a>(
            mut ctx: #apigate_path::PartsCtx<'a>,
            body: #apigate_path::__private::axum::body::Body,
            limit: usize,
        ) -> #apigate_path::MapFuture<'a> {
            ::std::boxed::Box::pin(async move {
                let bytes = #apigate_path::__private::axum::body::to_bytes(body, limit)
                    .await
                    .map_err(|_| #apigate_path::ApigateError::payload_too_large("request body is too large"))?;

                let input: #ty = #apigate_path::__private::serde_json::from_slice(&bytes)
                    .map_err(|_| #apigate_path::ApigateError::bad_request("invalid json body"))?;

                let output = #map_fn(input, &mut ctx).await?;

                let new_body = #apigate_path::__private::serde_json::to_vec(&output)
                    .map_err(|_| #apigate_path::ApigateError::internal("failed to serialize mapped json"))?;

                ctx.headers_mut().insert(
                    #apigate_path::__private::http::header::CONTENT_TYPE,
                    #apigate_path::__private::http::HeaderValue::from_static("application/json"),
                );
                ctx.headers_mut().remove(#apigate_path::__private::http::header::CONTENT_LENGTH);

                Ok(#apigate_path::__private::axum::body::Body::from(new_body))
            })
        }
    }
}

fn generate_query_map_wrapper(
    apigate_path: &TokenStream2,
    wrapper_ident: &syn::Ident,
    ty: &Type,
    map_fn: &Path,
) -> TokenStream2 {
    quote! {
        #[doc(hidden)]
        fn #wrapper_ident<'a>(
            mut ctx: #apigate_path::PartsCtx<'a>,
            body: #apigate_path::__private::axum::body::Body,
            _limit: usize,
        ) -> #apigate_path::MapFuture<'a> {
            ::std::boxed::Box::pin(async move {
                let input: #ty = #apigate_path::__private::axum::extract::Query::<#ty>::try_from_uri(ctx.uri())
                    .map_err(|_| #apigate_path::ApigateError::bad_request("invalid query"))?
                    .0;

                let output = #map_fn(input, &mut ctx).await?;

                let encoded = #apigate_path::__private::serde_urlencoded::to_string(&output)
                    .map_err(|_| #apigate_path::ApigateError::internal("failed to serialize mapped query"))?;

                let path = ctx.uri().path().to_string();
                let mut path_and_query = path;

                if !encoded.is_empty() {
                    path_and_query.push('?');
                    path_and_query.push_str(&encoded);
                }

                *ctx.uri_mut() = #apigate_path::__private::http::Uri::builder()
                    .path_and_query(path_and_query)
                    .build()
                    .map_err(|_| #apigate_path::ApigateError::internal("failed to rebuild uri"))?;

                Ok(body)
            })
        }
    }
}

fn generate_form_map_wrapper(
    apigate_path: &TokenStream2,
    wrapper_ident: &syn::Ident,
    ty: &Type,
    map_fn: &Path,
) -> TokenStream2 {
    quote! {
        #[doc(hidden)]
        fn #wrapper_ident<'a>(
            mut ctx: #apigate_path::PartsCtx<'a>,
            body: #apigate_path::__private::axum::body::Body,
            limit: usize,
        ) -> #apigate_path::MapFuture<'a> {
            ::std::boxed::Box::pin(async move {
                let method = ctx.method().clone();

                if method == #apigate_path::__private::http::Method::GET
                    || method == #apigate_path::__private::http::Method::HEAD
                {
                    let raw = ctx.uri().query().unwrap_or_default();

                    let input: #ty = #apigate_path::__private::serde_urlencoded::from_str(raw)
                        .map_err(|_| #apigate_path::ApigateError::bad_request("invalid form query"))?;

                    let output = #map_fn(input, &mut ctx).await?;

                    let encoded = #apigate_path::__private::serde_urlencoded::to_string(&output)
                        .map_err(|_| #apigate_path::ApigateError::internal("failed to serialize mapped form"))?;

                    let path = ctx.uri().path().to_string();
                    let mut path_and_query = path;

                    if !encoded.is_empty() {
                        path_and_query.push('?');
                        path_and_query.push_str(&encoded);
                    }

                    *ctx.uri_mut() = #apigate_path::__private::http::Uri::builder()
                        .path_and_query(path_and_query)
                        .build()
                        .map_err(|_| #apigate_path::ApigateError::internal("failed to rebuild uri"))?;

                    Ok(body)
                } else {
                    let content_type = ctx
                        .headers()
                        .get(#apigate_path::__private::http::header::CONTENT_TYPE)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or_default();

                    if !content_type.starts_with("application/x-www-form-urlencoded") {
                        return Err(#apigate_path::ApigateError::unsupported_media_type(
                            "expected application/x-www-form-urlencoded",
                        ));
                    }

                    let bytes = #apigate_path::__private::axum::body::to_bytes(body, limit)
                        .await
                        .map_err(|_| #apigate_path::ApigateError::payload_too_large("form body is too large"))?;

                    let input: #ty = #apigate_path::__private::serde_urlencoded::from_bytes(&bytes)
                        .map_err(|_| #apigate_path::ApigateError::bad_request("invalid form body"))?;

                    let output = #map_fn(input, &mut ctx).await?;

                    let encoded = #apigate_path::__private::serde_urlencoded::to_string(&output)
                        .map_err(|_| #apigate_path::ApigateError::internal("failed to serialize mapped form"))?;

                    ctx.headers_mut().insert(
                        #apigate_path::__private::http::header::CONTENT_TYPE,
                        #apigate_path::__private::http::HeaderValue::from_static("application/x-www-form-urlencoded"),
                    );
                    ctx.headers_mut().remove(#apigate_path::__private::http::header::CONTENT_LENGTH);

                    Ok(#apigate_path::__private::axum::body::Body::from(encoded))
                }
            })
        }
    }
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

enum DataKind {
    None,
    Query(Type),
    Json(Type),
    Form(Type),
    Multipart,
}

struct RouteArgs {
    path: String,
    to: Option<String>,
    policy: Option<String>,
    before: Vec<Path>,
    map: Option<Path>,
    data: DataKind,
}

/// Парсим #[apigate::get("/x", to="/y", policy="...", before=[hook1, hook2])]
fn parse_route_attr(attr: &Attribute) -> Result<Option<(MethodKind, RouteArgs)>, syn::Error> {
    let last = match attr.path().segments.last() {
        Some(s) => s.ident.to_string(),
        None => return Ok(None),
    };

    let kind = match last.as_str() {
        "get" => MethodKind::Get,
        "post" => MethodKind::Post,
        "put" => MethodKind::Put,
        "delete" => MethodKind::Delete,
        "patch" => MethodKind::Patch,
        "head" => MethodKind::Head,
        "options" => MethodKind::Options,
        _ => return Ok(None),
    };

    // args как список Expr: первый аргумент обычно строка "/path"
    let exprs: Punctuated<Expr, Token![,]> = attr.parse_args_with(Punctuated::parse_terminated)?;

    let mut path: Option<String> = None;
    let mut to: Option<String> = None;
    let mut policy: Option<String> = None;
    let mut before: Vec<Path> = Vec::new();
    let mut map: Option<Path> = None;
    let mut data = DataKind::None;

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
                    other => {
                        return Err(syn::Error::new_spanned(
                            other,
                            "unsupported left-hand side in route attribute",
                        ));
                    }
                };

                match (key.as_str(), *assign.right) {
                    ("path", Expr::Lit(l)) => {
                        if let Lit::Str(s) = l.lit {
                            path = Some(s.value());
                        } else {
                            return Err(syn::Error::new_spanned(
                                l,
                                "`path` must be a string literal",
                            ));
                        }
                    }

                    ("to", Expr::Lit(l)) => {
                        if let Lit::Str(s) = l.lit {
                            to = Some(s.value());
                        } else {
                            return Err(syn::Error::new_spanned(
                                l,
                                "`to` must be a string literal",
                            ));
                        }
                    }

                    ("policy", Expr::Lit(l)) => {
                        if let Lit::Str(s) = l.lit {
                            policy = Some(s.value());
                        } else {
                            return Err(syn::Error::new_spanned(
                                l,
                                "`policy` must be a string literal",
                            ));
                        }
                    }

                    ("before", Expr::Array(arr)) => {
                        let mut hooks = Vec::with_capacity(arr.elems.len());
                        for elem in arr.elems {
                            match elem {
                                Expr::Path(p) => hooks.push(p.path),
                                other => {
                                    return Err(syn::Error::new_spanned(
                                        other,
                                        "`before` must be an array of function paths",
                                    ));
                                }
                            }
                        }
                        before = hooks;
                    }

                    ("map", Expr::Path(p)) => {
                        map = Some(p.path);
                    }

                    ("query", expr) => {
                        let ty = expr_to_type(expr).ok_or_else(|| {
                            syn::Error::new(Span::call_site(), "`query` must be a type path")
                        })?;
                        data = ensure_single_data_kind(data, DataKind::Query(ty))?;
                    }

                    ("json", expr) => {
                        let ty = expr_to_type(expr).ok_or_else(|| {
                            syn::Error::new(Span::call_site(), "`json` must be a type path")
                        })?;
                        data = ensure_single_data_kind(data, DataKind::Json(ty))?;
                    }

                    ("form", expr) => {
                        let ty = expr_to_type(expr).ok_or_else(|| {
                            syn::Error::new(Span::call_site(), "`form` must be a type path")
                        })?;
                        data = ensure_single_data_kind(data, DataKind::Form(ty))?;
                    }

                    _ => {}
                }
            }

            Expr::Path(p) => {
                if p.path.is_ident("multipart") {
                    data = ensure_single_data_kind(data, DataKind::Multipart)?;
                }
            }

            _ => {}
        }
    }

    let path = path.ok_or_else(|| syn::Error::new(Span::call_site(), "missing route path"))?;

    if map.is_some() {
        match data {
            DataKind::Query(_) | DataKind::Json(_) | DataKind::Form(_) => {}
            DataKind::Multipart => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    "`map` is not supported together with `multipart`",
                ));
            }
            DataKind::None => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    "`map` requires one of `query = T`, `json = T`, or `form = T`",
                ));
            }
        }
    }

    Ok(Some((
        kind,
        RouteArgs {
            path,
            to,
            policy,
            before,
            map,
            data,
        },
    )))
}

fn expr_to_type(expr: Expr) -> Option<Type> {
    match expr {
        Expr::Path(p) => Some(Type::Path(TypePath {
            qself: None,
            path: p.path,
        })),
        _ => None,
    }
}

fn ensure_single_data_kind(current: DataKind, next: DataKind) -> Result<DataKind, syn::Error> {
    match current {
        DataKind::None => Ok(next),
        _ => Err(syn::Error::new(
            Span::call_site(),
            "only one of `query`, `json`, `form`, or `multipart` may be specified",
        )),
    }
}

struct CompiledTemplate {
    src_tokens: Vec<TokenStream2>,
    dst_tokens: Vec<TokenStream2>,
    static_len: usize,
}

fn compile_rewrite_template(
    apigate_path: &TokenStream2,
    route_path: &str,
    to: &str,
) -> Result<CompiledTemplate, String> {
    // Parse source segments from the route path
    let mut src_tokens = Vec::new();
    let mut param_names: Vec<String> = Vec::new();

    for seg in route_path.split('/').filter(|s| !s.is_empty()) {
        if seg.starts_with('{') && seg.ends_with('}') {
            let name = &seg[1..seg.len() - 1];
            param_names.push(name.to_string());
            src_tokens.push(quote!(#apigate_path::SrcSeg::Param));
        } else {
            src_tokens.push(quote!(#apigate_path::SrcSeg::Lit(#seg)));
        }
    }

    // Build param name -> capture index map
    let param_map: HashMap<String, u8> = param_names
        .iter()
        .enumerate()
        .map(|(i, name)| (name.clone(), i as u8))
        .collect();

    // Parse destination chunks
    let mut dst_tokens = Vec::new();
    let mut static_len = 0usize;
    let mut rest = to;

    while !rest.is_empty() {
        if let Some(brace_start) = rest.find('{') {
            if brace_start > 0 {
                let lit = &rest[..brace_start];
                static_len += lit.len();
                dst_tokens.push(quote!(#apigate_path::DstChunk::Lit(#lit)));
            }
            let brace_end = rest
                .find('}')
                .ok_or_else(|| "unclosed `{` in `to`".to_string())?;
            let param_name = &rest[brace_start + 1..brace_end];
            let &src_index = param_map.get(param_name).ok_or_else(|| {
                format!("parameter `{param_name}` in `to` not found in route path")
            })?;
            dst_tokens.push(quote!(#apigate_path::DstChunk::Capture { src_index: #src_index }));
            rest = &rest[brace_end + 1..];
        } else {
            static_len += rest.len();
            dst_tokens.push(quote!(#apigate_path::DstChunk::Lit(#rest)));
            break;
        }
    }

    Ok(CompiledTemplate {
        src_tokens,
        dst_tokens,
        static_len,
    })
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
