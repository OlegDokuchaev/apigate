use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::{FnArg, ItemFn, Pat, Path, PathArguments, Stmt, Type, TypePath, TypeReference};

use crate::apigate_crate_path;

// ---------------------------------------------------------------------------
// Parameter classification
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum ParamKind {
    Ctx,
    Scope,
    Ref(Type),
    MutRef(Type),
    Owned,
}

#[derive(Clone)]
struct ParamPlan {
    pat: Pat,
    ty: Type,
    kind: ParamKind,
}

#[derive(Clone)]
struct SpecialPaths {
    ctx: Path,
    scope: Path,
}

impl SpecialPaths {
    fn new(apigate: &TokenStream2) -> syn::Result<Self> {
        Ok(Self {
            ctx: syn::parse2(quote!(#apigate::PartsCtx))?,
            scope: syn::parse2(quote!(#apigate::RequestScope))?,
        })
    }
}

fn classify_param(ty: &Type, special: &SpecialPaths) -> syn::Result<ParamKind> {
    let ty = peel_type(ty);

    match ty {
        Type::Reference(r) => classify_ref_param(ty, r, special),
        other => {
            if is_special_type(other, &special.ctx) {
                return Err(syn::Error::new(
                    other.span(),
                    "`PartsCtx` parameter must be `&mut PartsCtx`",
                ));
            }

            if is_special_type(other, &special.scope) {
                return Err(syn::Error::new(
                    other.span(),
                    "`RequestScope` parameter must be `&mut RequestScope`",
                ));
            }

            Ok(ParamKind::Owned)
        }
    }
}

fn classify_ref_param(
    original_ty: &Type,
    r: &TypeReference,
    special: &SpecialPaths,
) -> syn::Result<ParamKind> {
    let inner = peel_type(&r.elem);

    if is_special_type(inner, &special.ctx) {
        return if r.mutability.is_some() {
            Ok(ParamKind::Ctx)
        } else {
            Err(syn::Error::new(
                original_ty.span(),
                "`PartsCtx` parameter must be `&mut PartsCtx`",
            ))
        };
    }

    if is_special_type(inner, &special.scope) {
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
        Ok(ParamKind::MutRef(inner.clone()))
    } else {
        Ok(ParamKind::Ref(inner.clone()))
    }
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

/// Checks whether `ty` refers to the type at `expected` path.
/// Derives the bare ident from `expected` itself — no separate string needed.
fn is_special_type(ty: &Type, expected: &Path) -> bool {
    let ty = peel_type(ty);

    let Type::Path(TypePath {
        qself: None,
        ref path,
    }) = *ty
    else {
        return false;
    };

    // Bare ident: user wrote just `PartsCtx` / `RequestScope` without path prefix.
    // The ident is derived from the last segment of `expected`.
    let last_ident = &expected.segments.last().unwrap().ident;
    if path.is_ident(last_ident) {
        return true;
    }

    // Full path: `apigate::PartsCtx`, `crate::PartsCtx`, etc.
    path_eq_ignoring_args(path, expected)
}

fn path_eq_ignoring_args(a: &Path, b: &Path) -> bool {
    if a.segments.len() != b.segments.len() {
        return false;
    }

    a.segments.iter().zip(b.segments.iter()).all(|(lhs, rhs)| {
        lhs.ident == rhs.ident && !matches!(lhs.arguments, PathArguments::Parenthesized(_))
    })
}

// ---------------------------------------------------------------------------
// Validation
// ---------------------------------------------------------------------------

#[derive(Default)]
struct BorrowSummary {
    seen_ctx: usize,
    seen_scope: usize,
    immut_borrowed_from_scope: usize,
    mut_borrowed_from_scope: usize,
}

fn validate_plans(plans: &[ParamPlan], sig: &syn::Signature, macro_name: &str) -> syn::Result<()> {
    let mut s = BorrowSummary::default();

    for plan in plans {
        match &plan.kind {
            ParamKind::Ctx => s.seen_ctx += 1,
            ParamKind::Scope => s.seen_scope += 1,
            ParamKind::Ref(_) => s.immut_borrowed_from_scope += 1,
            ParamKind::MutRef(_) => s.mut_borrowed_from_scope += 1,
            ParamKind::Owned => {}
        }
    }

    if s.seen_ctx > 1 {
        return Err(syn::Error::new_spanned(
            sig,
            format!("only one `&mut PartsCtx` parameter is allowed in #[apigate::{macro_name}]"),
        ));
    }

    if s.seen_scope > 1 {
        return Err(syn::Error::new_spanned(
            sig,
            format!(
                "only one `&mut RequestScope` parameter is allowed in #[apigate::{macro_name}]"
            ),
        ));
    }

    if s.mut_borrowed_from_scope > 1 {
        return Err(syn::Error::new_spanned(
            sig,
            format!("#[apigate::{macro_name}] supports at most one extracted `&mut T` parameter"),
        ));
    }

    if s.seen_scope > 0 && (s.immut_borrowed_from_scope > 0 || s.mut_borrowed_from_scope > 0) {
        return Err(syn::Error::new_spanned(
            sig,
            format!(
                "`&mut RequestScope` cannot be combined with borrowed `&T` or `&mut T` parameters in #[apigate::{macro_name}]"
            ),
        ));
    }

    if s.mut_borrowed_from_scope > 0 && s.immut_borrowed_from_scope > 0 {
        return Err(syn::Error::new_spanned(
            sig,
            format!(
                "borrowed `&T` and extracted `&mut T` cannot be combined in #[apigate::{macro_name}]"
            ),
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Codegen helpers
// ---------------------------------------------------------------------------

fn missing_from_scope_tokens(apigate: &TokenStream2, ty: &Type) -> TokenStream2 {
    quote! {
        #apigate::ApigateError::internal(
            concat!("missing ", stringify!(#ty), " in request scope")
        )
    }
}

fn build_param_plans(
    f: &ItemFn,
    special: &SpecialPaths,
    keep_first_value: bool,
) -> syn::Result<(Vec<ParamPlan>, Option<FnArg>)> {
    let mut plans = Vec::new();
    let mut kept_param: Option<FnArg> = None;
    let mut found_first_value = false;

    for param in f.sig.inputs.iter() {
        let FnArg::Typed(pt) = param else {
            return Err(syn::Error::new_spanned(
                param,
                "attribute macro does not support `self`",
            ));
        };

        let pat = (*pt.pat).clone();
        let ty = (*pt.ty).clone();
        let kind = classify_param(&ty, special)?;

        match &kind {
            ParamKind::Owned if keep_first_value && !found_first_value => {
                found_first_value = true;
                kept_param = Some(param.clone());
            }
            _ => {
                plans.push(ParamPlan { pat, ty, kind });
            }
        }
    }

    Ok((plans, kept_param))
}

fn build_bindings(
    plans: &[ParamPlan],
    apigate: &TokenStream2,
    ctx_ident: &syn::Ident,
    scope_ident: &syn::Ident,
) -> syn::Result<Vec<Stmt>> {
    let mut ctx_tokens = Vec::<TokenStream2>::new();
    let mut take_tokens = Vec::<TokenStream2>::new();
    let mut get_tokens = Vec::<TokenStream2>::new();
    let mut get_mut_tokens = Vec::<TokenStream2>::new();
    let mut scope_tokens = Vec::<TokenStream2>::new();

    for plan in plans {
        let pat = &plan.pat;
        let ty = &plan.ty;

        match &plan.kind {
            ParamKind::Ctx => {
                ctx_tokens.push(quote! {
                    let #pat: #ty = #ctx_ident;
                });
            }
            ParamKind::Scope => {
                scope_tokens.push(quote! {
                    let #pat: #ty = #scope_ident;
                });
            }
            ParamKind::Owned => {
                let err = missing_from_scope_tokens(apigate, ty);
                take_tokens.push(quote! {
                    let #pat: #ty = #scope_ident.take::<#ty>()
                        .ok_or_else(|| #err)?;
                });
            }
            ParamKind::Ref(inner_ty) => {
                let err = missing_from_scope_tokens(apigate, inner_ty);
                get_tokens.push(quote! {
                    let #pat: #ty = #scope_ident.get::<#inner_ty>()
                        .ok_or_else(|| #err)?;
                });
            }
            ParamKind::MutRef(inner_ty) => {
                let err = missing_from_scope_tokens(apigate, inner_ty);
                get_mut_tokens.push(quote! {
                    let #pat: #ty = #scope_ident.get_mut::<#inner_ty>()
                        .ok_or_else(|| #err)?;
                });
            }
        }
    }

    let mut stmts = Vec::new();
    for tokens in ctx_tokens
        .iter()
        .chain(take_tokens.iter())
        .chain(get_tokens.iter())
        .chain(get_mut_tokens.iter())
        .chain(scope_tokens.iter())
    {
        stmts.push(syn::parse2(tokens.clone())?);
    }

    Ok(stmts)
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
    let special = SpecialPaths::new(&apigate)?;

    if f.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &f.sig,
            format!("#[apigate::{macro_name}] requires an async function"),
        ));
    }

    let ctx_ident = format_ident!("__apigate_ctx");
    let scope_ident = format_ident!("__apigate_scope");

    let (plans, kept_param) = build_param_plans(&f, &special, keep_first_value)?;
    validate_plans(&plans, &f.sig, macro_name)?;

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
        .push(syn::parse_quote!(#scope_ident: &mut #apigate::RequestScope<'_>));

    let generated_stmts = build_bindings(&plans, &apigate, &ctx_ident, &scope_ident)?;
    if !generated_stmts.is_empty() {
        let original = std::mem::take(&mut f.block.stmts);
        f.block.stmts = generated_stmts;
        f.block.stmts.extend(original);
    }

    Ok(quote!(#f))
}
