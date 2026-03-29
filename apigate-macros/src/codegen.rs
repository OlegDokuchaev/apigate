use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Ident, Item, ItemFn, Path, Result, Type};

use crate::route::DataKind;

// ---------------------------------------------------------------------------
// MapMode
// ---------------------------------------------------------------------------

enum MapMode<'a> {
    Json(&'a Type),
    Query(&'a Type),
    Form(&'a Type),
}

/// Resolves which map wrapper variant to generate, or errors if the
/// data kind is incompatible with `map`.
fn resolve_map_mode<'a>(data: &'a DataKind, route_fn_ident: &Ident) -> Result<MapMode<'a>> {
    match data {
        DataKind::Json(ty) => Ok(MapMode::Json(ty)),
        DataKind::Query(ty) => Ok(MapMode::Query(ty)),
        DataKind::Form(ty) => Ok(MapMode::Form(ty)),
        DataKind::Multipart => Err(syn::Error::new_spanned(
            route_fn_ident,
            "`map` is not supported with `multipart`",
        )),
        DataKind::None => Err(syn::Error::new_spanned(
            route_fn_ident,
            "`map` requires one of `query = T`, `json = T`, or `form = T`",
        )),
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Generates a hidden `__apigate_before_<fn>` function that chains all before-hooks,
/// or returns `None` if there are no hooks.
pub(crate) fn generate_before_wrapper(
    apigate_path: &TokenStream2,
    f: &ItemFn,
    hooks: &[Path],
    generated_items: &mut Vec<Item>,
) -> Result<TokenStream2> {
    if hooks.is_empty() {
        return Ok(quote!(None));
    }

    let before_ident = format_ident!("__apigate_before_{}", f.sig.ident);
    let item = before_item(apigate_path, &before_ident, hooks);
    generated_items.push(item);

    Ok(quote!(Some(#before_ident as #apigate_path::BeforeFn)))
}

/// Generates a hidden `__apigate_map_<fn>` function that deserializes the request
/// body/query, calls the user's map function, and re-serializes the output.
pub(crate) fn generate_map_wrapper(
    apigate_path: &TokenStream2,
    f: &ItemFn,
    data: &DataKind,
    map_fn: Option<&Path>,
    generated_items: &mut Vec<Item>,
) -> Result<TokenStream2> {
    let Some(map_fn) = map_fn else {
        return Ok(quote!(None));
    };

    let mode = resolve_map_mode(data, &f.sig.ident)?;
    let wrapper_ident = format_ident!("__apigate_map_{}", f.sig.ident);

    let item = match mode {
        MapMode::Json(ty) => json_map_item(apigate_path, &wrapper_ident, ty, map_fn),
        MapMode::Query(ty) => query_map_item(apigate_path, &wrapper_ident, ty, map_fn),
        MapMode::Form(ty) => form_map_item(apigate_path, &wrapper_ident, ty, map_fn),
    };

    generated_items.push(item);
    Ok(quote!(Some(#wrapper_ident as #apigate_path::MapFn)))
}

// ---------------------------------------------------------------------------
// Item emitters
// ---------------------------------------------------------------------------

fn before_item(apigate_path: &TokenStream2, before_ident: &Ident, hooks: &[Path]) -> Item {
    let hook_calls = hooks
        .iter()
        .map(|hook_path| quote!(#hook_path(&mut ctx).await?;));

    syn::parse_quote! {
        #[doc(hidden)]
        fn #before_ident<'a>(
            mut ctx: #apigate_path::PartsCtx<'a>,
        ) -> #apigate_path::BeforeFuture<'a> {
            ::std::boxed::Box::pin(async move {
                #(#hook_calls)*
                Ok(())
            })
        }
    }
}

fn json_map_item(
    apigate_path: &TokenStream2,
    wrapper_ident: &Ident,
    ty: &Type,
    map_fn: &Path,
) -> Item {
    syn::parse_quote! {
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

fn query_map_item(
    apigate_path: &TokenStream2,
    wrapper_ident: &Ident,
    ty: &Type,
    map_fn: &Path,
) -> Item {
    syn::parse_quote! {
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

fn form_map_item(
    apigate_path: &TokenStream2,
    wrapper_ident: &Ident,
    ty: &Type,
    map_fn: &Path,
) -> Item {
    syn::parse_quote! {
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
