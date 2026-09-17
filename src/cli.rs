//! The command line.

use std::path::PathBuf;

use clap::{
    Parser,
    Subcommand,
};

/// Build and serve an Antora documentation site.
#[derive(Debug, Parser)]
#[command(name = "antors", version, about, long_about = None)]
pub(crate) struct Cli {
    /// What to do. Without a subcommand, `build`.
    #[command(subcommand)]
    pub(crate) command: Option<Command>,

    /// The playbook to build, for the bare form.
    #[arg(value_name = "PLAYBOOK")]
    pub(crate) playbook: Option<PathBuf>,

    #[command(flatten)]
    pub(crate) build: BuildArgs,
}

/// One thing to do.
#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Build the site.
    Build {
        /// The playbook.
        #[arg(value_name = "PLAYBOOK", default_value = "antora-playbook.yml")]
        playbook: PathBuf,

        #[command(flatten)]
        args: BuildArgs,
    },

    /// Build the site, serve it, and rebuild as its sources change.
    #[cfg(feature = "serve")]
    Serve {
        /// The playbook.
        #[arg(value_name = "PLAYBOOK", default_value = "antora-playbook.yml")]
        playbook: PathBuf,

        #[command(flatten)]
        args: BuildArgs,

        #[command(flatten)]
        serve: ServeArgs,
    },
}

/// Where to listen, and whether to keep rebuilding.
#[cfg(feature = "serve")]
#[derive(Clone, Debug, clap::Args)]
pub(crate) struct ServeArgs {
    /// The address to listen on. Port 0 picks a free one.
    #[arg(long, default_value = "127.0.0.1:4000", value_name = "ADDR")]
    pub(crate) bind: std::net::SocketAddr,

    /// Build and serve once, without watching for changes.
    #[arg(long)]
    pub(crate) no_watch: bool,
}

/// What to leave out of a build, and where to put it.
#[derive(Clone, Debug, clap::Args)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "one field per command line flag is what reads clearly"
)]
pub(crate) struct BuildArgs {
    /// Write the site here instead of where the playbook says.
    #[arg(short = 'o', long, value_name = "DIR")]
    pub(crate) to_dir: Option<PathBuf>,

    /// Empty the output directory first.
    #[arg(long)]
    pub(crate) clean: bool,

    /// Leave source blocks unhighlighted.
    #[arg(long)]
    pub(crate) no_highlight: bool,

    /// Show mermaid blocks as the listings they were written as.
    #[arg(long)]
    pub(crate) no_mermaid: bool,

    /// Show equations as the notation they were written in.
    #[arg(long)]
    pub(crate) no_math: bool,

    /// Mark admonitions with their label instead of an icon.
    #[arg(long)]
    pub(crate) no_icons: bool,

    /// Do not generate the page that gathers every `:page-tags:` entry.
    #[arg(long)]
    pub(crate) no_tags_page: bool,

    /// Also write every page, and every component version, as a PDF.
    #[arg(long)]
    pub(crate) pdf: bool,

    /// Say nothing unless something went wrong.
    #[arg(short, long)]
    pub(crate) quiet: bool,

    /// Fail the build if anything was reported, not only errors.
    #[arg(long)]
    pub(crate) strict: bool,
}

impl BuildArgs {
    /// What these arguments ask the renderer for.
    pub(crate) fn render_options(&self) -> antors_site::build::RenderOptions {
        antors_site::build::RenderOptions {
            icons: !self.no_icons,
            highlight: !self.no_highlight,
            mermaid: !self.no_mermaid,
            math: !self.no_math,
            toc_levels: 2,
        }
    }
}

/// What the command line asked for, with the two forms of `build` reduced to
/// one.
#[derive(Debug)]
pub(crate) enum Action {
    /// Build the site and stop.
    Build(PathBuf, BuildArgs),

    /// Build the site and serve it.
    #[cfg(feature = "serve")]
    Serve(PathBuf, BuildArgs, ServeArgs),
}

impl Cli {
    /// What to do, however the command line was written.
    pub(crate) fn resolve(self) -> Action {
        match self.command {
            Some(Command::Build { playbook, args }) => Action::Build(playbook, args),

            #[cfg(feature = "serve")]
            Some(Command::Serve {
                playbook,
                args,
                serve,
            }) => Action::Serve(playbook, args, serve),

            None => Action::Build(
                self.playbook
                    .unwrap_or_else(|| PathBuf::from("antora-playbook.yml")),
                self.build,
            ),
        }
    }
}
