use vc_diagnostics::Code;
use vc_model::{ContentHash, SourceFile, SourceFileId, SourcePath};

use super::{Script, is_script, position, strip};

fn run(path: &str, text: &str) -> (Option<Script>, vc_diagnostics::Diagnostics) {
    let file =
        SourceFile { path: SourcePath::new(path).unwrap(), text: text.to_owned(), hash: ContentHash::new([0; 32]) };
    strip(SourceFileId::new(3), &file)
}

const SIGN: &str = "\
import type { BeforeRequestHook } from \"@vibranium/sdk\";
import { Request } from \"@vibranium/sdk\";
import { hmacSha256Hex } from \"vibranium:std\";
import { pick } from \"./util.ts\";

interface Signed { signature: string }

export const sign: BeforeRequestHook = ({ request }) => {
  const signed: Signed = { signature: hmacSha256Hex(\"k\", request.path) };
  throw new Error(pick(signed));
};
export type Shape = { a: number };
export default function check(): boolean { return true; }
export { sign as alias } from \"./other.ts\";
export async function later() { return import(\"./lazy.ts\"); }
";

#[test]
fn strips_types_and_reads_value_exports_and_the_imports_left() {
    let (script, diagnostics) = run("hooks/sign.ts", SIGN);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let script = script.unwrap();
    assert_eq!(script.exports, ["alias", "default", "later", "sign"]);
    assert!(!script.stars);
    assert!(!script.module.js.contains("interface") && !script.module.js.contains("BeforeRequestHook"));
    let imports: Vec<(&str, bool)> = script.imports.iter().map(|i| (i.specifier.as_str(), i.dynamic)).collect();
    assert_eq!(imports, [("vibranium:std", false), ("./util.ts", false), ("./other.ts", false), ("./lazy.ts", true)]);
    assert_eq!(script.imports[0].span.file, SourceFileId::new(3));
    assert_eq!((script.imports[0].span.start.line, script.imports[0].span.start.column), (3, 31));
}

#[test]
fn maps_each_javascript_line_to_its_typescript_line() {
    let script = run("hooks/sign.ts", SIGN).0.unwrap();
    let thrown = script.module.js.lines().position(|line| line.contains("throw new Error")).unwrap();
    assert_eq!(script.module.ts_line(u32::try_from(thrown + 1).unwrap()), Some(10), "{}", script.module.js);
    assert_eq!(script.module.lines.len(), script.module.js.lines().count());
}

#[test]
fn a_file_that_does_not_parse_is_v1013_at_the_spot() {
    let (script, diagnostics) = run("hooks/bad.ts", "export const a = 1;\nexport function (\n");
    assert!(script.is_none());
    let found: Vec<_> = diagnostics.iter().map(|d| (d.code, d.primary.span.start.line)).collect();
    assert_eq!(found, [(Code::V1013, 2)]);
    assert!(diagnostics.iter().all(|d| d.message.contains("not valid TypeScript")));
}

#[test]
fn javascript_files_and_star_exports_are_read_too() {
    let (script, diagnostics) = run("hooks/all.mjs", "export * from './a.js';\nexport const b = 1;\n");
    assert!(diagnostics.is_empty());
    let script = script.unwrap();
    assert!(script.stars && script.exports == ["b"]);
    let (script, _) = run("hooks/ns.js", "export * as ns from './a.js';\n");
    assert!(script.is_some_and(|script| !script.stars && script.exports == ["ns"]));
}

#[test]
fn only_es_module_suffixes_are_hook_files() {
    for path in ["hooks/a.ts", "hooks/a.mts", "hooks/a.js", "hooks/a.mjs"] {
        assert!(is_script(path), "{path}");
    }
    for path in ["hooks/a.cjs", "hooks/a.tsx", "datasets/a.json", "a.api.yaml"] {
        assert!(!is_script(path), "{path}");
    }
}

#[test]
fn positions_count_lines_and_characters() {
    let text = "ab\nçd\n";
    assert_eq!((position(text, 0).line, position(text, 0).column), (1, 1));
    assert_eq!((position(text, 5).line, position(text, 5).column), (2, 2));
    assert_eq!(position(text, 4).offset, 3, "inside a character, the start of it");
    assert_eq!(position(text, 99).offset, text.len());
}

#[test]
fn a_tick_on_the_loop_s_line_comes_just_before_each_loop() {
    let text = "export function loops(xs: number[]): number {\n  let n = 0;\n  while (n < 1) n++;\n  do {\n    n++;\n  } while (n < 2);\n  for (let i = 0; i < 1; i++) {}\n  for (const x of xs) n += x;\n  outer: for (const k in xs) {\n    if (n > 9) continue outer;\n    while (n < 3) n++;\n  }\n  return n;\n}\n";
    let module = run("hooks/loops.ts", text).0.unwrap().module;
    let ticks: Vec<Option<u32>> = module
        .js
        .lines()
        .zip(1..)
        .filter(|(line, _)| line.trim() == "__vcTick();")
        .map(|(_, js_line)| module.ts_line(js_line))
        .collect();
    assert_eq!(ticks, [Some(3), Some(4), Some(7), Some(8), Some(9), Some(11)], "{}", module.js);
    assert!(module.js.contains("outer: for"), "a label stays on its loop: {}", module.js);
}
