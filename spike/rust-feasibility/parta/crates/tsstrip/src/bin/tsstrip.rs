//! Prints the stripped JS for a .ts file (baseline binary: oxc only, no JS engine).
fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| "hooks/sign.ts".to_owned());
    let path = std::path::Path::new(&path);
    match std::fs::read_to_string(path).map_err(|e| e.to_string()).and_then(|s| tsstrip::strip_types(path, &s).map_err(|e| e.to_string())) {
        Ok(m) => print!("{}", m.code),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
