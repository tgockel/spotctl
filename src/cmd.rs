//! Module for command-line parsing.

/// The basic command set.
#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub enum BaseCmd {
    /// Shuffle the user's entire library into a playlist.
    ShuffleLibrary,
}

#[derive(StructOpt, Debug)]
#[structopt(rename_all = "kebab-case")]
pub struct BaseOpts {
    #[structopt(subcommand)]
    pub command: BaseCmd,
}
