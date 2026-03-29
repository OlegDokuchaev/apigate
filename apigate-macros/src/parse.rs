use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Error, Path, Result, Token, bracketed};

/// Sets `slot` to `value`, returning an error if the slot is already occupied.
pub(crate) fn set_once<T>(slot: &mut Option<T>, value: T, span: Span, name: &str) -> Result<()> {
    if slot.is_some() {
        Err(Error::new(span, format!("duplicate `{name}`")))
    } else {
        *slot = Some(value);
        Ok(())
    }
}

/// Unwraps an `Option<T>`, returning a compile error at `span` if `None`.
pub(crate) fn required<T>(slot: Option<T>, span: Span, message: &'static str) -> Result<T> {
    slot.ok_or_else(|| Error::new(span, message))
}

/// Parses `= <value>` from the input stream (consumes the `=` token first).
pub(crate) fn parse_assigned<T: Parse>(input: ParseStream<'_>) -> Result<T> {
    input.parse::<Token![=]>()?;
    input.parse()
}

/// Parses `[path1, path2, ...]` — a bracketed, comma-separated list of paths.
pub(crate) fn parse_bracketed_paths(input: ParseStream<'_>) -> Result<Vec<Path>> {
    let content;
    bracketed!(content in input);

    Ok(content
        .call(Punctuated::<Path, Token![,]>::parse_terminated)?
        .into_iter()
        .collect())
}
