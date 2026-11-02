//! One file per rule. Graph rules read the package model; syntax rules read the parsed sources.

/// Implements `Rule` for a rule that is only a syntax visitor (`Visit for Finder<Rule>`) over library code.
macro_rules! syntax_rule {
    ($rule:ident, $id:literal) => {
        impl crate::Rule for $rule {
            fn id(&self) -> &'static str {
                $id
            }

            fn check(&self, workspace: &crate::Workspace) -> Vec<crate::Violation> {
                super::scan_library(workspace, $id, |_, file| super::find::<Self>(file))
            }
        }
    };
}

mod adapter_name;
mod anyhow;
mod contract_suite;
mod dev_cycle;
mod external_types;
mod file_size;
mod layer;
mod locks;
mod params;
mod pub_api;
mod pub_mod;
mod statics;
mod string_match;
mod testkit_dev;
mod trait_size;
mod unknown_crate;

use std::marker::PhantomData;

use proc_macro2::Span;
use syn::visit::Visit;

use crate::{Rule, SourceFile, Violation, Workspace};

/// A finding inside one file: where, and what is wrong.
type Finding = (Span, String);

/// Every rule, in the order the LLD lists them.
pub fn all() -> Vec<Box<dyn Rule>> {
    vec![
        Box::new(layer::Layer),
        Box::new(unknown_crate::UnknownCrate),
        Box::new(dev_cycle::DevCycle),
        Box::new(testkit_dev::TestkitDev),
        Box::new(anyhow::Anyhow),
        Box::new(statics::Statics),
        Box::new(locks::Locks),
        Box::new(pub_mod::PubMod),
        Box::new(params::Params),
        Box::new(file_size::FileSize),
        Box::new(trait_size::TraitSize),
        Box::new(string_match::StringMatch),
        Box::new(external_types::ExternalTypes),
        Box::new(adapter_name::AdapterName),
        Box::new(contract_suite::ContractSuite),
    ]
}

/// Library code only: test files are skipped and `#[cfg(test)]` items removed.
fn library_files(workspace: &Workspace) -> impl Iterator<Item = (&SourceFile, &syn::File)> {
    workspace.sources().iter().filter_map(|source| source.library().map(|file| (source, file)))
}

/// Runs `scan` over every library file and reports its findings as rule `id`.
fn scan_library<F>(workspace: &Workspace, id: &'static str, scan: F) -> Vec<Violation>
where
    F: Fn(&SourceFile, &syn::File) -> Vec<Finding>,
{
    let found = library_files(workspace).flat_map(|(source, file)| {
        scan(source, file).into_iter().map(move |(span, message)| Violation::new(id, source.at(span), message))
    });
    found.collect()
}

/// The syntax visitor of rule `R`: each rule implements `Visit` for its own `Finder<R>`.
struct Finder<R> {
    found: Vec<Finding>,
    rule: PhantomData<R>,
}

impl<R> Finder<R> {
    fn push(&mut self, span: Span, message: String) {
        self.found.push((span, message));
    }
}

/// Walks `file` with rule `R`'s visitor and returns what it found.
fn find<R>(file: &syn::File) -> Vec<Finding>
where
    for<'a> Finder<R>: Visit<'a>,
{
    let mut finder = Finder::<R> { found: Vec::new(), rule: PhantomData };
    finder.visit_file(file);
    finder.found
}
