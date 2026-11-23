//! `#[tpt_bench]` and `#[tpt_bench_group]` expansion.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::{
    Attribute, Error, Expr, FnArg, Ident, Item, ItemFn, ItemMod, Meta, ReturnType, Signature,
    Token, Type,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};

use crate::Errors;
use crate::spec::{Layer, default_layers, parse_layer_list};

/// Parsed contents of `#[tpt_bench(...)]`.
struct BenchOptions {
    /// The `args = [...]` values, one benchmark per entry.
    args: Option<Vec<Expr>>,
}

impl Parse for BenchOptions {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(Self { args: None });
        }
        let key: Ident = input.parse()?;
        if key != "args" {
            return Err(Error::new_spanned(
                &key,
                "unknown option; expected `args = [...]`",
            ));
        }
        input.parse::<Token![=]>()?;
        let content;
        let bracket = syn::bracketed!(content in input);
        let values: Vec<Expr> = Punctuated::<Expr, Token![,]>::parse_terminated(&content)?
            .into_iter()
            .collect();
        if !input.is_empty() {
            return Err(input.error("unexpected tokens after the `args` list"));
        }
        if values.is_empty() {
            return Err(Error::new(
                bracket.span.join(),
                "`args` needs at least one value",
            ));
        }
        Ok(Self { args: Some(values) })
    }
}

/// Whether an attribute's last path segment is `name`, so that both `#[name]`
/// and `#[path::name]` match.
pub(crate) fn is_named(attr: &Attribute, name: &str) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|segment| segment.ident == name)
}

/// Removes and returns every attribute called `name`.
fn take_attrs(attrs: &mut Vec<Attribute>, name: &str) -> Vec<Attribute> {
    let (taken, kept): (Vec<_>, Vec<_>) = std::mem::take(attrs)
        .into_iter()
        .partition(|attr| is_named(attr, name));
    *attrs = kept;
    taken
}

/// The tokens inside `#[name(...)]`, or nothing for `#[name]`.
fn attr_tokens(attr: &Attribute) -> syn::Result<TokenStream> {
    match &attr.meta {
        Meta::Path(_) => Ok(TokenStream::new()),
        Meta::List(list) => Ok(list.tokens.clone()),
        Meta::NameValue(name_value) => Err(Error::new_spanned(
            name_value,
            "expected `#[name(...)]` or `#[name]`",
        )),
    }
}

/// Parses the `#[layers(...)]` attribute among `attrs`, if there is one.
fn layers_in(attrs: &[Attribute], errors: &mut Errors) -> Option<Vec<Layer>> {
    let mut found = attrs.iter().filter(|attr| is_named(attr, "layers"));
    let first = found.next()?;
    for extra in found {
        errors.add(Error::new_spanned(extra, "duplicate `#[layers]` attribute"));
    }
    let tokens = errors.check(attr_tokens(first))?;
    errors.check(parse_layer_list(tokens))
}

fn is_bencher_param(arg: &FnArg) -> bool {
    let FnArg::Typed(pat) = arg else {
        return false;
    };
    let Type::Reference(reference) = &*pat.ty else {
        return false;
    };
    if reference.mutability.is_none() {
        return false;
    }
    let Type::Path(path) = &*reference.elem else {
        return false;
    };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "Bencher")
}

const BENCHER_PARAM_MSG: &str =
    "the first parameter of a benchmark function must be `&mut Bencher`";

/// Checks that a benchmark function has the shape the generated code relies on.
fn check_signature(sig: &Signature, has_args: bool) -> syn::Result<()> {
    if let Some(asyncness) = sig.asyncness {
        return Err(Error::new(
            asyncness.span,
            "async benchmark functions are not supported",
        ));
    }

    let mut inputs = sig.inputs.iter();
    let first = inputs
        .next()
        .ok_or_else(|| Error::new_spanned(&sig.ident, BENCHER_PARAM_MSG))?;
    if !is_bencher_param(first) {
        return Err(Error::new_spanned(first, BENCHER_PARAM_MSG));
    }

    let extra: Vec<&FnArg> = inputs.collect();
    if has_args {
        if extra.len() != 1 {
            return Err(Error::new_spanned(
                &sig.ident,
                "`args` supplies one value per benchmark, so the function must take exactly \
                 one parameter after `&mut Bencher`",
            ));
        }
    } else if let Some(param) = extra.first() {
        return Err(Error::new_spanned(
            param,
            "this benchmark takes parameters beyond `&mut Bencher`; add `args = [...]` to supply them",
        ));
    }

    if let ReturnType::Type(_, ty) = &sig.output {
        let is_unit = matches!(&**ty, Type::Tuple(tuple) if tuple.elems.is_empty());
        if !is_unit {
            return Err(Error::new_spanned(
                ty,
                "benchmark functions must return `()`",
            ));
        }
    }
    Ok(())
}

/// One concrete benchmark: a function, optionally with one argument value.
struct Target {
    /// The benchmark name, e.g. `multiply` or `multiply/128`.
    name: String,
    /// The argument expression, when `args = [...]` was given.
    arg: Option<Expr>,
}

/// A benchmark function that passed validation.
struct Bench {
    ident: Ident,
    targets: Vec<Target>,
    layers: Vec<Layer>,
}

/// Expands a standalone `#[tpt_bench]`. It validates the function and leaves
/// it unchanged; registration happens in `#[tpt_bench_group]`.
pub(crate) fn expand_bench(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut errors = Errors::default();
    match syn::parse2::<Item>(item.clone()) {
        Ok(Item::Fn(function)) => {
            if let Some(opts) = errors.check(syn::parse2::<BenchOptions>(attr)) {
                errors.check(check_signature(&function.sig, opts.args.is_some()));
            }
        }
        Ok(other) => errors.add(Error::new_spanned(
            other,
            "`#[tpt_bench]` can only be applied to a function",
        )),
        Err(err) => errors.add(err),
    }
    let errors = errors.into_tokens();
    quote!(#errors #item)
}

/// Expands `#[tpt_bench_group]` on a module.
pub(crate) fn expand_group(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut errors = Errors::default();
    if !attr.is_empty() {
        errors.add(Error::new_spanned(
            &attr,
            "`#[tpt_bench_group]` takes no arguments; put `#[layers(...)]` on the module",
        ));
    }
    match syn::parse2::<Item>(item.clone()) {
        Ok(Item::Mod(module)) => {
            let expanded = expand_group_module(module, None, &mut errors);
            let errors = errors.into_tokens();
            quote!(#errors #expanded)
        }
        Ok(other) => {
            errors.add(Error::new_spanned(
                other,
                "`#[tpt_bench_group]` can only be applied to an inline `mod`",
            ));
            let errors = errors.into_tokens();
            quote!(#errors #item)
        }
        Err(err) => {
            errors.add(err);
            let errors = errors.into_tokens();
            quote!(#errors #item)
        }
    }
}

/// Builds the group module: strips the benchmark attributes, keeps every item,
/// and appends the generated run functions and `__tpt_benches`.
///
/// `outer_layers` comes from a `#[layers]` written above `#[tpt_bench_group]`,
/// which is expanded first and therefore cannot see the group.
pub(crate) fn expand_group_module(
    module: ItemMod,
    outer_layers: Option<Vec<Layer>>,
    errors: &mut Errors,
) -> TokenStream {
    let ItemMod {
        mut attrs,
        vis,
        ident,
        content,
        ..
    } = module;

    let mut mod_errors = Errors::default();
    let mod_layers = layers_in(&attrs, &mut mod_errors);
    if !mod_errors.is_empty() {
        // Keep the module's own `#[layers]` out of the output: the expansion
        // above has already reported it.
        attrs.retain(|attr| !is_named(attr, "layers"));
    }
    errors.absorb(mod_errors);
    let group_layers = match (mod_layers, outer_layers) {
        (Some(_), Some(_)) => {
            errors.add(Error::new_spanned(
                &ident,
                "`#[layers]` is given both above and below `#[tpt_bench_group]`",
            ));
            None
        }
        (Some(layers), None) | (None, Some(layers)) => Some(layers),
        (None, None) => None,
    };

    let Some((_, mut items)) = content else {
        errors.add(Error::new_spanned(
            &ident,
            "`#[tpt_bench_group]` requires an inline module: `mod name { ... }`",
        ));
        return quote!(#(#attrs)* #vis mod #ident;);
    };

    let mut benches = Vec::new();
    for item in &mut items {
        let Item::Fn(function) = item else {
            continue;
        };
        if !function
            .attrs
            .iter()
            .any(|attr| is_named(attr, "tpt_bench"))
        {
            continue;
        }
        if let Some(bench) = collect_bench(function, group_layers.as_ref(), errors) {
            benches.push(bench);
        }
    }

    let mut seen = std::collections::HashSet::new();
    let group_name = ident.to_string();
    let mut run_fns = Vec::new();
    let mut defs = Vec::new();
    for bench in &benches {
        for (index, target) in bench.targets.iter().enumerate() {
            if !seen.insert(target.name.clone()) {
                errors.add(Error::new_spanned(
                    &bench.ident,
                    format!("duplicate benchmark name `{}`", target.name),
                ));
                continue;
            }
            let run = format_ident!("__tpt_run_{}_{}", bench.ident, index);
            let fn_name = &bench.ident;
            let invoke = match &target.arg {
                Some(arg) => quote!(#fn_name(__b, #arg)),
                None => quote!(#fn_name(__b)),
            };
            let arms = bench.layers.iter().map(|layer| layer.run_arm(&invoke));
            run_fns.push(quote! {
                #[doc(hidden)]
                #[allow(unreachable_patterns, clippy::redundant_closure)]
                fn #run(
                    kind: ::tpt_benchmark::LayerKind,
                    config: &::tpt_benchmark::Config,
                ) -> ::core::option::Option<::tpt_benchmark::Summary> {
                    match kind {
                        #(#arms)*
                        _ => ::core::option::Option::None,
                    }
                }
            });

            let name = &target.name;
            let kinds = bench.layers.iter().map(|layer| layer.kind_path());
            defs.push(quote! {
                ::tpt_benchmark::BenchDef::new(
                    #group_name,
                    #name,
                    &[#(#kinds),*],
                    #run,
                )
            });
        }
    }

    let generated = quote! {
        #(#run_fns)*

        #[doc(hidden)]
        pub fn __tpt_benches() -> ::std::vec::Vec<::tpt_benchmark::BenchDef> {
            ::std::vec![#(#defs),*]
        }
    };

    quote! {
        #(#attrs)*
        #vis mod #ident {
            #(#items)*
            #generated
        }
    }
}

/// Validates one `#[tpt_bench]` function and returns what the group needs to
/// register it. The benchmark attributes stay on the function unless it failed,
/// so the no-op `#[tpt_bench]`/`#[layers]` still resolve (no unused-import
/// warnings), while a failure is reported only once.
fn collect_bench(
    function: &mut ItemFn,
    group_layers: Option<&Vec<Layer>>,
    errors: &mut Errors,
) -> Option<Bench> {
    let mut local = Errors::default();
    let fn_layers = layers_in(&function.attrs, &mut local);
    let bench_count = function
        .attrs
        .iter()
        .filter(|attr| is_named(attr, "tpt_bench"))
        .count();
    if bench_count > 1 {
        local.add(Error::new(
            Span::call_site(),
            "duplicate `#[tpt_bench]` attribute",
        ));
    }
    let bench_attr = function
        .attrs
        .iter()
        .find(|attr| is_named(attr, "tpt_bench"))?;
    let opts = local.check(attr_tokens(bench_attr).and_then(syn::parse2::<BenchOptions>));
    let checked = opts.and_then(|opts| {
        local.check(check_signature(&function.sig, opts.args.is_some()))?;
        Some(opts)
    });

    if !local.is_empty() {
        errors.absorb(local);
        take_attrs(&mut function.attrs, "tpt_bench");
        take_attrs(&mut function.attrs, "layers");
        return None;
    }
    let opts = checked?;

    let layers = fn_layers
        .or_else(|| group_layers.cloned())
        .unwrap_or_else(default_layers);

    let ident = function.sig.ident.clone();
    let targets = match opts.args {
        None => vec![Target {
            name: ident.to_string(),
            arg: None,
        }],
        Some(values) => values
            .into_iter()
            .map(|value| {
                let text = quote!(#value).to_string();
                Target {
                    name: format!("{ident}/{text}"),
                    arg: Some(value),
                }
            })
            .collect(),
    };

    Some(Bench {
        ident,
        targets,
        layers,
    })
}
