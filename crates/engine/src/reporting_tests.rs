use std::path::{Path, PathBuf};

use vc_model::Program;

use super::Chosen;
use crate::EngineError;
use crate::settings::{Named, Settings};

fn program(formats: &[&str], dir: Option<&str>) -> Program {
    let mut program = Program::default();
    program.config.report_formats = formats.iter().map(|name| (*name).to_owned()).collect();
    program.config.reports_dir = dir.map(str::to_owned);
    program
}

fn flag(names: &[&str]) -> Settings {
    let formats = Named { names: names.iter().map(|name| (*name).to_owned()).collect(), origin: "--report".into() };
    Settings { formats: Some(formats), ..Settings::default() }
}

/// The choice for a project at `/p`.
fn choose(console: Option<&str>, settings: &Settings, program: &Program) -> Result<Chosen, EngineError> {
    super::choose(console, settings, program).map(|chosen| chosen.under(Path::new("/p")))
}

fn refusal(result: Result<Chosen, EngineError>) -> (String, Option<String>) {
    let error = result.unwrap_err();
    (error.to_string(), error.help())
}

#[test]
fn defaults_to_spec_and_ctrf_under_the_reports_folder() {
    let chosen = choose(None, &Settings::default(), &program(&[], None)).unwrap();
    let dir = PathBuf::from("/p/.vibranium/reports");
    assert_eq!(chosen, Chosen { console: "spec".into(), files: vec!["ctrf".into()], dir });
}

#[test]
fn the_config_formats_and_folder_apply_when_no_layer_names_any() {
    let chosen = choose(Some("json"), &Settings::default(), &program(&["junit"], Some("out"))).unwrap();
    assert_eq!((chosen.console.as_str(), chosen.files), ("json", vec!["junit".to_owned()]));
    assert_eq!(chosen.dir, PathBuf::from("/p/out"));
}

#[test]
fn a_flag_wins_over_the_config_and_an_absolute_out_stays() {
    let settings = Settings { reports_dir: Some("/tmp/r".into()), ..flag(&["ctrf", "junit"]) };
    let chosen = choose(None, &settings, &program(&["junit"], Some("out"))).unwrap();
    assert_eq!((chosen.files.len(), chosen.dir), (2, PathBuf::from("/tmp/r")));
}

#[test]
fn a_planned_reporter_is_not_supported_yet() {
    let (reason, help) = refusal(choose(Some("dots"), &Settings::default(), &program(&[], None)));
    assert_eq!((reason.as_str(), help), ("--reporter: `dots` is not supported yet", None));
    let (reason, _) = refusal(choose(None, &Settings::default(), &program(&["html"], None)));
    assert_eq!(reason, "vibranium.config.yaml: `html` is not supported yet");
}

#[test]
fn a_name_on_the_wrong_flag_points_to_the_right_one() {
    let (reason, help) = refusal(choose(Some("junit"), &Settings::default(), &program(&[], None)));
    assert_eq!(reason, "--reporter: `junit` is a report file");
    assert_eq!(help.as_deref(), Some("pass it to `--report junit`"));
    let (reason, help) = refusal(choose(None, &flag(&["spec"]), &program(&[], None)));
    assert_eq!(reason, "--report: `spec` is a console format");
    assert_eq!(help.as_deref(), Some("pass it to `--reporter spec`"));
}

#[test]
fn an_unknown_name_lists_the_known_ones() {
    let (reason, help) = refusal(choose(Some("tap"), &Settings::default(), &program(&[], None)));
    assert_eq!(reason, "--reporter: no console format `tap`");
    assert_eq!(help.as_deref(), Some("the console formats are spec, json"));
    let (reason, help) = refusal(choose(None, &flag(&["xml"]), &program(&[], None)));
    assert_eq!(reason, "--report: no report format `xml`");
    assert_eq!(help.as_deref(), Some("the report formats are ctrf, junit"));
}
