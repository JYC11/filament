use clap::Args;
use clap_complete::{generate, Shell};

use crate::Cli;

/// Generate shell completions for filament.
#[derive(Args, Debug)]
pub struct CompletionsArgs {
    /// Target shell.
    pub shell: Shell,
}

pub fn run(args: &CompletionsArgs) {
    let mut cmd = <Cli as clap::CommandFactory>::command();
    generate(args.shell, &mut cmd, "filament", &mut std::io::stdout());
}
