use clap::{Arg, ArgMatches, Command};
use mdbook_preprocessor::errors::Error;
use mdbook_preprocessor::{parse_input, Preprocessor};
use std::io;
use std::process;

fn make_app() -> Command {
    Command::new("mdbook-bib-compat")
        .about("mdBook 0.5.2-compatible wrapper for mdbook-bib")
        .subcommand(
            Command::new("supports")
                .arg(Arg::new("renderer").required(true))
                .about("Check whether a renderer is supported by this preprocessor"),
        )
}

fn main() {
    logging_initialization();
    let matches = make_app().get_matches();
    let preprocessor = mdbook_bib::Bibliography;

    if let Some(sub_args) = matches.subcommand_matches("supports") {
        handle_supports(&preprocessor, sub_args);
    } else if let Err(error) = handle_preprocessing(&preprocessor) {
        eprintln!("Errors: {error}");
        process::exit(1);
    }
}

fn logging_initialization() {
    let filter = tracing_subscriber::EnvFilter::builder()
        .with_env_var("MDBOOK_LOG")
        .with_default_directive(tracing_subscriber::filter::LevelFilter::INFO.into())
        .from_env_lossy();
    let log_env = std::env::var("MDBOOK_LOG");
    let silence_unless_specified = |filter: tracing_subscriber::EnvFilter, target| {
        if !log_env
            .as_ref()
            .is_ok_and(|value| value.split(',').any(|directive| directive.starts_with(target)))
        {
            filter.add_directive(format!("{target}=warn").parse().unwrap())
        } else {
            filter
        }
    };
    let filter = silence_unless_specified(filter, "handlebars");
    let filter = silence_unless_specified(filter, "html5ever");

    let with_target = log_env.is_ok();

    tracing_subscriber::fmt()
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stderr()))
        .with_writer(std::io::stderr)
        .with_env_filter(filter)
        .with_target(with_target)
        .init();
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
