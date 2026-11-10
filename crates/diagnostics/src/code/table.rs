//! The code table: the LLD's "Diagnostic codes" section plus D137 and D139.
//! Codes are listed in numeric order; a new code takes the next free number in its range.

use super::Code;
use crate::Severity;

/// Declares one associated const per code, documented with its title, and [`Code::ALL`].
macro_rules! code_table {
    ($($name:ident = $number:literal, $severity:ident, $title:literal;)*) => {
        impl Code {
            $(
                #[doc = $title]
                pub const $name: Self = Self { number: $number, severity: Severity::$severity, title: $title };
            )*

            /// Every code, in numeric order.
            pub const ALL: &'static [Self] = &[$(Self::$name,)*];
        }
    };
}

code_table! {
    // V1xxx: syntax
    V1001 = 1001, Error, "File is not valid YAML or JSON, or its suffix is not a known kind";
    V1002 = 1002, Error, "YAML structure error (indentation, a mapping expected)";
    V1003 = 1003, Error, "`syntaxVersion` newer than this Vibranium, or missing";
    V1004 = 1004, Error, "Unknown key";
    V1005 = 1005, Error, "Required key missing";
    V1006 = 1006, Error, "Duplicate key in a map";
    V1007 = 1007, Error, "YAML alias or anchor not supported";
    V1008 = 1008, Error, "YAML tag not supported";
    V1009 = 1009, Error, "Map key is not text";
    V1010 = 1010, Error, "More than one YAML document in a file";
    V1011 = 1011, Error, "Nesting deeper than 128 levels";
    V1012 = 1012, Error, "File cannot be read (permissions, not UTF-8, outside the project, bad name)";
    V1013 = 1013, Error, "Hook file is not valid TypeScript or JavaScript";
    // V2xxx: references and types
    V2001 = 2001, Error, "Duplicate ID";
    V2002 = 2002, Error, "Unknown auth, profile, dataset or generator";
    V2003 = 2003, Error, "Unknown api or test";
    V2004 = 2004, Error, "`$file` or snapshot reference not found";
    V2005 = 2005, Error, "Hook export not found";
    V2006 = 2006, Error, "Reserved ID";
    V2007 = 2007, Error, "File ID doesn't match its path";
    V2008 = 2008, Error, "Hook import not allowed";
    V2101 = 2101, Error, "Var used but never set for some profile";
    V2102 = 2102, Error, "`${item}`, `${example}` or `${iteration}` used outside its block";
    V2103 = 2103, Error, "Unknown example or dataset column";
    V2105 = 2105, Error, "Template or CEL root not available at that spot";
    V2106 = 2106, Error, "`${secret.X}` names a secret `.env.schema` does not declare";
    V2107 = 2107, Error, "`${env.X}` names a secret declared in `.env.schema`";
    V2201 = 2201, Error, "Exactly one of `call` or `steps` needed";
    V2205 = 2205, Error, "Cycle through `test:` reuse";
    V2210 = 2210, Error, "Shared action saves values but has no `id`";
    V2301 = 2301, Error, "Invalid duration or size";
    V2302 = 2302, Error, "Wrong value type";
    V2303 = 2303, Error, "Invalid status code or class";
    V2304 = 2304, Error, "Pipe or path cannot apply to that value's type";
    V2305 = 2305, Error, "Template or CEL does not parse";
    V2306 = 2306, Error, "Matcher argument invalid";
    V2308 = 2308, Error, "Unknown pipe or wrong pipe arguments";
    V2309 = 2309, Error, "fromRegExp pattern not supported";
    // V3xxx: lifecycle
    V3101 = 3101, Error, "No profile defined, or no active profile for this run";
    V3102 = 3102, Error, "More than one default profile";
    V3103 = 3103, Error, "`skip` without a reason";
    V3104 = 3104, Warning, "Skip expired; the test runs again";
    V3201 = 3201, Warning, "`PUT`, `PATCH` or `DELETE` uses a shared value from `beforeRun` or `beforeAll`";
    V3202 = 3202, Warning, "`cache` on a per-test action";
    V3203 = 3203, Warning, "Call to a `deprecated` api";
    V3204 = 3204, Warning, "Unquoted value that YAML cut at `  # ` (a comment)";
    V3205 = 3205, Warning, "A test-level `body` default reaches a GET or HEAD step";
    V3206 = 3206, Warning, "A test-level default that no step uses";
    V3207 = 3207, Warning, "A key a `test:` action ignores";
    // V4xxx: conventions
    V4001 = 4001, Warning, "Test ID breaks the convention";
    V4002 = 4002, Warning, "Api ID breaks the convention";
    V4003 = 4003, Warning, "File name or folder breaks the convention";
    V4004 = 4004, Warning, "Tag, profile or dataset name breaks the convention";
    // V5xxx: run time
    V5101 = 5101, Error, "Expectation failed";
    V5102 = 5102, Error, "CEL check false";
    V5103 = 5103, Error, "Custom check failed or threw";
    V5104 = 5104, Error, "Timeout";
    V5105 = 5105, Error, "Transport error";
    V5106 = 5106, Error, "A lifecycle action this test needs failed";
    V5107 = 5107, Warning, "Cleanup failed";
    V5108 = 5108, Warning, "A dynamically saved value is used but not declared in `provides`";
    V5109 = 5109, Error, "A value declared in `provides` was not saved";
    V5110 = 5110, Error, "Command exited non-zero";
    V5111 = 5111, Error, "Template failed to render at run time";
    V5112 = 5112, Error, "Dataset row missing a column";
    V5113 = 5113, Warning, "No cache key: the cross-run cache is off";
    V5114 = 5114, Error, "Credentials could not be obtained";
    V5115 = 5115, Error, "Hook threw or returned a value that is not JSON";
    V5116 = 5116, Error, "Hook ran past its time or memory budget";
    V5117 = 5117, Warning, "An alert could not be sent";
    V5118 = 5118, Error, "A plugin failed, ran past its deadline or asked for what it was not granted";
    V5119 = 5119, Error, "A plugin cannot be confined to its permissions";
}
