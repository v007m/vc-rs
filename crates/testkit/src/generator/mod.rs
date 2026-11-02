//! A deterministic project generator for benchmarks and large-project tests.
//!
//! `generate_project(seed, files)` writes a realistic project of exactly `files` files: the config with
//! its profiles, then one folder per service holding `<service>.api.yaml` and up to four
//! `<area>.test.yaml` files. The same seed always gives the same bytes; nothing reads the clock.

mod api;
mod cases;
mod config;
mod rng;
mod suite;

use crate::MemorySources;
use rng::{Rng, service_name};

/// Test files per service, at most; one per area.
const TESTS_PER_SERVICE: usize = suite::AREAS.len();

/// One generated file: its project-relative path and its text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedFile {
    pub path: String,
    pub text: String,
}

/// A generated project, its files in a stable order with the config first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedProject {
    files: Vec<GeneratedFile>,
}

/// A service and the neighbours its tests call into.
struct Service<'a> {
    name: &'a str,
    other: &'a str,
    first: &'a str,
}

impl GeneratedProject {
    /// The files, the config first.
    #[must_use]
    pub fn files(&self) -> &[GeneratedFile] {
        &self.files
    }

    /// The files as an in-memory source provider.
    #[must_use]
    pub fn sources(&self) -> MemorySources {
        let pairs: Vec<(&str, &[u8])> =
            self.files.iter().map(|file| (file.path.as_str(), file.text.as_bytes())).collect();
        MemorySources::new(&pairs)
    }

    /// The same project with its last file edited, as after a one-line change in an editor.
    #[must_use]
    pub fn with_one_file_changed(&self) -> Self {
        let mut files = self.files.clone();
        if let Some(last) = files.last_mut() {
            last.text = if last.text.starts_with("name: ") {
                last.text.replacen("name: ", "name: Edited ", 1)
            } else {
                format!("{}# edited\n", last.text)
            };
        }
        Self { files }
    }
}

/// A project of exactly `files` files, the same for the same `seed`.
#[must_use]
pub fn generate_project(seed: u64, files: usize) -> GeneratedProject {
    let mut rng = Rng::new(seed);
    let services = service_names(files.saturating_sub(1));
    let mut out = Vec::with_capacity(files);
    if files > 0 {
        out.push(GeneratedFile { path: "vibranium.config.yaml".to_owned(), text: config::render(&services, &mut rng) });
    }
    let mut left = files.saturating_sub(1);
    for (index, name) in services.iter().enumerate() {
        let service = Service { name, other: &services[(index + 1) % services.len()], first: &services[0] };
        let tests = left.saturating_sub(1).min(TESTS_PER_SERVICE);
        out.extend(service_files(&service, tests, &mut rng));
        left = left.saturating_sub(tests + 1);
    }
    GeneratedProject { files: out }
}

/// Enough service names for `files` service files: one api file plus up to four test files each.
fn service_names(files: usize) -> Vec<String> {
    (0..files.div_ceil(TESTS_PER_SERVICE + 1)).map(service_name).collect()
}

/// A service's api file and its first `tests` test files.
fn service_files(service: &Service<'_>, tests: usize, rng: &mut Rng) -> Vec<GeneratedFile> {
    let folder = format!("services/{}", service.name);
    let mut out = vec![GeneratedFile {
        path: format!("{folder}/{}.api.yaml", service.name),
        text: api::render(service.name, rng),
    }];
    for area in 0..tests {
        let path = format!("{folder}/{}.test.yaml", suite::AREAS[area]);
        out.push(GeneratedFile { path, text: suite::render(service, area, rng) });
    }
    out
}
