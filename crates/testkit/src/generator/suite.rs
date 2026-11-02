use super::Service;
use super::cases::{CASES, PREPARE};
use super::rng::{Rng, fill, title};

/// The areas a service's test files cover, in order; the first one is always `crud`.
pub const AREAS: &[&str] = &["crud", "search", "lifecycle", "errors"];

/// File-level keys and the shared blocks: a seed record for the file and a fresh one per test.
const HEAD: &str = "name: %title% %area%
tags: [%svc%, %area%]
vars:
  label: %area%-%word%
  quantity: %qty%
profiles:
  - id: staging
    vars:
      quantity: %double%

beforeAll:
  - id: seed
    call: %svc%#create
    vars:
      label: seed-%word%

beforeEach:
  - %each%

afterAll:
  - command: echo %svc% %area% done

tests:
";

/// The test file for area number `area` of `service`.
pub fn render(service: &Service<'_>, area: usize, rng: &mut Rng) -> String {
    let area_name = AREAS[area % AREAS.len()];
    let qty = rng.between(1, 9);
    let numbers = [qty.to_string(), (qty * 2).to_string(), rng.between(5, 50).to_string(), rng.between(2, 5).to_string()];
    let each = if area == 0 { format!("call: {}#create", service.name) } else { format!("test: {}.crud#preparesItem", service.name) };
    let values = [
        ("svc", service.name),
        ("other", service.other),
        ("first", service.first),
        ("title", &title(service.name)),
        ("area", area_name),
        ("each", &each),
        ("word", rng.word()),
        ("qty", &numbers[0]),
        ("double", &numbers[1]),
        ("page", &numbers[2]),
        ("count", &numbers[3]),
    ];
    let mut text = fill(HEAD, &values);
    if area == 0 {
        text.push_str(&fill(PREPARE, &values));
    }
    let count = rng.between(6, 10);
    for index in rng.subset(CASES.len(), count) {
        text.push_str(&fill(CASES[index], &values));
    }
    text
}
