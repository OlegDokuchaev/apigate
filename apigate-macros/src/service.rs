use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Error, Ident, LitStr, Result, Token};

use crate::parse::{parse_assigned, required, set_once};

pub(crate) struct ServiceArgs {
    pub name: LitStr,
    pub prefix: Option<LitStr>,
    pub policy: Option<LitStr>,
}

enum ServiceArg {
    Name(LitStr),
    Prefix(LitStr),
    Policy(LitStr),
}

impl Parse for ServiceArg {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let key: Ident = input.parse()?;
        let value: LitStr = parse_assigned(input)?;

        if key == "name" {
            Ok(Self::Name(value))
        } else if key == "prefix" {
            Ok(Self::Prefix(value))
        } else if key == "policy" {
            Ok(Self::Policy(value))
        } else {
            Err(Error::new(key.span(), format!("unknown argument `{key}`")))
        }
    }
}

impl Parse for ServiceArgs {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let args = input.call(Punctuated::<ServiceArg, Token![,]>::parse_terminated)?;

        let mut name = None;
        let mut prefix = None;
        let mut policy = None;

        for arg in args {
            match arg {
                ServiceArg::Name(v) => set_once(&mut name, v.clone(), v.span(), "name")?,
                ServiceArg::Prefix(v) => set_once(&mut prefix, v.clone(), v.span(), "prefix")?,
                ServiceArg::Policy(v) => set_once(&mut policy, v.clone(), v.span(), "policy")?,
            }
        }

        Ok(Self {
            name: required(name, Span::call_site(), "missing `name = \"...\"`")?,
            prefix,
            policy,
        })
    }
}
