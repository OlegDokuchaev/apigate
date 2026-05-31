use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{Ident, Item, ItemFn, Path, Result, Type};

use crate::route::DataKind;

/// Generates a single `__apigate_pipeline_<fn>` that orchestrates:
/// 1. parse path/query params into scope
/// 2. before hooks (with ctx + scope)
/// 3. parse/validate body (always if data type declared)
/// 4. optional map (reads parsed input, writes transformed body)
#[allow(clippy::too_many_arguments)]
pub(crate) fn generate_pipeline_wrapper(
    apigate_path: &TokenStream2,
    f: &ItemFn,
    hooks: &[Path],
    data: &DataKind,
    map_fn: Option<&Path>,
    path_type: Option<&Type>,
    query_type: Option<&Type>,
    body_in_query: bool,
    generated_items: &mut Vec<Item>,
) -> Result<TokenStream2> {
    let has_hooks = !hooks.is_empty();
    let has_body = matches!(data, DataKind::Json(_) | DataKind::Form(_));
    let has_path = path_type.is_some();
    let has_query = query_type.is_some();

    if !has_hooks && !has_body && map_fn.is_none() && !has_path && !has_query {
        return Ok(quote!(None));
    }

    let pipeline_ident = format_ident!("__apigate_pipeline_{}", f.sig.ident);

    let path_phase = build_path_phase(path_type);
    let query_phase = build_query_phase(query_type);
    let hook_phase = if has_hooks {
        let calls = hooks
            .iter()
            .map(|h| quote!(#h(&mut ctx, &mut scope).await?;));
        quote!(#(#calls)*)
    } else {
        quote!()
    };
    let body_phase = build_body_phase(apigate_path, data, map_fn, &f.sig.ident, body_in_query)?;

    let item: Item = syn::parse_quote! {
        #[doc(hidden)]
        fn #pipeline_ident<'a>(
            mut ctx: #apigate_path::PartsCtx<'a>,
            mut scope: #apigate_path::RequestScope<'a>,
        ) -> #apigate_path::PipelineFuture<'a> {
            ::std::boxed::Box::pin(async move {
                #path_phase
                #query_phase
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
// Query phase
// ---------------------------------------------------------------------------

fn build_query_phase(query_type: Option<&Type>) -> TokenStream2 {
    match query_type {
        Some(ty) => quote! {
            let __apigate_query_value: #ty = ctx.extract_query::<#ty>()?;
            scope.insert(__apigate_query_value);
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
    body_in_query: bool,
) -> Result<TokenStream2> {
    match (map_fn, data) {
        (None, DataKind::None | DataKind::Multipart) => Ok(take_body_expr(apigate_path)),
        (Some(_), DataKind::Multipart) => Err(syn::Error::new_spanned(
            route_fn_ident,
            "`map` is not supported with `multipart`",
        )),
        (Some(map_fn), DataKind::None) => Ok(none_map_phase(apigate_path, map_fn)),
        (map_fn, DataKind::Json(ty)) => Ok(json_phase(apigate_path, ty, map_fn)),
        (map_fn, DataKind::Form(ty)) => Ok(form_phase(apigate_path, ty, map_fn, body_in_query)),
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

/// Shared tail for *every* mapped phase
fn map_replace_or_keep(
    apigate_path: &TokenStream2,
    map_fn: &Path,
    format: &TokenStream2,
    input_expr: &TokenStream2,
    content_type: Option<&str>,
) -> TokenStream2 {
    let set_content_type = match content_type {
        Some(content_type) => quote! {
            ctx.headers_mut().insert(
                #apigate_path::__private::http::header::CONTENT_TYPE,
                #apigate_path::__private::http::HeaderValue::from_static(#content_type),
            );
        },
        None => quote!(),
    };
    quote! {
        match #map_fn::<#format>(#input_expr, &mut ctx, &mut scope).await? {
            #apigate_path::__private::BodyOutcome::Replace(new_body) => {
                #set_content_type
                ctx.headers_mut().remove(#apigate_path::__private::http::header::CONTENT_LENGTH);
                Ok(#apigate_path::__private::axum::body::Body::from(new_body))
            }
            #apigate_path::__private::BodyOutcome::Keep => Ok(#apigate_path::__private::axum::body::Body::from(bytes)),
        }
    }
}

// ---------------------------------------------------------------------------
// Raw body map phase (no `json`/`form` data)
// ---------------------------------------------------------------------------

fn none_map_phase(apigate_path: &TokenStream2, map_fn: &Path) -> TokenStream2 {
    let format = quote!(#apigate_path::__private::Raw);
    let apply = map_replace_or_keep(apigate_path, map_fn, &format, &quote!(raw), None);
    quote! {
        let bytes = scope.read_body_bytes().await?;
        let raw = scope.raw_body_cloned().ok_or_else(|| #apigate_path::ApigateError::from(
            #apigate_path::ApigatePipelineError::MissingFromScope("RawBody")))?;
        #apply
    }
}

// ---------------------------------------------------------------------------
// JSON phase
// ---------------------------------------------------------------------------

fn json_phase(apigate_path: &TokenStream2, ty: &Type, map_fn: Option<&Path>) -> TokenStream2 {
    match map_fn {
        Some(map_fn) => {
            let format = quote!(#apigate_path::__private::Json);
            let apply = map_replace_or_keep(
                apigate_path,
                map_fn,
                &format,
                &quote!(input),
                Some("application/json"),
            );
            quote! {
                let bytes = scope.read_body_bytes().await?;
                let input: #ty = #apigate_path::__private::serde_json::from_slice(&bytes)
                    .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::InvalidJsonBody(err.to_string())))?;
                #apply
            }
        }
        None => quote! {
            let bytes = scope.read_body_bytes().await?;
            let _: #ty = #apigate_path::__private::serde_json::from_slice(&bytes)
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::InvalidJsonBody(err.to_string())))?;
            Ok(#apigate_path::__private::axum::body::Body::from(bytes))
        },
    }
}

// ---------------------------------------------------------------------------
// Form phase
// ---------------------------------------------------------------------------

fn form_phase(
    apigate_path: &TokenStream2,
    ty: &Type,
    map_fn: Option<&Path>,
    body_in_query: bool,
) -> TokenStream2 {
    if body_in_query {
        form_get_branch(apigate_path, ty, map_fn)
    } else {
        let post_branch = form_post_branch(apigate_path, ty, map_fn);
        quote! {
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

fn form_get_branch(apigate_path: &TokenStream2, ty: &Type, map_fn: Option<&Path>) -> TokenStream2 {
    let take = take_body_expr(apigate_path);
    match map_fn {
        Some(map_fn) => quote! {
            let raw = ctx.uri().query().unwrap_or_default();
            let input: #ty = #apigate_path::__private::serde_urlencoded::from_str(raw)
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::InvalidFormQuery(err.to_string())))?;
            if let #apigate_path::__private::BodyOutcome::Replace(encoded) =
                #map_fn::<#apigate_path::__private::Form>(input, &mut ctx, &mut scope).await?
            {
                ctx.set_encoded_query(&encoded)?;
            }
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
    match map_fn {
        Some(map_fn) => {
            let format = quote!(#apigate_path::__private::Form);
            let apply = map_replace_or_keep(
                apigate_path,
                map_fn,
                &format,
                &quote!(input),
                Some("application/x-www-form-urlencoded"),
            );
            quote! {
                let bytes = scope.read_body_bytes().await?;
                let input: #ty = #apigate_path::__private::serde_urlencoded::from_bytes(&bytes)
                    .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::InvalidFormBody(err.to_string())))?;
                #apply
            }
        }
        None => quote! {
            let bytes = scope.read_body_bytes().await?;
            let _: #ty = #apigate_path::__private::serde_urlencoded::from_bytes(&bytes)
                .map_err(|err| #apigate_path::ApigateError::from(#apigate_path::ApigatePipelineError::InvalidFormBody(err.to_string())))?;
            Ok(#apigate_path::__private::axum::body::Body::from(bytes))
        },
    }
}
