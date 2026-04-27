use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Ident, Item, ItemFn, Path, Result, Type};

use crate::route::DataKind;

/// Generates a single `__apigate_pipeline_<fn>` that orchestrates:
/// 1. before hooks (with ctx + scope)
/// 2. parse/validate body (always if data type declared)
/// 3. optional map (reads parsed input, writes transformed body)
pub(crate) fn generate_pipeline_wrapper(
    apigate_path: &TokenStream2,
    f: &ItemFn,
    hooks: &[Path],
    data: &DataKind,
    map_fn: Option<&Path>,
    path_type: Option<&Type>,
    generated_items: &mut Vec<Item>,
) -> Result<TokenStream2> {
    let has_hooks = !hooks.is_empty();
    let has_body = matches!(
        data,
        DataKind::Json(_) | DataKind::Query(_) | DataKind::Form(_)
    );
    let has_path = path_type.is_some();

    if !has_hooks && !has_body && map_fn.is_none() && !has_path {
        return Ok(quote!(None));
    }

    let pipeline_ident = format_ident!("__apigate_pipeline_{}", f.sig.ident);

    let path_phase = build_path_phase(path_type);
    let hook_phase = if has_hooks {
        let calls = hooks
            .iter()
            .map(|h| quote!(#h(&mut ctx, &mut scope).await?;));
        quote!(#(#calls)*)
    } else {
        quote!()
    };
    let body_phase = build_body_phase(apigate_path, data, map_fn, &f.sig.ident)?;

    let item: Item = syn::parse_quote! {
        #[doc(hidden)]
        fn #pipeline_ident<'a>(
            mut ctx: #apigate_path::PartsCtx<'a>,
            mut scope: #apigate_path::RequestScope<'a>,
        ) -> #apigate_path::PipelineFuture<'a> {
            ::std::boxed::Box::pin(async move {
                #path_phase
                #hook_phase
                #body_phase
            })
        }
    };

    generated_items.push(item);
    Ok(quote!(Some(#pipeline_ident as #apigate_path::PipelineFn)))
}

// ---------------------------------------------------------------------------
// Path phase
// ---------------------------------------------------------------------------

fn build_path_phase(path_type: Option<&Type>) -> TokenStream2 {
    match path_type {
        Some(ty) => quote! {
            let __apigate_path_value: #ty = ctx.extract_path::<#ty>().await?;
            scope.insert(__apigate_path_value);
        },
        None => quote!(),
    }
}

// ---------------------------------------------------------------------------
// Body phase dispatch
// ---------------------------------------------------------------------------

fn build_body_phase(
    apigate_path: &TokenStream2,
    data: &DataKind,
    map_fn: Option<&Path>,
    route_fn_ident: &Ident,
) -> Result<TokenStream2> {
    match (map_fn, data) {
        (None, DataKind::None | DataKind::Multipart) => {
            let take = take_body_expr(apigate_path);
            Ok(quote!(#take))
        }
        (Some(_), DataKind::Multipart) => Err(syn::Error::new_spanned(
            route_fn_ident,
            "`map` is not supported with `multipart`",
        )),
        (Some(_), DataKind::None) => Err(syn::Error::new_spanned(
            route_fn_ident,
            "`map` requires one of `query = T`, `json = T`, or `form = T`",
        )),

        (map_fn, DataKind::Json(ty)) => Ok(json_phase(apigate_path, ty, map_fn)),
        (map_fn, DataKind::Query(ty)) => Ok(query_phase(apigate_path, ty, map_fn)),
        (map_fn, DataKind::Form(ty)) => Ok(form_phase(apigate_path, ty, map_fn)),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `scope.take_body().ok_or_else(...)` returns `Result<Body>`.
fn take_body_expr(apigate_path: &TokenStream2) -> TokenStream2 {
    quote! {
        scope.take_body()
            .ok_or_else(|| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::RequestBodyAlreadyConsumed))
    }
}

/// `let body = scope.take_body().ok_or_else(...)?;` unwraps into `Body`.
fn take_body_let(apigate_path: &TokenStream2) -> TokenStream2 {
    let take = take_body_expr(apigate_path);
    quote!(let body = #take?;)
}

/// Read body bytes: take body + to_bytes.
fn read_body_bytes(apigate_path: &TokenStream2) -> TokenStream2 {
    let take = take_body_let(apigate_path);
    quote! {
        #take
        let limit = scope.body_limit();
        let bytes = #apigate_path::__private::axum::body::to_bytes(body, limit)
            .await
            .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::RequestBodyTooLarge(err.to_string())))?;
    }
}

// ---------------------------------------------------------------------------
// JSON phase
// ---------------------------------------------------------------------------

fn json_phase(apigate_path: &TokenStream2, ty: &Type, map_fn: Option<&Path>) -> TokenStream2 {
    let read = read_body_bytes(apigate_path);

    match map_fn {
        Some(map_fn) => quote! {
            #read
            let input: #ty = #apigate_path::__private::serde_json::from_slice(&bytes)
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::InvalidJsonBody(err.to_string())))?;
            let output = #map_fn(input, &mut ctx, &mut scope).await?;
            let new_body = #apigate_path::__private::serde_json::to_vec(&output)
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::FailedSerializeMappedJson(err.to_string())))?;
            ctx.headers_mut().insert(
                #apigate_path::__private::http::header::CONTENT_TYPE,
                #apigate_path::__private::http::HeaderValue::from_static("application/json"),
            );
            ctx.headers_mut().remove(#apigate_path::__private::http::header::CONTENT_LENGTH);
            Ok(#apigate_path::__private::axum::body::Body::from(new_body))
        },
        None => quote! {
            #read
            let _: #ty = #apigate_path::__private::serde_json::from_slice(&bytes)
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::InvalidJsonBody(err.to_string())))?;
            Ok(#apigate_path::__private::axum::body::Body::from(bytes))
        },
    }
}

// ---------------------------------------------------------------------------
// Query phase
// ---------------------------------------------------------------------------

fn query_phase(apigate_path: &TokenStream2, ty: &Type, map_fn: Option<&Path>) -> TokenStream2 {
    let take = take_body_expr(apigate_path);

    match map_fn {
        Some(map_fn) => quote! {
            let input: #ty = #apigate_path::__private::axum::extract::Query::<#ty>::try_from_uri(ctx.uri())
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::InvalidQuery(err.to_string())))?
                .0;
            let output = #map_fn(input, &mut ctx, &mut scope).await?;
            let encoded = #apigate_path::__private::serde_urlencoded::to_string(&output)
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::FailedSerializeMappedQuery(err.to_string())))?;
            let path = ctx.uri().path().to_string();
            let mut path_and_query = path;
            if !encoded.is_empty() {
                path_and_query.push('?');
                path_and_query.push_str(&encoded);
            }
            *ctx.uri_mut() = #apigate_path::__private::http::Uri::builder()
                .path_and_query(path_and_query)
                .build()
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::FailedRebuildUri(err.to_string())))?;
            #take
        },
        None => quote! {
            let _: #ty = #apigate_path::__private::axum::extract::Query::<#ty>::try_from_uri(ctx.uri())
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::InvalidQuery(err.to_string())))?
                .0;
            #take
        },
    }
}

// ---------------------------------------------------------------------------
// Form phase
// ---------------------------------------------------------------------------

fn form_phase(apigate_path: &TokenStream2, ty: &Type, map_fn: Option<&Path>) -> TokenStream2 {
    let take = take_body_expr(apigate_path);

    let get_branch = form_get_branch(apigate_path, ty, map_fn, &take);
    let post_branch = form_post_branch(apigate_path, ty, map_fn);

    quote! {
        let method = ctx.method().clone();
        if method == #apigate_path::__private::http::Method::GET
            || method == #apigate_path::__private::http::Method::HEAD
        {
            #get_branch
        } else {
            let content_type = ctx
                .headers()
                .get(#apigate_path::__private::http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or_default();
            if !content_type.starts_with("application/x-www-form-urlencoded") {
                return Err(#apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::ExpectedFormUrlEncoded));
            }
            #post_branch
        }
    }
}

fn form_get_branch(
    apigate_path: &TokenStream2,
    ty: &Type,
    map_fn: Option<&Path>,
    take: &TokenStream2,
) -> TokenStream2 {
    match map_fn {
        Some(map_fn) => quote! {
            let raw = ctx.uri().query().unwrap_or_default();
            let input: #ty = #apigate_path::__private::serde_urlencoded::from_str(raw)
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::InvalidFormQuery(err.to_string())))?;
            let output = #map_fn(input, &mut ctx, &mut scope).await?;
            let encoded = #apigate_path::__private::serde_urlencoded::to_string(&output)
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::FailedSerializeMappedForm(err.to_string())))?;
            let path = ctx.uri().path().to_string();
            let mut path_and_query = path;
            if !encoded.is_empty() {
                path_and_query.push('?');
                path_and_query.push_str(&encoded);
            }
            *ctx.uri_mut() = #apigate_path::__private::http::Uri::builder()
                .path_and_query(path_and_query)
                .build()
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::FailedRebuildUri(err.to_string())))?;
            #take
        },
        None => quote! {
            let raw = ctx.uri().query().unwrap_or_default();
            let _: #ty = #apigate_path::__private::serde_urlencoded::from_str(raw)
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::InvalidFormQuery(err.to_string())))?;
            #take
        },
    }
}

fn form_post_branch(apigate_path: &TokenStream2, ty: &Type, map_fn: Option<&Path>) -> TokenStream2 {
    let read = read_body_bytes(apigate_path);

    match map_fn {
        Some(map_fn) => quote! {
            #read
            let input: #ty = #apigate_path::__private::serde_urlencoded::from_bytes(&bytes)
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::InvalidFormBody(err.to_string())))?;
            let output = #map_fn(input, &mut ctx, &mut scope).await?;
            let encoded = #apigate_path::__private::serde_urlencoded::to_string(&output)
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::FailedSerializeMappedForm(err.to_string())))?;
            ctx.headers_mut().insert(
                #apigate_path::__private::http::header::CONTENT_TYPE,
                #apigate_path::__private::http::HeaderValue::from_static("application/x-www-form-urlencoded"),
            );
            ctx.headers_mut().remove(#apigate_path::__private::http::header::CONTENT_LENGTH);
            Ok(#apigate_path::__private::axum::body::Body::from(encoded))
        },
        None => quote! {
            #read
            let _: #ty = #apigate_path::__private::serde_urlencoded::from_bytes(&bytes)
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::InvalidFormBody(err.to_string())))?;
            Ok(#apigate_path::__private::axum::body::Body::from(bytes))
        },
    }
}
