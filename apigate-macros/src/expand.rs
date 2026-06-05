use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::visit_mut::VisitMut;
use syn::{
    FnArg, ItemFn, Lifetime, Pat, Path, PathArguments, ReturnType, Stmt, Type, TypePath,
    TypeReference, WherePredicate,
};

use crate::apigate_crate_path;

// ---------------------------------------------------------------------------
// Parameter classification
// ---------------------------------------------------------------------------

#[derive(Clone)]
enum ParamSource {
    PartsCtx,
    RequestScope,
    ScopeRef(Type),
    ScopeMut(Type),
    ScopeTake,
    RawBody,
}

impl ParamSource {
    /// Whether binding this parameter reads the generated `ctx` argument.
    fn binds_ctx(&self) -> bool {
        matches!(self, Self::PartsCtx)
    }

    /// Whether binding this parameter reads the generated `scope` argument.
    fn binds_scope(&self) -> bool {
        matches!(
            self,
            Self::RequestScope
                | Self::ScopeRef(_)
                | Self::ScopeMut(_)
                | Self::ScopeTake
                | Self::RawBody
        )
    }
}

#[derive(Clone)]
struct ParamPlan {
    pat: Pat,
    ty: Type,
    source: ParamSource,
}

#[derive(Clone)]
struct SpecialTypePaths {
    ctx: Path,
    scope: Path,
    raw_body: Path,
}

#[derive(Clone, Copy)]
enum SpecialKind {
    PartsCtx,
    RequestScope,
    RawBody,
}

impl SpecialTypePaths {
    fn new(apigate: &TokenStream2) -> syn::Result<Self> {
        Ok(Self {
            ctx: syn::parse2(quote!(#apigate::PartsCtx))?,
            scope: syn::parse2(quote!(#apigate::RequestScope))?,
            raw_body: syn::parse2(quote!(#apigate::RawBody))?,
        })
    }

    fn match_kind(&self, ty: &Type) -> Option<SpecialKind> {
        if is_special_type(ty, &self.ctx) {
            Some(SpecialKind::PartsCtx)
        } else if is_special_type(ty, &self.scope) {
            Some(SpecialKind::RequestScope)
        } else if is_special_type(ty, &self.raw_body) {
            Some(SpecialKind::RawBody)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ExpansionMode {
    /// Expand an `#[apigate::hook]` function.
    Hook,
    /// Expand an `#[apigate::map]` function.
    Map,
}

impl ExpansionMode {
    fn macro_name(self) -> &'static str {
        match self {
            Self::Hook => "hook",
            Self::Map => "map",
        }
    }

    fn keeps_first_owned_param(self) -> bool {
        matches!(self, Self::Map)
    }
}

fn classify_param(ty: &Type, special: &SpecialTypePaths) -> syn::Result<ParamSource> {
    let ty = peel_type(ty);

    match ty {
        Type::Reference(r) => classify_ref_param(ty, r, special),
        other => match special.match_kind(other) {
            Some(SpecialKind::PartsCtx) => Err(syn::Error::new(
                other.span(),
                "`PartsCtx` parameter must be `&mut PartsCtx`",
            )),
            Some(SpecialKind::RequestScope) => Err(syn::Error::new(
                other.span(),
                "`RequestScope` parameter must be `&mut RequestScope`",
            )),
            Some(SpecialKind::RawBody) => Ok(ParamSource::RawBody),
            None => Ok(ParamSource::ScopeTake),
        },
    }
}

fn classify_ref_param(
    original_ty: &Type,
    r: &TypeReference,
    special: &SpecialTypePaths,
) -> syn::Result<ParamSource> {
    let inner = peel_type(&r.elem);

    match special.match_kind(inner) {
        Some(SpecialKind::PartsCtx) => {
            if r.mutability.is_some() {
                Ok(ParamSource::PartsCtx)
            } else {
                Err(syn::Error::new(
                    original_ty.span(),
                    "`PartsCtx` parameter must be `&mut PartsCtx`",
                ))
            }
        }
        Some(SpecialKind::RequestScope) => {
            if r.mutability.is_some() {
                Ok(ParamSource::RequestScope)
            } else {
                Err(syn::Error::new(
                    original_ty.span(),
                    "`RequestScope` parameter must be `&mut RequestScope`",
                ))
            }
        }
        Some(SpecialKind::RawBody) => Err(syn::Error::new(
            original_ty.span(),
            "`RawBody` parameter must be taken by value (`RawBody`), not by reference",
        )),
        None => {
            if r.mutability.is_some() {
                Ok(ParamSource::ScopeMut(inner.clone()))
            } else {
                Ok(ParamSource::ScopeRef(inner.clone()))
            }
        }
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
/// Derives the bare ident from `expected` itself, so no separate string is needed.
fn is_special_type(ty: &Type, expected: &Path) -> bool {
    let ty = peel_type(ty);

    let Type::Path(TypePath {
        qself: None,
        ref path,
    }) = *ty
    else {
        return false;
    };

    let last_ident = &expected.segments.last().unwrap().ident;
    if is_bare_special_path(path, last_ident) {
        return true;
    }

    // Full path: `apigate::PartsCtx`, `::apigate::PartsCtx`, or renamed crate path.
    path_eq_ignoring_args(path, expected)
}

fn is_bare_special_path(path: &Path, ident: &syn::Ident) -> bool {
    path.leading_colon.is_none()
        && path.segments.len() == 1
        && path.segments[0].ident == *ident
        && !matches!(path.segments[0].arguments, PathArguments::Parenthesized(_))
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
        match &plan.source {
            ParamSource::PartsCtx => s.seen_ctx += 1,
            ParamSource::RequestScope => s.seen_scope += 1,
            ParamSource::ScopeRef(_) => s.immut_borrowed_from_scope += 1,
            ParamSource::ScopeMut(_) => s.mut_borrowed_from_scope += 1,
            ParamSource::ScopeTake | ParamSource::RawBody => {}
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
        #apigate::ApigateError::from(
            #apigate::ApigatePipelineError::MissingFromScope(stringify!(#ty))
        )
    }
}

fn build_param_plans(
    f: &ItemFn,
    special: &SpecialTypePaths,
    mode: ExpansionMode,
) -> syn::Result<(Vec<ParamPlan>, Option<FnArg>)> {
    let mut plans = Vec::new();
    let mut kept_param: Option<FnArg> = None;

    for param in f.sig.inputs.iter() {
        let FnArg::Typed(pt) = param else {
            return Err(syn::Error::new_spanned(
                param,
                "attribute macro does not support `self`",
            ));
        };

        let pat = (*pt.pat).clone();
        let ty = (*pt.ty).clone();
        let source = classify_param(&ty, special)?;

        if matches!(source, ParamSource::RawBody) && !matches!(mode, ExpansionMode::Map) {
            return Err(syn::Error::new_spanned(
                param,
                "`RawBody` is only available in #[apigate::map]",
            ));
        }

        match &source {
            ParamSource::ScopeTake | ParamSource::RawBody
                if mode.keeps_first_owned_param() && kept_param.is_none() =>
            {
                kept_param = Some(param.clone());
            }
            _ => {
                plans.push(ParamPlan { pat, ty, source });
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
    let mut raw_tokens = Vec::<TokenStream2>::new();
    let mut ctx_tokens = Vec::<TokenStream2>::new();
    let mut take_tokens = Vec::<TokenStream2>::new();
    let mut get_tokens = Vec::<TokenStream2>::new();
    let mut get_mut_tokens = Vec::<TokenStream2>::new();
    let mut scope_tokens = Vec::<TokenStream2>::new();

    for plan in plans {
        let pat = &plan.pat;
        let ty = &plan.ty;

        match &plan.source {
            ParamSource::PartsCtx => {
                ctx_tokens.push(quote! {
                    let #pat: #ty = #ctx_ident;
                });
            }
            ParamSource::RequestScope => {
                scope_tokens.push(quote! {
                    let #pat: #ty = #scope_ident;
                });
            }
            ParamSource::ScopeTake => {
                let err = missing_from_scope_tokens(apigate, ty);
                take_tokens.push(quote! {
                    let #pat: #ty = #scope_ident.take::<#ty>()
                        .ok_or_else(|| #err)?;
                });
            }
            ParamSource::ScopeRef(inner_ty) => {
                let err = missing_from_scope_tokens(apigate, inner_ty);
                get_tokens.push(quote! {
                    let #pat: #ty = #scope_ident.get::<#inner_ty>()
                        .ok_or_else(|| #err)?;
                });
            }
            ParamSource::ScopeMut(inner_ty) => {
                let err = missing_from_scope_tokens(apigate, inner_ty);
                get_mut_tokens.push(quote! {
                    let #pat: #ty = #scope_ident.get_mut::<#inner_ty>()
                        .ok_or_else(|| #err)?;
                });
            }
            ParamSource::RawBody => {
                let err = missing_from_scope_tokens(apigate, ty);
                raw_tokens.push(quote! {
                    let #pat: #ty = #scope_ident.raw_body_cloned().ok_or_else(|| #err)?;
                });
            }
        }
    }

    let mut stmts = Vec::new();
    for tokens in raw_tokens
        .iter()
        .chain(ctx_tokens.iter())
        .chain(take_tokens.iter())
        .chain(get_tokens.iter())
        .chain(get_mut_tokens.iter())
        .chain(scope_tokens.iter())
    {
        stmts.push(syn::parse2(tokens.clone())?);
    }

    Ok(stmts)
}

/// Runs the user body in a non-`move` `async` block and yields its `MapResult` output.
fn run_user_body(apigate: &TokenStream2, original: &[Stmt]) -> TokenStream2 {
    quote! {
        {
            let __apigate_result: #apigate::MapResult<_> = async { #(#original)* }.await;
            __apigate_result
        }?
    }
}

/// Wrapper body for *every* map. Run the user body, then dispatch its output
/// through the autoref marker [`Finisher`]
fn finish_block(apigate: &TokenStream2, original: Vec<Stmt>) -> syn::Block {
    let run = run_user_body(apigate, &original);
    syn::parse_quote!({
        let __apigate_output = #run;
        let __apigate_payload = {
            use #apigate::__private::Finish as _;
            #apigate::__private::Finisher::<__apigate_format, _>::for_output(&__apigate_output)
                .apigate_finish(__apigate_output)?
        };
        ::std::result::Result::Ok(#apigate::__private::BodyOutcome::from(__apigate_payload))
    })
}

/// Extracts a map's declared return type for the [`finish_predicate`] bound.
fn map_return_type(output: &ReturnType) -> syn::Result<Type> {
    match output {
        ReturnType::Type(_, ty) => Ok((**ty).clone()),
        ReturnType::Default => Err(syn::Error::new_spanned(
            output,
            "#[apigate::map] must return `apigate::MapResult<T>`",
        )),
    }
}

/// Rewrites every non-`'static` lifetime in a type to a fresh HRTB binder,
/// collecting the binders. Turns a borrowing output `Out<'_>` into `Out<'__apigate_o0>`
/// plus the binder list, which [`finish_predicate`] quantifies in the `Finish` bound.
struct HrtbRewriter {
    binders: Vec<Lifetime>,
}

impl HrtbRewriter {
    fn fresh(&mut self) -> Lifetime {
        let lt = Lifetime::new(
            &format!("'__apigate_o{}", self.binders.len()),
            Span::call_site(),
        );
        self.binders.push(lt.clone());
        lt
    }
}

impl VisitMut for HrtbRewriter {
    fn visit_lifetime_mut(&mut self, lt: &mut Lifetime) {
        if lt.ident != "static" {
            *lt = self.fresh();
        }
    }

    fn visit_type_reference_mut(&mut self, reference: &mut TypeReference) {
        let replace = reference
            .lifetime
            .as_ref()
            .is_none_or(|lt| lt.ident != "static");
        if replace {
            reference.lifetime = Some(self.fresh());
        }
        self.visit_type_mut(&mut reference.elem);
    }
}

/// Builds the map wrapper's single autoref `Finish` predicate over the declared
/// return type `RET`:
/// `for<'__apigate_m[, ..]> &'m Finisher<F, <RET as MapOutput>::Output>: Finish<F, <RET as MapOutput>::Output>`.
fn finish_predicate(apigate: &TokenStream2, return_ty: &Type) -> WherePredicate {
    let mut ty = return_ty.clone();
    let mut rewriter = HrtbRewriter {
        binders: Vec::new(),
    };
    rewriter.visit_type_mut(&mut ty);

    let marker = Lifetime::new("'__apigate_m", Span::call_site());
    let mut binders = rewriter.binders;
    binders.push(marker.clone());

    // The map output `T`, projected out of the declared `MapResult<T>`; named
    // once so both sides of the bound stay in lockstep.
    let output = quote!(<#ty as #apigate::__private::MapOutput>::Output);
    syn::parse_quote!(
        for<#(#binders),*> &#marker #apigate::__private::Finisher<__apigate_format, #output>:
            #apigate::__private::Finish<__apigate_format, #output>
    )
}

/// Applies the map output protocol to the wrapper `f`: makes it generic over the
/// format `__apigate_format`.
fn apply_map_output(
    f: &mut ItemFn,
    apigate: &TokenStream2,
    original: Vec<Stmt>,
) -> syn::Result<Vec<Stmt>> {
    let return_ty = map_return_type(&f.sig.output)?;

    // Generic over the format `F` the route supplies via turbofish; its `Out`
    // associated type decides the wire payload.
    f.sig
        .generics
        .params
        .push(syn::parse_quote!(__apigate_format));
    f.sig.output = syn::parse_quote!(
        -> #apigate::MapResult<
            #apigate::__private::BodyOutcome<<__apigate_format as #apigate::__private::MapFormat>::Out>
        >
    );
    {
        let where_clause = f.sig.generics.make_where_clause();
        where_clause
            .predicates
            .push(syn::parse_quote!(__apigate_format: #apigate::__private::MapFormat));
        where_clause
            .predicates
            .push(finish_predicate(apigate, &return_ty));
    }

    Ok(finish_block(apigate, original).stmts)
}

// ---------------------------------------------------------------------------
// expand_fn_params
// ---------------------------------------------------------------------------

/// Shared expansion for `#[apigate::hook]` and `#[apigate::map]`.
///
/// Rewrites function signature to canonical form.
/// - hook: all owned params are extracted from scope
/// - map: first owned param is kept as direct input, rest are extracted from scope
pub(crate) fn expand_fn_params(
    input: TokenStream,
    mode: ExpansionMode,
) -> syn::Result<TokenStream2> {
    let mut f = syn::parse::<ItemFn>(input)?;
    let apigate = apigate_crate_path()?;
    let special = SpecialTypePaths::new(&apigate)?;
    let macro_name = mode.macro_name();

    if f.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            &f.sig,
            format!("#[apigate::{macro_name}] requires an async function"),
        ));
    }

    let ctx_ident = format_ident!("__apigate_ctx");
    let scope_ident = format_ident!("__apigate_scope");

    let (plans, kept_param) = build_param_plans(&f, &special, mode)?;
    if mode.keeps_first_owned_param() && kept_param.is_none() {
        return Err(syn::Error::new_spanned(
            &f.sig,
            "#[apigate::map] requires owned input parameter",
        ));
    }

    validate_plans(&plans, &f.sig, macro_name)?;

    // Only the bindings reference the generated `ctx`/`scope` params; name them
    // `_` when nothing uses them so generated wrappers stay warning-free.
    let ctx_used = plans.iter().any(|p| p.source.binds_ctx());
    let scope_used = plans.iter().any(|p| p.source.binds_scope());
    let ctx_pat: Pat = if ctx_used {
        syn::parse_quote!(#ctx_ident)
    } else {
        syn::parse_quote!(_)
    };
    let scope_pat: Pat = if scope_used {
        syn::parse_quote!(#scope_ident)
    } else {
        syn::parse_quote!(_)
    };

    // Rewrite parameter list: [input,] ctx, scope[, finisher]
    f.sig.inputs.clear();
    if let Some(kept) = kept_param {
        f.sig.inputs.push(kept);
    }
    f.sig
        .inputs
        .push(syn::parse_quote!(#ctx_pat: &mut #apigate::PartsCtx<'_>));
    f.sig
        .inputs
        .push(syn::parse_quote!(#scope_pat: &mut #apigate::RequestScope<'_>));

    let bindings = build_bindings(&plans, &apigate, &ctx_ident, &scope_ident)?;
    let original = std::mem::take(&mut f.block.stmts);

    let body_stmts = if matches!(mode, ExpansionMode::Map) {
        apply_map_output(&mut f, &apigate, original)?
    } else {
        original
    };

    f.block.stmts = bindings;
    f.block.stmts.extend(body_stmts);

    Ok(quote!(#f))
}
