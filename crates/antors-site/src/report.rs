//! What a build has to say for itself.
//!
//! A problem is reported against the page it belongs to rather than printed as
//! it is found, so that a build can finish, say everything at once, and then
//! decide whether any of it was bad enough to fail on. A broken reference in
//! one page is not a reason to stop rendering the other two hundred.

use std::fmt;

/// How bad a problem is.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Severity {
    /// The site was built, and something in it is not what the author meant.
    Warning,

    /// The site was built with a hole in it.
    Error,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Warning => "warning",
            Self::Error => "error",
        })
    }
}

/// One thing that went wrong.
#[derive(Clone, Debug)]
pub struct Problem {
    /// How bad it is.
    pub severity: Severity,

    /// The file it is in, as the author would name it.
    pub file: String,

    /// The line, when it is known.
    pub line: Option<usize>,

    /// What went wrong.
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.severity, self.file)?;

        if let Some(line) = self.line {
            write!(f, ":{line}")?;
        }

        write!(f, ": {}", self.message)
    }
}

/// Everything a build had to say, and what it produced.
#[derive(Clone, Debug, Default)]
pub struct Report {
    /// Every problem, in the order the build found them.
    pub problems: Vec<Problem>,

    /// How many pages were written.
    pub pages: usize,

    /// How many other files — images, attachments, redirects, assets — were
    /// written.
    pub files: usize,
}

impl Report {
    /// Record a problem.
    pub fn push(&mut self, problem: Problem) {
        self.problems.push(problem);
    }

    /// Record a warning against a file.
    pub fn warn(
        &mut self,
        file: impl Into<String>,
        line: Option<usize>,
        message: impl Into<String>,
    ) {
        self.push(Problem {
            severity: Severity::Warning,
            file: file.into(),
            line,
            message: message.into(),
        });
    }

    /// Record an error against a file.
    pub fn error(&mut self, file: impl Into<String>, message: impl Into<String>) {
        self.push(Problem {
            severity: Severity::Error,
            file: file.into(),
            line: None,
            message: message.into(),
        });
    }

    /// How many problems of at least this severity there were.
    pub fn count(&self, severity: Severity) -> usize {
        self.problems
            .iter()
            .filter(|problem| problem.severity >= severity)
            .count()
    }

    /// Whether anything reached `failure_level`.
    pub fn should_fail(&self, failure_level: Severity) -> bool {
        self.count(failure_level) > 0
    }
}
