use std::collections::HashMap;

fn main() {
    let mut builder = env_logger::Builder::from_default_env();
    builder.target(env_logger::Target::Stderr);
    let _ = builder.try_init();

    let env_vars: HashMap<String, String> = std::env::vars().collect();
    let mut stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    let exit_code = rudelblinken_bindgen::run_cli(
        std::env::args_os(),
        &env_vars,
        &mut stdin,
        &mut stdout,
        &mut stderr,
    );
    std::process::exit(exit_code);
}
