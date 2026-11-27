use super::Tools;

const READING: [&str; 6] =
    ["check", "explain_failure", "get_definition", "list_definitions", "query_history", "search"];
const ACTING: [&str; 3] = ["call_api", "run_tests", "write_definition"];

#[test]
fn the_standard_tools_are_the_nine_the_design_names() {
    let specs = Tools::standard().specs();
    let mut names: Vec<&str> = specs.iter().map(|tool| tool.name.as_ref()).collect();
    names.sort_unstable();
    let mut wanted = [READING.as_slice(), ACTING.as_slice()].concat();
    wanted.sort_unstable();
    assert_eq!(names, wanted);
}

#[test]
fn each_tool_says_whether_it_only_reads_and_takes_an_object() {
    for tool in Tools::standard().specs() {
        let hints = tool.annotations.clone().unwrap();
        let reads = READING.contains(&tool.name.as_ref());
        assert_eq!(hints.read_only_hint, Some(reads), "{}", tool.name);
        assert_eq!(hints.destructive_hint, Some(!reads), "{}", tool.name);
        assert_eq!(tool.input_schema["type"], "object", "{}", tool.name);
        assert!(tool.description.as_deref().is_some_and(|text| !text.is_empty()), "{}", tool.name);
    }
}

#[test]
fn a_tool_is_found_by_its_name_only() {
    let tools = Tools::standard();
    assert_eq!(tools.get("run_tests").map(|tool| tool.name()), Some("run_tests"));
    assert!(tools.get("run").is_none());
}

/// What an agent sends cannot move the report folder: no tool takes sets or an `out`, so the
/// only `--set` values are `vc mcp --set`'s, which the person chose (D327, D685). A tool that
/// takes them must refuse a `reports.dir` outside the project first, as `vc serve` does.
#[test]
fn no_tool_takes_sets_or_a_report_folder_from_the_agent() {
    for tool in Tools::standard().specs() {
        let properties = tool.input_schema.get("properties").and_then(|found| found.as_object()).cloned();
        for key in properties.unwrap_or_default().keys() {
            assert!(!["set", "sets", "out", "reports", "reportsDir"].contains(&key.as_str()), "{}: {key}", tool.name);
        }
    }
}
