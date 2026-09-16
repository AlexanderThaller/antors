//! `antors`: build an Antora documentation site.

#![warn(missing_docs)]

mod cli;
#[cfg(feature = "serve")]
mod serve;

use std::process::ExitCode;

use antors_model::Playbook;
use antors_site::{
    Build,
    Options,
    Report,
    report::Severity,
};
use anyhow::{
    Context,
    Result,
};
use clap::Parser as _;

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,

        Err(error) => {
            eprintln!("antors: {error:#}");
            ExitCode::FAILURE
        }
    }
}

/// Do the work, and say whether the build should be called a success.
fn run() -> Result<ExitCode> {
    match cli::Cli::parse().resolve() {
        cli::Action::Build(path, args) => build(&path, &args),

        #[cfg(feature = "serve")]
        cli::Action::Serve(path, args, serve) => {
            let playbook = load(&path, &args)?;

            serve::run(
                playbook,
                Options {
                    render: args.render_options(),
                    clean: args.clean,
                },
                &serve::Config {
                    address: serve.bind,
                    watch: !serve.no_watch,
                },
            )?;

            Ok(ExitCode::SUCCESS)
        }
    }
}

/// Build the site once.
fn build(path: &std::path::Path, args: &cli::BuildArgs) -> Result<ExitCode> {
    let failure_level = if args.strict {
        Severity::Warning
    } else {
        Severity::Error
    };

    let report = Build::new(
        load(path, args)?,
        Options {
            render: args.render_options(),
            clean: args.clean,
        },
    )
    .run()?;

    print_report(&report, args.quiet);

    Ok(if report.should_fail(failure_level) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    })
}

/// Read the playbook, applying what the command line overrode.
fn load(path: &std::path::Path, args: &cli::BuildArgs) -> Result<Playbook> {
    let mut playbook =
        Playbook::load(path).with_context(|| format!("loading `{}`", path.display()))?;

    if let Some(to_dir) = &args.to_dir {
        playbook.output.dir.clone_from(to_dir);
    }

    Ok(playbook)
}

/// Say what the build found, and what it produced.
fn print_report(report: &Report, quiet: bool) {
    for problem in &report.problems {
        eprintln!("antors: {problem}");
    }

    if quiet {
        return;
    }

    let warnings = report.count(Severity::Warning);

    println!(
        "antors: {} pages, {} other files{}",
        report.pages,
        report.files,
        match warnings {
            0 => String::new(),
            1 => ", 1 problem".to_string(),
            many => format!(", {many} problems"),
        }
    );
}
