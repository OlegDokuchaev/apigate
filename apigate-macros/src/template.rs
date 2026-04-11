use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use thiserror::Error;

/// Token arrays ready to be emitted into a `RewriteTemplate`.
pub(crate) struct CompiledTemplate {
    pub src_tokens: Vec<TokenStream2>,
    pub dst_tokens: Vec<TokenStream2>,
    pub static_len: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub(crate) enum RewriteTemplateError {
    #[error("empty parameter `{{}}` in route path")]
    EmptyRouteParameter,
    #[error("duplicate parameter `{name}` in route path")]
    DuplicateRouteParameter { name: String },
    #[error("unclosed `{{` in `to`")]
    UnclosedToParameter,
    #[error("empty parameter `{{}}` in `to`")]
    EmptyToParameter,
    #[error("parameter `{name}` in `to` not found in route path")]
    UnknownToParameter { name: String },
    #[error("unmatched `}}` in `to`")]
    UnmatchedToCloseBrace,
    #[error("too many path parameters in route path (max {max})")]
    TooManyRouteParameters { max: usize },
    #[error("rewrite capture index overflow")]
    CaptureIndexOverflow,
}

// ---------------------------------------------------------------------------
// IR — intermediate representation between parsing and token emission
// ---------------------------------------------------------------------------

enum SrcSegIr {
    Lit(String),
    Param,
}

enum DstChunkIr {
    Lit(String),
    Capture(u8),
}

struct RewriteIr {
    src: Vec<SrcSegIr>,
    dst: Vec<DstChunkIr>,
    static_len: usize,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Compiles a rewrite template at macro-expand time.
///
/// Parses `{param}` segments from `route_path`, resolves parameter references
/// in `to`, and emits the corresponding `SrcSeg`/`DstChunk` token arrays.
pub(crate) fn compile_rewrite_template(
    apigate_path: &TokenStream2,
    route_path: &str,
    to: &str,
) -> Result<CompiledTemplate, RewriteTemplateError> {
    let (src, params) = parse_route_source(route_path)?;
    let (dst, static_len) = parse_rewrite_target(to, &params)?;

    Ok(emit(
        apigate_path,
        RewriteIr {
            src,
            dst,
            static_len,
        },
    ))
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parses the source route path into IR segments and a list of parameter names.
fn parse_route_source(
    route_path: &str,
) -> Result<(Vec<SrcSegIr>, Vec<String>), RewriteTemplateError> {
    const MAX_CAPTURE_PARAMS: usize = u8::MAX as usize + 1;

    let mut src = Vec::new();
    let mut params = Vec::new();

    for seg in route_path.split('/').filter(|s| !s.is_empty()) {
        if seg.starts_with('{') && seg.ends_with('}') {
            let name = &seg[1..seg.len() - 1];
            if name.is_empty() {
                return Err(RewriteTemplateError::EmptyRouteParameter);
            }
            if params.iter().any(|p| p == name) {
                return Err(RewriteTemplateError::DuplicateRouteParameter {
                    name: name.to_owned(),
                });
            }
            if params.len() >= MAX_CAPTURE_PARAMS {
                return Err(RewriteTemplateError::TooManyRouteParameters {
                    max: MAX_CAPTURE_PARAMS,
                });
            }

            params.push(name.to_owned());
            src.push(SrcSegIr::Param);
        } else {
            src.push(SrcSegIr::Lit(seg.to_owned()));
        }
    }

    Ok((src, params))
}

/// Parses the destination template, resolving `{param}` references against known parameters.
fn parse_rewrite_target(
    to: &str,
    params: &[String],
) -> Result<(Vec<DstChunkIr>, usize), RewriteTemplateError> {
    let mut dst = Vec::new();
    let mut static_len = 0usize;
    let mut lit_start = 0;
    let bytes = to.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                if lit_start < i {
                    let lit = &to[lit_start..i];
                    static_len += lit.len();
                    dst.push(DstChunkIr::Lit(lit.to_owned()));
                }

                let close = to[i + 1..]
                    .find('}')
                    .ok_or(RewriteTemplateError::UnclosedToParameter)?;

                let name = &to[i + 1..i + 1 + close];
                if name.is_empty() {
                    return Err(RewriteTemplateError::EmptyToParameter);
                }

                let src_index = params.iter().position(|p| p == name).ok_or_else(|| {
                    RewriteTemplateError::UnknownToParameter {
                        name: name.to_owned(),
                    }
                })?;
                let src_index = u8::try_from(src_index)
                    .map_err(|_| RewriteTemplateError::CaptureIndexOverflow)?;

                dst.push(DstChunkIr::Capture(src_index));

                i = i + 1 + close + 1;
                lit_start = i;
            }
            b'}' => return Err(RewriteTemplateError::UnmatchedToCloseBrace),
            _ => i += 1,
        }
    }

    if lit_start < to.len() {
        let lit = &to[lit_start..];
        static_len += lit.len();
        dst.push(DstChunkIr::Lit(lit.to_owned()));
    }

    Ok((dst, static_len))
}

// ---------------------------------------------------------------------------
// Token emission
// ---------------------------------------------------------------------------

/// Converts the IR into `quote!`-ready token arrays.
fn emit(apigate_path: &TokenStream2, ir: RewriteIr) -> CompiledTemplate {
    let src_tokens = ir
        .src
        .into_iter()
        .map(|seg| match seg {
            SrcSegIr::Lit(lit) => quote!(#apigate_path::SrcSeg::Lit(#lit)),
            SrcSegIr::Param => quote!(#apigate_path::SrcSeg::Param),
        })
        .collect();

    let dst_tokens = ir
        .dst
        .into_iter()
        .map(|chunk| match chunk {
            DstChunkIr::Lit(lit) => quote!(#apigate_path::DstChunk::Lit(#lit)),
            DstChunkIr::Capture(idx) => {
                quote!(#apigate_path::DstChunk::Capture { src_index: #idx })
            }
        })
        .collect();

    CompiledTemplate {
        src_tokens,
        dst_tokens,
        static_len: ir.static_len,
    }
}
