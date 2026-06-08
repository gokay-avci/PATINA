use clap::{Arg, ArgMatches, Command};
use mdbook_preprocessor::errors::Error;
use mdbook_preprocessor::{parse_input, Preprocessor};
use std::io;
use std::process;

fn make_app() -> Command {
    Command::new("mdbook-mermaid-compat")
        .about("mdBook 0.5.2-compatible wrapper for mdbook-mermaid")
        .subcommand(
            Command::new("supports")
                .arg(Arg::new("renderer").required(true))
                .about("Check whether a renderer is supported by this preprocessor"),
        )
}

fn main() {
    let matches = make_app().get_matches();
    let preprocessor = mdbook_mermaid::Mermaid;

    if let Some(sub_args) = matches.subcommand_matches("supports") {
        handle_supports(&preprocessor, sub_args);
    } else if let Err(error) = handle_preprocessing(&preprocessor) {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn handle_preprocessing(pre: &dyn Preprocessor) -> Result<(), Error> {
    let (ctx, book) = parse_input(io::stdin())?;
    let processed_book = pre.run(&ctx, book)?;
    serde_json::to_writer(io::stdout(), &processed_book)?;
    Ok(())
}

fn handle_supports(pre: &dyn Preprocessor, sub_args: &ArgMatches) -> ! {
    let renderer = sub_args
        .get_one::<String>("renderer")
        .expect("required argument");
    let supported = pre.supports_renderer(renderer).unwrap_or(false);

    if supported {
        process::exit(0);
    } else {
        process::exit(1);
    }
}
