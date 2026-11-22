//! Measurement-layer names accepted by `#[layers(...)]`, and the code each one
//! expands to.

use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::{Ident, Item, Token, parse::Parser, punctuated::Punctuated};

use crate::Errors;

/// One of the four hardware counters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Counter {
    Instructions,
    L1Misses,
    L2Misses,
    BranchMisses,
}

impl Counter {
    const ALL: [Self; 4] = [
        Self::Instructions,
        Self::L1Misses,
        Self::L2Misses,
        Self::BranchMisses,
    ];

    /// The variant name in `tpt_benchmark::Counter`.
    const fn variant(self) -> &'static str {
        match self {
            Self::Instructions => "Instructions",
            Self::L1Misses => "L1Misses",
            Self::L2Misses => "L2Misses",
            Self::BranchMisses => "BranchMisses",
        }
    }

    /// The variant name in `tpt_benchmark::LayerKind`.
    const fn kind(self) -> &'static str {
        match self {
            Self::Instructions => "HwInstructions",
            Self::L1Misses => "HwL1Misses",
            Self::L2Misses => "HwL2Misses",
            Self::BranchMisses => "HwBranchMisses",
        }
    }
}

/// A single measurement layer, after `HardwareCounters` has been expanded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layer {
    WallClock,
    Allocations,
    AllocBytes,
    Hardware(Counter),
}

impl Layer {
    /// The `LayerKind` path for this layer.
    pub(crate) fn kind_path(self) -> TokenStream {
        let name = match self {
            Self::WallClock => "WallClock",
            Self::Allocations => "Allocations",
            Self::AllocBytes => "AllocBytes",
            Self::Hardware(counter) => counter.kind(),
        };
        let variant = Ident::new(name, Span::call_site());
        quote!(::tpt_benchmark::LayerKind::#variant)
    }

    /// A match arm of the generated `run` function. `call` is the benchmark
    /// invocation, written in terms of the closure parameter `__b`.
    pub(crate) fn run_arm(self, call: &TokenStream) -> TokenStream {
        let kind = self.kind_path();
        match self {
            Self::WallClock => quote! {
                #kind => ::core::option::Option::Some(::tpt_benchmark::measure(
                    config,
                    ::tpt_benchmark::WallClock::new(),
                    |__b| #call,
                )),
            },
            Self::Allocations => quote! {
                #kind => ::core::option::Option::Some(::tpt_benchmark::measure(
                    config,
                    ::tpt_benchmark::alloc::Allocations::new(),
                    |__b| #call,
                )),
            },
            Self::AllocBytes => quote! {
                #kind => ::core::option::Option::Some(::tpt_benchmark::measure(
                    config,
                    ::tpt_benchmark::alloc::AllocBytes::new(),
                    |__b| #call,
                )),
            },
            Self::Hardware(counter) => {
                let variant = Ident::new(counter.variant(), Span::call_site());
                let kind_name = counter.kind();
                quote! {
                    #kind => match ::tpt_benchmark::HardwareCounter::try_new(
                        ::tpt_benchmark::Counter::#variant,
                    ) {
                        ::core::result::Result::Ok(__layer) => ::core::option::Option::Some(
                            ::tpt_benchmark::measure(config, __layer, |__b| #call),
                        ),
                        ::core::result::Result::Err(__err) => {
                            ::std::eprintln!(
                                "warning: {} unavailable: {}",
                                #kind_name,
                                __err.reason,
                            );
                            ::core::option::Option::None
                        }
                    },
                }
            }
        }
    }
}

/// The layers a benchmark runs under when it has no `#[layers]` attribute.
pub(crate) fn default_layers() -> Vec<Layer> {
    vec![Layer::WallClock]
}

fn layers_for_name(name: &Ident) -> syn::Result<Vec<Layer>> {
    let layers = match name.to_string().as_str() {
        "WallClock" => vec![Layer::WallClock],
        "Allocations" => vec![Layer::Allocations],
        "AllocBytes" => vec![Layer::AllocBytes],
        "HardwareCounters" => Counter::ALL.into_iter().map(Layer::Hardware).collect(),
        "HwInstructions" => vec![Layer::Hardware(Counter::Instructions)],
        "HwL1Misses" => vec![Layer::Hardware(Counter::L1Misses)],
        "HwL2Misses" => vec![Layer::Hardware(Counter::L2Misses)],
        "HwBranchMisses" => vec![Layer::Hardware(Counter::BranchMisses)],
        other => {
            return Err(syn::Error::new_spanned(
                name,
                format!(
                    "unknown layer `{other}`; expected one of WallClock, Allocations, \
                     AllocBytes, HardwareCounters, HwInstructions, HwL1Misses, HwL2Misses, \
                     HwBranchMisses"
                ),
            ));
        }
    };
    Ok(layers)
}

/// Parses the contents of `#[layers(...)]` into a de-duplicated layer list.
pub(crate) fn parse_layer_list(tokens: TokenStream) -> syn::Result<Vec<Layer>> {
    let names = Parser::parse2(Punctuated::<Ident, Token![,]>::parse_terminated, tokens)?;
    if names.is_empty() {
        return Err(syn::Error::new(
            Span::call_site(),
            "`#[layers(...)]` needs at least one layer name",
        ));
    }
    let mut layers = Vec::new();
    for name in &names {
        for layer in layers_for_name(name)? {
            if !layers.contains(&layer) {
                layers.push(layer);
            }
        }
    }
    Ok(layers)
}

/// Expands a standalone `#[layers(...)]`. It only validates the names, unless
/// it sits on a module that also has `#[tpt_bench_group]` below it, in which
/// case the group is expanded here so it can see the layer list.
pub(crate) fn expand_layers(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut errors = Errors::default();
    let layers = errors.check(parse_layer_list(attr));

    let parsed = match syn::parse2::<Item>(item.clone()) {
        Ok(parsed) => parsed,
        Err(err) => {
            errors.add(err);
            return errors.into_tokens();
        }
    };

    match parsed {
        Item::Mod(mut module)
            if module
                .attrs
                .iter()
                .any(|attr| crate::bench::is_named(attr, "tpt_bench_group")) =>
        {
            module
                .attrs
                .retain(|attr| !crate::bench::is_named(attr, "tpt_bench_group"));
            let grouped = crate::bench::expand_group_module(module, layers, &mut errors);
            let errors = errors.into_tokens();
            quote!(#errors #grouped)
        }
        _ => {
            let errors = errors.into_tokens();
            quote!(#errors #item)
        }
    }
}
