use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{FnArg, ItemFn, Type, TypeReference};

use crate::apigate_crate_path;

// ---------------------------------------------------------------------------
// Parameter classification
// ---------------------------------------------------------------------------

enum ParamKind<'a> {
    Ctx,
    Scope,
    Ref(&'a Type),
    Owned,
}

fn classify_param(ty: &Type) -> syn::Result<ParamKind<'_>> {
    let ty = peel_type(ty);

    match ty {
        Type::Reference(r) => classify_ref_param(ty, r),
        other => {
            if type_ends_with(other, "PartsCtx") {
                return Err(syn::Error::new(
                    other.span(),
                    "`PartsCtx` parameter must be `&mut PartsCtx<'_>`",
                ));
            }

            if type_ends_with(other, "RequestScope") {
                return Err(syn::Error::new(
                    other.span(),
                    "`RequestScope` parameter must be `&mut RequestScope`",
                ));
            }

            Ok(ParamKind::Owned)
        }
    }
}

fn classify_ref_param<'a>(
    original_ty: &'a Type,
    r: &'a TypeReference,
) -> syn::Result<ParamKind<'a>> {
    let inner = peel_type(&r.elem);

    if type_ends_with(inner, "PartsCtx") {
        return if r.mutability.is_some() {
            Ok(ParamKind::Ctx)
        } else {
            Err(syn::Error::new(
                original_ty.span(),
                "`PartsCtx` parameter must be `&mut PartsCtx<'_>`",
            ))
        };
    }

    if type_ends_with(inner, "RequestScope") {
        return if r.mutability.is_some() {
            Ok(ParamKind::Scope)
        } else {
            Err(syn::Error::new(
                original_ty.span(),
                "`RequestScope` parameter must be `&mut RequestScope`",
            ))
        };
    }

    if r.mutability.is_some() {
        return Err(syn::Error::new(
            original_ty.span(),
            "only `&T`, `&mut PartsCtx<'_>`, and `&mut RequestScope` are supported",
        ));
    }

    Ok(ParamKind::Ref(inner))
}

fn peel_type(mut ty: &Type) -> &Type {
    loop {
        match ty {
            Type::Group(g) => ty = &g.elem,
            Type::Paren(p) => ty = &p.elem,
            _ => return ty,
        }
    }
}

fn type_ends_with(ty: &Type, name: &str) -> bool {
    let ty = peel_type(ty);

    match ty {
        Type::Path(p) => p
            .path
            .segments
            .last()
            .map(|s| s.ident == name)
            .unwrap_or(false),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// expand_fn_params
// ---------------------------------------------------------------------------

/// Shared expansion for `#[apigate::hook]` and `#[apigate::map]`.
///
/// Rewrites function signature to canonical form.
/// - hook (`keep_first_value = false`): all value params extracted from scope
/// - map  (`keep_first_value = true`):  first owned param kept as direct input,
///   rest extracted from scope
pub(crate) fn expand_fn_params(
    input: TokenStream,
    macro_name: &str,
    keep_first_value: bool,
) -> syn::Result<TokenStream2> {
    let mut f = syn::parse::<ItemFn>(input)?;
    let apigate = apigate_crate_path()?;

    if f.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &f.sig,
            format!("#[apigate::{macro_name}] requires an async function"),
        ));
    }

    let ctx_ident = format_ident!("__apigate_ctx");
    let scope_ident = format_ident!("__apigate_scope");

    let mut ctx_bind_tokens: Vec<TokenStream2> = Vec::new();
    let mut scope_bind_tokens: Vec<TokenStream2> = Vec::new();
    let mut take_tokens: Vec<TokenStream2> = Vec::new();
    let mut get_tokens: Vec<TokenStream2> = Vec::new();

    let mut kept_param: Option<syn::FnArg> = None;
    let mut found_first_value = false;

    let mut seen_ctx = false;
    let mut seen_scope = false;
    let mut has_borrowed_scope_values = false;

    for param in f.sig.inputs.iter() {
        let FnArg::Typed(pt) = param else {
            return Err(syn::Error::new_spanned(
                param,
                format!("#[apigate::{macro_name}] does not support `self`"),
            ));
        };

        let pat = &*pt.pat;
        let ty = &*pt.ty;

        match classify_param(ty)? {
            ParamKind::Ctx => {
                if seen_ctx {
                    return Err(syn::Error::new_spanned(
                        ty,
                        format!(
                            "only one `&mut PartsCtx<'_>` parameter is allowed in #[apigate::{macro_name}]"
                        ),
                    ));
                }
                seen_ctx = true;

                ctx_bind_tokens.push(quote! {
                    let #pat: #ty = #ctx_ident;
                });
            }
            ParamKind::Scope => {
                if seen_scope {
                    return Err(syn::Error::new_spanned(
                        ty,
                        format!(
                            "only one `&mut RequestScope` parameter is allowed in #[apigate::{macro_name}]"
                        ),
                    ));
                }
                seen_scope = true;

                scope_bind_tokens.push(quote! {
                    let #pat: #ty = #scope_ident;
                });
            }
            ParamKind::Ref(inner_ty) => {
                has_borrowed_scope_values = true;

                get_tokens.push(quote! {
                    let #pat: #ty = #scope_ident.get::<#inner_ty>()
                        .ok_or_else(|| #apigate::ApigateError::internal(
                            concat!("missing ", stringify!(#inner_ty), " in request scope")
                        ))?;
                });
            }
            ParamKind::Owned => {
                if keep_first_value && !found_first_value {
                    found_first_value = true;
                    kept_param = Some(param.clone());
                } else {
                    take_tokens.push(quote! {
                        let #pat: #ty = #scope_ident.take::<#ty>()
                            .ok_or_else(|| #apigate::ApigateError::internal(
                                concat!("missing ", stringify!(#ty), " in request scope")
                            ))?;
                    });
                }
            }
        }
    }

    if seen_scope && has_borrowed_scope_values {
        return Err(syn::Error::new_spanned(
            &f.sig,
            "`&mut RequestScope` cannot be combined with borrowed `&T` parameters; \
             use owned `T` or remove the mutable scope parameter",
        ));
    }

    // Rewrite parameter list: [input,] ctx, scope
    f.sig.inputs.clear();
    if let Some(param) = kept_param {
        f.sig.inputs.push(param);
    }
    f.sig
        .inputs
        .push(syn::parse_quote!(#ctx_ident: &mut #apigate::PartsCtx<'_>));
    f.sig
        .inputs
        .push(syn::parse_quote!(#scope_ident: &mut #apigate::RequestScope));

    // Prepend extraction statements:
    // 1) ctx binding  2) takes (mut scope)  3) gets (&scope)  4) scope binding
    if !ctx_bind_tokens.is_empty()
        || !take_tokens.is_empty()
        || !get_tokens.is_empty()
        || !scope_bind_tokens.is_empty()
    {
        let mut stmts = Vec::new();

        for tokens in ctx_bind_tokens
            .iter()
            .chain(take_tokens.iter())
            .chain(get_tokens.iter())
            .chain(scope_bind_tokens.iter())
        {
            stmts.push(syn::parse2(tokens.clone())?);
        }

        let original = std::mem::take(&mut f.block.stmts);
        f.block.stmts = stmts;
        f.block.stmts.extend(original);
    }

    Ok(quote!(#f))
}
