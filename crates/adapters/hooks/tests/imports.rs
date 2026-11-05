//! Hooks that import other hook files and `vibranium:std`.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use vc_adapter_hooks::QuickJsHooks;
use vc_interfaces::{HookCall, HookError, HookRunner};
use vc_model::{HookModule, HookModules, HookRef};

/// The entry module; its JavaScript lines came from TypeScript lines 1, 2, 5 and 6.
const SIGN_JS: &str = "import { hmacSha256Hex } from 'vibranium:std';\n\
                       import { prefix, fail } from './lib/util';\n\
                       export function sign(input) { return prefix + hmacSha256Hex(input.key, input.data); }\n\
                       export function broken() { return fail(); }\n";

/// The imported module; its JavaScript lines came from TypeScript lines 2, 4, 7 and 8.
const UTIL_JS: &str = "export const prefix = 'sig=';\n\
                       export function fail() {\n\
                       \x20 throw new Error('from util');\n\
                       }\n";

fn call(export: &str, input: Value) -> HookCall {
    let modules = HookModules::new(BTreeMap::from([
        ("hooks/sign.ts".to_owned(), HookModule { js: SIGN_JS.to_owned(), lines: vec![1, 2, 5, 6] }),
        ("hooks/lib/util.ts".to_owned(), HookModule { js: UTIL_JS.to_owned(), lines: vec![2, 4, 7, 8] }),
    ]));
    HookCall::new(modules, HookRef { file: "hooks/sign.ts".to_owned(), export: export.to_owned() }, input)
}

#[tokio::test]
async fn a_hook_imports_a_relative_module_and_signs_with_the_standard_library() {
    let runner = QuickJsHooks::new();
    let input = json!({"key": "Jefe", "data": "what do ya want for nothing?"});
    let signed = runner.call(call("sign", input)).await.map(|output| output.value);
    let expected = "sig=5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843";
    assert_eq!(signed, Ok(json!(expected)));
}

#[tokio::test]
async fn a_throw_in_an_imported_module_names_that_module_s_typescript_line() {
    let runner = QuickJsHooks::new();
    let threw = runner.call(call("broken", Value::Null)).await;
    assert_eq!(threw, Err(HookError::Threw { message: "Error: from util".to_owned(), line: Some(7) }));
}

#[tokio::test]
async fn a_hook_cannot_import_outside_relative_paths_and_the_standard_library() {
    let runner = QuickJsHooks::new();
    let modules = HookModules::new(BTreeMap::from([(
        "hooks/bad.ts".to_owned(),
        HookModule { js: "import fs from 'node:fs';\nexport function go() { return 1; }\n".to_owned(), lines: vec![1, 2] },
    )]));
    let hook = HookRef { file: "hooks/bad.ts".to_owned(), export: "go".to_owned() };
    let refused = runner.call(HookCall::new(modules, hook, Value::Null)).await;
    assert!(matches!(&refused, Err(HookError::Threw { message, .. }) if message.contains("vibranium:std")), "{refused:?}");
}
