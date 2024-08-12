use clap::Parser;

#[derive(Parser, Debug)]
struct Cli {
    /// Specify log level.
    #[clap(short, long = "log", default_value_t = tracing::Level::DEBUG)]
    log_level: tracing::Level,

    #[clap(subcommand)]
    command: Cmd,
}

#[derive(Debug, clap::Subcommand)]
enum Cmd {
    /// Find all git repos under the given directories.
    Find(og::cmd::find::Cmd),
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    og::tracing_init(Some(cli.log_level))?;
    tracing::debug!(?cli, "Starting");
    match cli.command {
        Cmd::Find(cmd) => {
            cmd.run()?;
        }
    }
    Ok(())
}
