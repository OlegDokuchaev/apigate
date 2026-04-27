use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Error, Ident, LitStr, Result, Token};

use crate::parse::{parse_assigned, set_once};

pub(crate) struct ServiceArgs {
    pub name: Option<LitStr>,
    pub prefix: Option<LitStr>,
    pub policy: Option<LitStr>,
}

enum ServiceArg {
    Name(LitStr),
    Prefix(LitStr),
    Policy(LitStr),
}

impl Parse for ServiceArg {
    fn parse(input: ParseStream) -> Result<Self> {
        let key: Ident = input.parse()?;
        let value: LitStr = parse_assigned(input)?;

        match key.to_string().as_str() {
            "name" => Ok(Self::Name(value)),
            "prefix" => Ok(Self::Prefix(value)),
            "policy" => Ok(Self::Policy(value)),
            _ => Err(Error::new(key.span(), format!("unknown argument `{key}`"))),
        }
    }
}

impl Parse for ServiceArgs {
    fn parse(input: ParseStream) -> Result<Self> {
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
            name,
            prefix,
            policy,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_empty_service_args() {
        let args: ServiceArgs = syn::parse_str("").unwrap();

        assert!(args.name.is_none());
        assert!(args.prefix.is_none());
        assert!(args.policy.is_none());
    }

    #[test]
    fn parses_all_service_args() {
        let args: ServiceArgs =
            syn::parse_str(r#"name = "sales", prefix = "/api/sales", policy = "sticky""#).unwrap();

        assert_eq!(args.name.unwrap().value(), "sales");
        assert_eq!(args.prefix.unwrap().value(), "/api/sales");
        assert_eq!(args.policy.unwrap().value(), "sticky");
    }

    #[test]
    fn rejects_duplicate_or_unknown_service_args() {
        assert!(syn::parse_str::<ServiceArgs>(r#"name = "a", name = "b""#).is_err());
        assert!(syn::parse_str::<ServiceArgs>(r#"unknown = "x""#).is_err());
    }
}
