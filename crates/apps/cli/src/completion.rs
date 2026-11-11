//! `vc completion <shell>` and the hidden `vc __complete <command> <word>` it calls (D180): the
//! `clap_complete` script for the commands and flags, plus a hook that asks `vc` for the
//! project's own IDs, profiles and tags. Each script completes both names, `vc` and
//! `vibranium`, and the hook asks the name that was typed; fish's `vibranium` lines ask
//! `vibranium` by name.

use std::io::Write;
use std::path::Path;

use clap::{Args, CommandFactory, ValueEnum};
use clap_complete::Shell as Generator;
use vc_engine::CompleteKind;

use crate::Outcome;
use crate::project;
use crate::stdout;

/// The shells `vc completion` writes a script for.
#[derive(Clone, Copy, ValueEnum)]
enum Shell {
    Bash,
    Zsh,
    Fish,
    Powershell,
}

#[derive(Args)]
pub struct CompletionArgs {
    /// The shell to complete in.
    shell: Shell,
}

#[derive(Args)]
pub struct CompleteArgs {
    /// The command being completed (`run`, `call`, …), or the flag before the word
    /// (`--profile`, `-t`).
    #[arg(allow_hyphen_values = true)]
    command: String,
    /// What has been typed of the word so far.
    #[arg(default_value = "", allow_hyphen_values = true)]
    word: String,
}

/// What `__complete` offers after each command or flag; test and file IDs after any other.
const KINDS: &[(&str, CompleteKind)] = &[
    ("call", CompleteKind::Apis),
    ("explain", CompleteKind::Ids),
    ("--profile", CompleteKind::Profiles),
    ("-t", CompleteKind::Tags),
    ("--tag", CompleteKind::Tags),
];

/// The first line of the generated zsh script, which `compinit` reads for the names it completes.
const ZSH_NAMES: &str = "#compdef vc\n";

/// The generated PowerShell completer is kept under this name, so the hook can call it first.
const POWERSHELL_STATIC: &str = "Register-ArgumentCompleter -Native -CommandName 'vc' -ScriptBlock {";

const BASH_HOOK: &str = r#"
_vc_ids() {
    _vc "$@"
    local cur="${COMP_WORDS[COMP_CWORD]}" kind="${COMP_WORDS[1]}" prev="${COMP_WORDS[COMP_CWORD-1]}"
    case "$prev" in --profile|-t|--tag) kind="$prev" ;; esac
    local typed="${COMP_WORDS[0]}"
    [[ $typed == "~/"* ]] && typed="$HOME/${typed#\~/}"
    if [[ $COMP_CWORD -ge 2 && "$cur" != -* ]]; then
        COMPREPLY+=($(compgen -W "$("$typed" __complete "$kind" "$cur" 2>/dev/null)" -- "$cur"))
    fi
}
complete -F _vc_ids -o bashdefault -o default vc vibranium
"#;

const ZSH_HOOK: &str = r#"
_vc_ids() {
    _vc "$@"
    local kind=$words[2] prev=$words[CURRENT-1]
    [[ $prev == (--profile|-t|--tag) ]] && kind=$prev
    if (( CURRENT > 2 )) && [[ $words[CURRENT] != -* ]]; then
        local -a ids
        ids=(${(f)"$(${~words[1]} __complete $kind $words[CURRENT] 2>/dev/null)"})
        (( $#ids )) && compadd -Q -- $ids
    fi
}
compdef _vc_ids vc vibranium
"#;

const FISH_HOOK: &str = r"
complete -c vc -f -n '__fish_seen_subcommand_from run watch plan list explain call skip unskip rename' -a '(command (commandline -opc)[1] __complete (commandline -opc)[2] (commandline -ct))'
complete -c vc -l profile -x -a '(command (commandline -opc)[1] __complete --profile (commandline -ct))'
complete -c vc -s t -l tag -x -a '(command (commandline -opc)[1] __complete --tag (commandline -ct))'
";

const POWERSHELL_HOOK: &str = r"
Register-ArgumentCompleter -Native -CommandName 'vc', 'vibranium' -ScriptBlock {
    param($wordToComplete, $commandAst, $cursorPosition)
    & $global:VcStaticCompleter $wordToComplete $commandAst $cursorPosition
    $words = $commandAst.CommandElements
    if ($words.Count -ge 2 -and -not $wordToComplete.StartsWith('-')) {
        $kind = $words[1].ToString()
        $prev = $words[$words.Count - 1].ToString()
        if ($prev -eq $wordToComplete -and $words.Count -ge 3) { $prev = $words[$words.Count - 2].ToString() }
        if ($prev -in '--profile', '-t', '--tag') { $kind = $prev }
        & $words[0].ToString() __complete $kind $wordToComplete 2>$null | ForEach-Object {
            [CompletionResult]::new($_, $_, [CompletionResultType]::ParameterValue, $_)
        }
    }
}
";

/// `vc completion bash|zsh|fish|powershell`: exit 0; clap refuses any other shell.
pub fn run(args: &CompletionArgs) -> anyhow::Result<Outcome> {
    let script = match args.shell {
        Shell::Bash => generated(Generator::Bash, "vc")? + BASH_HOOK,
        Shell::Zsh => generated(Generator::Zsh, "vc")?.replacen(ZSH_NAMES, "#compdef vc vibranium\n", 1) + ZSH_HOOK,
        Shell::Fish => fish()?,
        Shell::Powershell => {
            let script = generated(Generator::PowerShell, "vc")?;
            script.replacen(POWERSHELL_STATIC, "$global:VcStaticCompleter = {", 1) + POWERSHELL_HOOK
        }
    };
    write!(stdout::out(), "{script}")?;
    Ok(Outcome::Done)
}

/// The `clap_complete` script for the commands and flags, completing `name`.
fn generated(generator: Generator, name: &str) -> anyhow::Result<String> {
    let mut script = Vec::new();
    clap_complete::generate(generator, &mut crate::Cli::command(), name, &mut script);
    Ok(String::from_utf8(script)?)
}

/// fish completes a command only by the lines that name it, so `vibranium` gets lines of its
/// own, and its hook asks `vibranium` itself, never the `vc` first on `PATH`.
fn fish() -> anyhow::Result<String> {
    let long = FISH_HOOK
        .replace("complete -c vc ", "complete -c vibranium ")
        .replace("command (commandline -opc)[1]", "command vibranium");
    let (short, long_static) = (generated(Generator::Fish, "vc")?, generated(Generator::Fish, "vibranium")?);
    Ok(format!("{short}{FISH_HOOK}{long_static}{long}"))
}

/// `vc __complete <command> <word>`: one candidate a line; nothing, and exit 0, outside a
/// project or in one with errors.
pub fn complete(args: &CompleteArgs, project: Option<&Path>) -> anyhow::Result<Outcome> {
    let Ok(located) = project::locate(project)? else {
        return Ok(Outcome::Done);
    };
    let kind = KINDS.iter().find(|(name, _)| *name == args.command).map_or(CompleteKind::Tests, |(_, kind)| *kind);
    let mut out = stdout::out();
    for word in located.engine.complete(kind, &args.word) {
        writeln!(out, "{word}")?;
    }
    Ok(Outcome::Done)
}
