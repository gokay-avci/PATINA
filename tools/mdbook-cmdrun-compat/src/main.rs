use clap::{Arg, ArgMatches, Command};
use std::io;
use std::process;

use mdbook_cmdrun_compat::CmdRunCompat;
use mdbook_preprocessor::{parse_input, Preprocessor};

fn main() {
    let matches = make_app().get_matches();

    if let Some(sub_args) = matches.subcommand_matches("supports") {
        handle_supports(sub_args);
    } else if let Err(err) = handle_preprocessing() {
        eprintln!("{err}");
        process::exit(1);
    }
}

fn make_app() -> Command {
    Command::new("mdbook-cmdrun-compat")
        .about("mdBook 0.5-compatible cmdrun-style preprocessor")
        .subcommand(
            Command::new("supports")
                .arg(Arg::new("renderer").required(true))
                .about("Check whether a renderer is supported by this preprocessor"),
        )
}

fn handle_preprocessing() -> anyhow::Result<()> {
    let (ctx, book) = parse_input(io::stdin())?;
    let processed_book = CmdRunCompat.run(&ctx, book)?;
    serde_json::to_writer(io::stdout(), &processed_book)?;
    Ok(())
}

fn handle_supports(sub_args: &ArgMatches) -> ! {
    let renderer = sub_args
        .get_one::<String>("renderer")
        .expect("required argument");
    let supported = CmdRunCompat
        .supports_renderer(renderer)
        .unwrap_or(false);

    if supported {
        process::exit(0);
    } else {
        process::exit(1);
    }
}
