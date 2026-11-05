use std::collections::BTreeSet;

use vc_diagnostics::{Code, Diagnostic};
use vc_model::{ContentHash, HookRef, RedactRule, SourceFile, SourcePath};

use crate::{CompileInput, Compiled};

const CONFIG: &str = "syntaxVersion: 2\nproject: shop\nprofiles:\n  - id: dev\n    default: true\n";
const APIS: &str = "baseUrl: http://x\nbeforeRequest:\n  - function: ../../hooks/sign.ts#sign\napis:\n  - id: get\n    path: /users/1\n";
const SIGN: &str = "import { pick } from \"./util\";\nexport function sign(r) { return pick(r); }\nexport function check() { return true; }\n";
const UTIL: &str = "export const pick = (r: unknown) => r;\n";

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) }
}

/// The config, the users apis and `tests`, with `given` other files, and `listed` as the assets.
fn compile(tests: &str, given: &[(&str, &str)], listed: &[&str]) -> Compiled {
    let mut files = vec![
        file("vibranium.config.yaml", CONFIG),
        file("services/users/users.api.yaml", APIS),
        file("services/users/crud.test.yaml", tests),
    ];
    files.extend(given.iter().map(|(path, text)| file(path, text)));
    let assets: BTreeSet<SourcePath> = listed.iter().map(|path| SourcePath::new(path).unwrap()).collect();
    crate::compile(CompileInput { files, assets: Some(assets), ..CompileInput::default() })
}

/// The hook diagnostics, with their lines.
fn hook_codes(compiled: &Compiled) -> Vec<(Code, u32)> {
    let hooks = [Code::V1013, Code::V2004, Code::V2005, Code::V2008];
    let found = compiled.diagnostics.iter().filter(|found| hooks.contains(&found.code));
    found.map(|found| (found.code, found.primary.span.start.line)).collect()
}

fn only(compiled: &Compiled, code: Code) -> &Diagnostic {
    let mut found = compiled.diagnostics.iter().filter(|found| found.code == code);
    let first = found.next().unwrap_or_else(|| panic!("no {code:?}: {:?}", compiled.diagnostics));
    assert!(found.next().is_none());
    first
}

fn tests(function: &str) -> String {
    format!("tests:\n  - id: getsUser\n    call: users#get\n    expect:\n      custom:\n        - function: {function}\n")
}

#[test]
fn wants_each_hook_file_reached_until_all_are_given_then_keeps_their_modules() {
    let text = tests("../../hooks/sign.ts#check");
    let listed = ["hooks/sign.ts", "hooks/util.ts"];
    let first = compile(&text, &[], &listed);
    assert_eq!(hook_codes(&first), []);
    assert_eq!(first.hooks_wanted, [SourcePath::new("hooks/sign.ts").unwrap()]);
    let second = compile(&text, &[("hooks/sign.ts", SIGN)], &listed);
    assert_eq!(second.hooks_wanted, [SourcePath::new("hooks/util.ts").unwrap()], "an import without its extension");
    let third = compile(&text, &[("hooks/sign.ts", SIGN), ("hooks/util.ts", UTIL)], &listed);
    assert!(third.hooks_wanted.is_empty() && hook_codes(&third).is_empty(), "{:?}", third.diagnostics);
    let program = third.program.unwrap();
    let paths: Vec<&str> = program.hooks.iter().map(|(path, _)| path).collect();
    assert_eq!(paths, ["hooks/sign.ts", "hooks/util.ts"]);
    assert!(!program.hooks.get("hooks/util.ts").unwrap().js.contains("unknown"), "types are stripped");
}

#[test]
fn a_missing_export_is_v2005_with_the_close_one_as_a_fix() {
    let compiled = compile(&tests("../../hooks/sign.ts#chek"), &[("hooks/sign.ts", SIGN), ("hooks/util.ts", UTIL)], &[]);
    let found = only(&compiled, Code::V2005);
    assert_eq!(found.primary.span.start.line, 6);
    assert_eq!(found.help, ["did you mean `check`?"]);
    let edit = &found.fixes[0].edits[0];
    assert_eq!((edit.new_text.as_str(), edit.range.len()), ("check", 4), "only the export is replaced");
    let none = compile(&tests("../../hooks/sign.ts"), &[("hooks/sign.ts", SIGN), ("hooks/util.ts", UTIL)], &[]);
    assert!(only(&none, Code::V2005).help[0].contains("`check`, `sign`"));
}

#[test]
fn a_star_export_hides_which_names_a_file_has() {
    let given = [("hooks/sign.ts", "export * from \"./util.ts\";\nexport function sign() {}\n"), ("hooks/util.ts", UTIL)];
    assert_eq!(hook_codes(&compile(&tests("../../hooks/sign.ts#anything"), &given, &[])), []);
}

#[test]
fn a_hook_path_to_no_file_is_v2004_and_to_a_data_file_is_v2008() {
    let listed = ["hooks/sign.ts", "datasets/a.json"];
    let missing = compile(&tests("../../hooks/sing.ts#check"), &[], &listed);
    let found = only(&missing, Code::V2004);
    assert_eq!(found.help, ["did you mean `../../hooks/sign.ts`?"]);
    assert_eq!(found.fixes.len(), 1);
    let outside = compile(&tests("../../../sign.ts#check"), &[], &listed);
    assert_eq!(only(&outside, Code::V2004).primary.text, "leads out of the project");
    let data = compile(&tests("../../datasets/a.json#check"), &[], &listed);
    assert_eq!(hook_codes(&data), [(Code::V2008, 6)]);
}

#[test]
fn a_hook_imports_only_relative_files_and_the_standard_module() {
    let sign = concat!(
        "import type { Hook } from \"@vibranium/sdk\";\n",
        "import { hmacSha256Hex } from \"vibranium:std\";\n",
        "import { readFileSync } from \"node:fs\";\n",
        "import _ from \"lodash\";\n",
        "import { pick } from \"./gone.ts\";\n",
        "export const check: Hook = async () => [readFileSync, _, pick, hmacSha256Hex, await import(\"./util.ts\")];\n",
        "export const sign = check;\n",
    );
    let compiled = compile(&tests("../../hooks/sign.ts#check"), &[("hooks/sign.ts", sign)], &["hooks/util.ts"]);
    let mut found = hook_codes(&compiled);
    found.sort_unstable();
    assert_eq!(found, [(Code::V2004, 5), (Code::V2008, 3), (Code::V2008, 4), (Code::V2008, 6)]);
    let node = compiled.diagnostics.iter().find(|found| found.primary.span.start.line == 3).unwrap();
    assert!(node.help[0].contains("vibranium:std"), "{:?}", node.help);
}

#[test]
fn every_kind_of_reference_is_linked_and_a_broken_file_only_reports_v1013() {
    let text = "beforeAll:\n  - function: ../../hooks/bad.ts#seed\ntests:\n  - id: getsUser\n    call: users#get\n    cache:\n      check:\n        function: ../../hooks/nope.ts#a\n    poll:\n      until:\n        custom:\n          - function: ../../hooks/none.ts#b\n";
    let compiled = compile(text, &[("hooks/bad.ts", "export function (")], &[]);
    let mut found = hook_codes(&compiled);
    found.sort_unstable();
    assert_eq!(found, [(Code::V1013, 1), (Code::V2004, 3), (Code::V2004, 8), (Code::V2004, 12)]);
}

#[test]
fn a_redact_rule_function_is_linked_from_the_project_root() {
    let config = format!("{CONFIG}redact:\n  rules:\n    - function: hooks/mask.ts#cards\n    - function: hooks/mask.ts#nope\n");
    let mut files = vec![file("vibranium.config.yaml", &config), file("services/users/users.api.yaml", "apis: []\n")];
    let first = crate::compile(CompileInput { files: files.clone(), ..CompileInput::default() });
    assert_eq!(first.hooks_wanted, [SourcePath::new("hooks/mask.ts").unwrap()]);
    files.push(file("hooks/mask.ts", "export const cards = (text: string) => text;\n"));
    let second = crate::compile(CompileInput { files, ..CompileInput::default() });
    assert_eq!(hook_codes(&second), [(Code::V2005, 9)]);
    let config = &crate::compile(CompileInput {
        files: vec![file("vibranium.config.yaml", &format!("{CONFIG}redact:\n  rules:\n    - function: hooks/mask.ts#cards\n")), file("hooks/mask.ts", "export const cards = 1;\n")],
        ..CompileInput::default()
    })
    .program
    .unwrap()
    .config;
    let cards = HookRef { file: "hooks/mask.ts".to_owned(), export: "cards".to_owned() };
    assert_eq!(config.redact_rules, [RedactRule::Function(cards)]);
}
