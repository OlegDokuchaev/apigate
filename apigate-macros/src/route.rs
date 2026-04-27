use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, Error, Ident, Item, ItemFn, LitStr, Path, Result, Token, Type};

use crate::codegen::generate_pipeline_wrapper;
use crate::parse::{parse_assigned, parse_bracketed_paths, set_once};
use crate::template::compile_rewrite_template;

// ---------------------------------------------------------------------------
// ExtractedRoute
// ---------------------------------------------------------------------------

/// Output of route expansion: the `RouteDef` token stream and any helper items
/// (before-wrappers, map-wrappers, rewrite template statics) to inject into the module.
pub(crate) struct ExtractedRoute {
    pub route_def: TokenStream2,
    pub generated_items: Vec<Item>,
}

// ---------------------------------------------------------------------------
// MethodKind
// ---------------------------------------------------------------------------

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

impl MethodKind {
    /// Tries to recognize an apigate route attribute (e.g. `#[apigate::get]`).
    fn from_attr(attr: &Attribute) -> Option<Self> {
        let last = attr.path().segments.last()?.ident.to_string();

        match last.as_str() {
            "get" => Some(Self::Get),
            "post" => Some(Self::Post),
            "put" => Some(Self::Put),
            "delete" => Some(Self::Delete),
            "patch" => Some(Self::Patch),
            "head" => Some(Self::Head),
            "options" => Some(Self::Options),
            _ => None,
        }
    }

    /// Emits `apigate::Method::*` variant tokens.
    fn to_tokens(self, apigate_path: &TokenStream2) -> TokenStream2 {
        match self {
            Self::Get => quote!(#apigate_path::Method::Get),
            Self::Post => quote!(#apigate_path::Method::Post),
            Self::Put => quote!(#apigate_path::Method::Put),
            Self::Delete => quote!(#apigate_path::Method::Delete),
            Self::Patch => quote!(#apigate_path::Method::Patch),
            Self::Head => quote!(#apigate_path::Method::Head),
            Self::Options => quote!(#apigate_path::Method::Options),
        }
    }
}

// ---------------------------------------------------------------------------
// DataKind
// ---------------------------------------------------------------------------

/// Which request body extractor the route uses (if any).
#[derive(Clone, Default)]
pub(crate) enum DataKind {
    #[default]
    None,
    Query(Type),
    Json(Type),
    Form(Type),
    Multipart,
}

impl DataKind {
    fn allows_map(&self) -> bool {
        matches!(self, Self::Query(_) | Self::Json(_) | Self::Form(_))
    }

    /// Transitions to `next`, erroring if a data kind was already chosen.
    fn set(self, next: DataKind, span: Span) -> Result<DataKind> {
        match self {
            Self::None => Ok(next),
            _ => Err(Error::new(
                span,
                "only one of `query`, `json`, `form`, or `multipart` may be specified",
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// RouteArgs
// ---------------------------------------------------------------------------

/// Parsed arguments of a route attribute (e.g. `#[apigate::get("/path", ...)]`).
pub(crate) struct RouteArgs {
    pub path: LitStr,
    pub to: Option<LitStr>,
    pub policy: Option<LitStr>,
    pub before: Vec<Path>,
    pub map: Option<Path>,
    pub data: DataKind,
    pub path_type: Option<Type>,
}

impl RouteArgs {
    /// Checks cross-field invariants (e.g. `map` requires a typed data kind).
    fn validate(&self) -> Result<()> {
        if self.map.is_some() && !self.data.allows_map() {
            match self.data {
                DataKind::Multipart => {
                    return Err(Error::new(
                        Span::call_site(),
                        "`map` is not supported together with `multipart`",
                    ));
                }
                DataKind::None => {
                    return Err(Error::new(
                        Span::call_site(),
                        "`map` requires one of `query = T`, `json = T`, or `form = T`",
                    ));
                }
                DataKind::Query(_) | DataKind::Json(_) | DataKind::Form(_) => {}
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// RouteArgsBuilder
// ---------------------------------------------------------------------------

#[derive(Default)]
struct RouteArgsBuilder {
    to: Option<LitStr>,
    policy: Option<LitStr>,
    before: Option<Vec<Path>>,
    map: Option<Path>,
    data: DataKind,
    path_type: Option<Type>,
}

impl RouteArgsBuilder {
    fn apply(&mut self, arg: RouteArg) -> Result<()> {
        match arg {
            RouteArg::To(v) => set_once(&mut self.to, v.clone(), v.span(), "to")?,
            RouteArg::Policy(v) => set_once(&mut self.policy, v.clone(), v.span(), "policy")?,
            RouteArg::Before(v) => {
                set_once(&mut self.before, v, Span::call_site(), "before")?;
            }
            RouteArg::Map(v) => set_once(&mut self.map, v, Span::call_site(), "map")?,
            RouteArg::Query(v) => {
                self.data =
                    std::mem::take(&mut self.data).set(DataKind::Query(v), Span::call_site())?;
            }
            RouteArg::Json(v) => {
                self.data =
                    std::mem::take(&mut self.data).set(DataKind::Json(v), Span::call_site())?;
            }
            RouteArg::Form(v) => {
                self.data =
                    std::mem::take(&mut self.data).set(DataKind::Form(v), Span::call_site())?;
            }
            RouteArg::PathType(v) => {
                set_once(&mut self.path_type, v, Span::call_site(), "path")?;
            }
            RouteArg::Flag(RouteFlag::Multipart) => {
                self.data =
                    std::mem::take(&mut self.data).set(DataKind::Multipart, Span::call_site())?;
            }
        }

        Ok(())
    }

    fn build(self, path: LitStr) -> Result<RouteArgs> {
        let args = RouteArgs {
            path,
            to: self.to,
            policy: self.policy,
            before: self.before.unwrap_or_default(),
            map: self.map,
            data: self.data,
            path_type: self.path_type,
        };

        args.validate()?;
        Ok(args)
    }
}

// ---------------------------------------------------------------------------
// RouteArg / RouteFlag
// ---------------------------------------------------------------------------

enum RouteFlag {
    Multipart,
}

enum RouteArg {
    To(LitStr),
    Policy(LitStr),
    Before(Vec<Path>),
    Map(Path),
    Query(Type),
    Json(Type),
    Form(Type),
    PathType(Type),
    Flag(RouteFlag),
}

impl Parse for RouteArg {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: Ident = input.parse()?;
        let key_str = key.to_string();

        if input.peek(Token![=]) {
            match key_str.as_str() {
                "to" => Ok(Self::To(parse_assigned(input)?)),
                "policy" => Ok(Self::Policy(parse_assigned(input)?)),
                "before" => {
                    input.parse::<Token![=]>()?;
                    Ok(Self::Before(parse_bracketed_paths(input)?))
                }
                "map" => Ok(Self::Map(parse_assigned(input)?)),
                "query" => Ok(Self::Query(parse_assigned(input)?)),
                "json" => Ok(Self::Json(parse_assigned(input)?)),
                "form" => Ok(Self::Form(parse_assigned(input)?)),
                "path" => Ok(Self::PathType(parse_assigned(input)?)),
                _ => Err(Error::new(
                    key.span(),
                    "unknown route argument, expected one of: \
                     `to`, `policy`, `before`, `map`, `query`, `json`, `form`, `path`",
                )),
            }
        } else {
            match key_str.as_str() {
                "multipart" => Ok(Self::Flag(RouteFlag::Multipart)),
                _ => Err(Error::new(
                    key.span(),
                    "expected `key = value` or a supported bare flag (`multipart`)",
                )),
            }
        }
    }
}

impl Parse for RouteArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let path: LitStr = input.parse()?;

        let mut builder = RouteArgsBuilder::default();

        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
            builder.apply(input.parse()?)?;
        }

        builder.build(path)
    }
}

// ---------------------------------------------------------------------------
// Route expansion (orchestration)
// ---------------------------------------------------------------------------

struct MatchedRouteAttr {
    idx: usize,
    kind: MethodKind,
    args: RouteArgs,
}

/// Expands a single function inside a `#[apigate::service]` module:
/// finds its route attribute, removes it, and generates a `RouteDef` plus
/// any helper items (before-wrappers, map-wrappers, rewrite statics).
pub(crate) fn expand_route_from_fn(
    apigate_path: &TokenStream2,
    f: &mut ItemFn,
) -> Result<Option<ExtractedRoute>> {
    let Some(matched) = find_route_attr(f)? else {
        return Ok(None);
    };

    f.attrs.remove(matched.idx);
    f.attrs.push(syn::parse_quote!(#[allow(dead_code)]));

    let mut generated_items = Vec::new();

    let rewrite_spec =
        build_rewrite_spec(apigate_path, &matched.args.path, matched.args.to.as_ref())?;

    let pipeline = generate_pipeline_wrapper(
        apigate_path,
        f,
        &matched.args.before,
        &matched.args.data,
        matched.args.map.as_ref(),
        matched.args.path_type.as_ref(),
        &mut generated_items,
    )?;

    let method = matched.kind.to_tokens(apigate_path);
    let path = &matched.args.path;

    let policy = match &matched.args.policy {
        None => quote!(None),
        Some(p) => quote!(Some(#p)),
    };

    let route_def = quote! {
        #apigate_path::RouteDef {
            method: #method,
            path: #path,
            rewrite: #rewrite_spec,
            policy: #policy,
            pipeline: #pipeline,
        }
    };

    Ok(Some(ExtractedRoute {
        route_def,
        generated_items,
    }))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Scans function attributes for a single `#[apigate::<method>(...)]` and parses it.
/// Returns an error if multiple route attributes are found on the same function.
fn find_route_attr(f: &ItemFn) -> Result<Option<MatchedRouteAttr>> {
    let mut found: Option<MatchedRouteAttr> = None;

    for (idx, attr) in f.attrs.iter().enumerate() {
        let Some(kind) = MethodKind::from_attr(attr) else {
            continue;
        };

        let args: RouteArgs = attr.parse_args()?;

        if found.is_some() {
            return Err(Error::new_spanned(
                attr,
                "multiple apigate route attributes on one function are not supported",
            ));
        }

        found = Some(MatchedRouteAttr { idx, kind, args });
    }

    Ok(found)
}

/// Generates a `RewriteSpec` token stream based on the `to` attribute:
/// - `None` maps to `StripPrefix`.
/// - A static string maps to `Static`.
/// - A string with `{param}` inlines a `RewriteTemplate` const-promoted to `'static`.
fn build_rewrite_spec(
    apigate_path: &TokenStream2,
    path: &LitStr,
    to: Option<&LitStr>,
) -> Result<TokenStream2> {
    match to {
        None => Ok(quote!(#apigate_path::RewriteSpec::StripPrefix)),

        Some(t) if !t.value().contains('{') => Ok(quote!(#apigate_path::RewriteSpec::Static(#t))),

        Some(t) => {
            let compiled = compile_rewrite_template(apigate_path, &path.value(), &t.value())
                .map_err(|msg| Error::new_spanned(t, msg))?;

            let src_tokens = &compiled.src_tokens;
            let dst_tokens = &compiled.dst_tokens;
            let static_len = compiled.static_len;

            Ok(
                quote!(#apigate_path::RewriteSpec::Template(&#apigate_path::RewriteTemplate {
                    src: &[#(#src_tokens),*],
                    dst: &[#(#dst_tokens),*],
                    static_len: #static_len,
                })),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_route_args_with_hooks_map_and_json_body() {
        let args: RouteArgs = syn::parse_str(
            r#""/items", to = "/internal", policy = "sticky", before = [auth, trace], json = Input, map = remap"#,
        )
        .unwrap();

        assert_eq!(args.path.value(), "/items");
        assert_eq!(args.to.unwrap().value(), "/internal");
        assert_eq!(args.policy.unwrap().value(), "sticky");
        assert_eq!(args.before.len(), 2);
        assert_eq!(args.map.unwrap().segments.last().unwrap().ident, "remap");
        assert!(matches!(args.data, DataKind::Json(_)));
    }

    #[test]
    fn parses_route_args_with_path_type_and_multipart_flag() {
        let args: RouteArgs = syn::parse_str(r#""/{id}", path = PathParams, multipart"#).unwrap();

        assert_eq!(args.path.value(), "/{id}");
        assert!(args.path_type.is_some());
        assert!(matches!(args.data, DataKind::Multipart));
    }

    #[test]
    fn rejects_duplicate_route_fields_or_unknown_arguments() {
        assert!(syn::parse_str::<RouteArgs>(r#""/items", to = "/a", to = "/b""#).is_err());
        assert!(syn::parse_str::<RouteArgs>(r#""/items", query = A, json = B"#).is_err());
        assert!(syn::parse_str::<RouteArgs>(r#""/items", unknown = A"#).is_err());
    }

    #[test]
    fn rejects_map_without_supported_data_kind() {
        assert!(syn::parse_str::<RouteArgs>(r#""/items", map = remap"#).is_err());
        assert!(syn::parse_str::<RouteArgs>(r#""/items", multipart, map = remap"#).is_err());
    }
}
