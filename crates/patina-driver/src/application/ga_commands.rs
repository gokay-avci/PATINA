use anyhow::Result;

pub(crate) fn handle_ga_mode_command(command: crate::GaModeCommand) -> Result<()> {
    match command {
        crate::GaModeCommand::PersistentDaemon(args) => crate::run_rust_janus_search(*args),
        crate::GaModeCommand::ScottMonolithic(args) => crate::run_scott_staged_ga(*args),
        crate::GaModeCommand::Explain => {
            println!("{}", crate::render_run_ga_architecture_help());
            Ok(())
        }
    }
}
