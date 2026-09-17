//! A local server that rebuilds the site as its sources change.
//!
//! This is what replaces `antora --watch` beside a static file server: one
//! command that builds the site, serves it, watches every content source, and
//! tells an open page to reload when a rebuild finishes.
//!
//! # How a page learns that it changed
//!
//! The server keeps a counter that goes up after every rebuild. Each HTML
//! response is served with a small script holding the counter's value at the
//! time it was served; that script asks the server for the current value, and
//! the server does not answer until the two differ or the request has waited
//! long enough to be worth renewing. So a change reaches the browser as soon as
//! the rebuild finishes, with no polling in between.
//!
//! The script is added as the page is *served*, not as it is built, so the site
//! on disk is the same whether it was built to be served or to be published.

use std::{
    net::SocketAddr,
    path::{
        Path,
        PathBuf,
    },
    sync::Arc,
    time::Duration,
};

use antors_model::{
    Playbook,
    playbook::is_local,
};
use antors_site::{
    Build,
    Options,
};
use anyhow::{
    Context as _,
    Result,
};
use axum::{
    Router,
    body::Body,
    extract::{
        Query,
        State,
    },
    http::{
        StatusCode,
        Uri,
        header,
    },
    response::{
        IntoResponse,
        Response,
    },
    routing::get,
};
use notify_debouncer_full::{
    DebounceEventResult,
    new_debouncer,
    notify::{
        RecursiveMode,
        event::EventKind,
    },
};
use tokio::sync::watch;

/// The path the reload script asks about changes on.
pub(crate) const ENDPOINT: &str = "/__antors/reload";

/// How long to let file-system events settle before rebuilding.
///
/// An editor writes a file in several steps — a temporary file, a rename, a
/// permission change — and rebuilding a whole site after each one would be
/// wasted work and confusing output.
pub(crate) const DEBOUNCE: Duration = Duration::from_millis(250);

/// How long a waiting reload request is held before it is answered unchanged.
///
/// Something has to bound the wait: proxies and browsers drop a connection that
/// produces nothing for long enough, and an answer the client recognizes as "no
/// change" costs one round trip and renews the wait cleanly.
pub(crate) const MAX_WAIT: Duration = Duration::from_secs(20);

/// What the server was told to do.
#[derive(Clone, Debug)]
pub(crate) struct Config {
    /// Where to listen.
    pub(crate) address: SocketAddr,

    /// Whether to rebuild when a source changes.
    pub(crate) watch: bool,
}

/// Build the site, then serve it until the process is interrupted.
pub(crate) fn run(playbook: Playbook, options: Options, config: &Config) -> Result<()> {
    let root = playbook.output.dir.clone();
    let build = Arc::new(Build::new(playbook.clone(), options));

    report(&build.run()?);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("starting the server runtime")?;

    runtime.block_on(serve(playbook, build, root, config))
}

/// Serve `root`, rebuilding into it as the sources change.
async fn serve(
    playbook: Playbook,
    build: Arc<Build>,
    root: PathBuf,
    config: &Config,
) -> Result<()> {
    let (sender, _) = watch::channel(0_u64);

    // Held for the life of the server: dropping it stops the watch.
    let _watcher = if config.watch {
        Some(start_watching(&playbook, build, sender.clone())?)
    } else {
        None
    };

    let state = Server {
        root: root.clone(),
        reload: sender,
        watching: config.watch,
    };

    let app = Router::new()
        .route(ENDPOINT, get(reload))
        .fallback(get(file))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(config.address)
        .await
        .with_context(|| format!("listening on {}", config.address))?;

    let address = listener.local_addr().unwrap_or(config.address);

    println!("antors: serving {} on http://{address}", root.display());
    println!("antors: press Ctrl-C to stop");

    axum::serve(listener, app)
        .with_graceful_shutdown(interrupted())
        .await
        .context("serving")
}

/// Resolves once the process is asked to stop.
///
/// The first interrupt starts a graceful shutdown: the listener closes and the
/// requests in flight are allowed to finish. One of those is almost always a
/// reload request waiting for the sources to change, which holds on for up to
/// [`MAX_WAIT`] — twenty seconds of nothing, for someone who only wanted their
/// prompt back. So a second press does not wait for it.
async fn interrupted() {
    // Failing to install the handler leaves this pending for ever, which means
    // the server runs until it is killed: the same as having no handler.
    if tokio::signal::ctrl_c().await.is_err() {
        std::future::pending::<()>().await;
    }

    // The terminal has just echoed `^C`, so open a line before writing on it.
    println!();
    println!("antors: stopping; press Ctrl-C again to quit at once");

    // The handler stays installed, so awaiting it again waits for the next
    // press. Exiting from here rather than letting the shutdown finish is the
    // whole point: nothing still running is worth waiting for.
    tokio::spawn(async {
        if tokio::signal::ctrl_c().await.is_ok() {
            println!("antors: quitting");
            std::process::exit(130);
        }
    });
}

/// What every request handler shares.
#[derive(Clone, Debug)]
struct Server {
    /// The directory being served.
    root: PathBuf,

    /// The rebuild counter.
    reload: watch::Sender<u64>,

    /// Whether the reload script is worth serving at all.
    watching: bool,
}

/// Watch every content source, rebuilding when one changes.
fn start_watching(
    playbook: &Playbook,
    build: Arc<Build>,
    reload: watch::Sender<u64>,
) -> Result<impl std::fmt::Debug> {
    let output = resolved(&playbook.output.dir);

    let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
        let Ok(events) = result else {
            return;
        };

        // A rebuild writes into the output directory, and watching that would
        // make every rebuild trigger the next one. Both sides of the
        // comparison go through [`resolved`], so an output directory inside a
        // watched one is recognized however the playbook happened to spell it.
        let touched = events.iter().any(|event| {
            matches!(
                event.kind,
                EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
            ) && event
                .paths
                .iter()
                .any(|path| !resolved(path).starts_with(&output))
        });

        if !touched {
            return;
        }

        match build.run() {
            Ok(report) => {
                report_changes(&report);
                reload.send_modify(|counter| *counter += 1);
            }

            Err(error) => eprintln!("antors: {error:#}"),
        }
    })
    .context("starting the file watcher")?;

    for root in watch_roots(playbook) {
        debouncer
            .watch(&root, RecursiveMode::Recursive)
            .with_context(|| format!("watching `{}`", root.display()))?;
    }

    Ok(debouncer)
}

/// Every directory a rebuild would read.
///
/// Only the start paths are watched, not the whole repository: a documentation
/// source inside a code repository would otherwise rebuild the site every time
/// a build wrote an object file.
///
/// The roots are [`resolved`] rather than watched as the playbook spells them,
/// because the watcher builds the path in every event out of the path it was
/// handed. Watching `../../docs` reports a change as `<cwd>/../../docs/...`,
/// and `Path::starts_with` compares component by component, so a `..` left in
/// the middle keeps that path from ever matching the output directory —
/// whereupon a rebuild's own writes look like a change, and the rebuilding
/// never stops.
fn watch_roots(playbook: &Playbook) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    for source in &playbook.content.sources {
        if !is_local(&source.url) {
            continue;
        }

        for start_path in source.start_paths() {
            let root = Path::new(&source.url).join(start_path);

            if root.is_dir() {
                roots.push(resolved(&root));
            }
        }
    }

    roots
}

/// A path in the form the watcher's own paths take: absolute, with every
/// symbolic link and `.` and `..` gone.
///
/// Watching and filtering only agree if both sides are spelled the same way. A
/// playbook says `url: ../..` and `dir: ./build/site`, and the watcher follows
/// symbolic links when it walks a directory, so a change under the output
/// directory can arrive named in any number of ways — all of them different
/// from the output directory as written. Resolving both ends settles it.
///
/// A path that does not exist — a file an event reports as removed, or an
/// output directory no build has written yet — is resolved as far as it does
/// exist, so the answer is still the one it will have once it is there.
fn resolved(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };

    if let Ok(canonical) = absolute.canonicalize() {
        return canonical;
    }

    // Nothing left to climb: hand back what there is rather than looping.
    let (Some(parent), Some(name)) = (absolute.parent(), absolute.file_name()) else {
        return absolute;
    };

    resolved(parent).join(name)
}

/// Answer a reload request once the counter has moved past `since`.
async fn reload(State(server): State<Server>, Query(query): Query<ReloadQuery>) -> Response {
    let mut receiver = server.reload.subscribe();
    let since = query.since;

    let wait = async {
        while *receiver.borrow_and_update() <= since {
            if receiver.changed().await.is_err() {
                break;
            }
        }
    };

    // Timing out is not a failure: the answer is the current counter either
    // way, and the client renews the wait.
    let _ = tokio::time::timeout(MAX_WAIT, wait).await;

    let counter = *server.reload.borrow();

    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        counter.to_string(),
    )
        .into_response()
}

/// What the reload script asks.
#[derive(Debug, serde::Deserialize)]
struct ReloadQuery {
    /// The counter the page was served with.
    #[serde(default)]
    since: u64,
}

/// Serve one file out of the output directory.
async fn file(State(server): State<Server>, uri: Uri) -> Response {
    let Some(path) = resolve(&server.root, uri.path()) else {
        return not_found(&server).await;
    };

    let Ok(bytes) = tokio::fs::read(&path).await else {
        return not_found(&server).await;
    };

    let content_type = mime_of(&path);

    if content_type.starts_with("text/html") {
        let html = String::from_utf8_lossy(&bytes).into_owned();
        let html = inject(&html, &server);

        return (
            [
                (header::CONTENT_TYPE, content_type),
                (header::CACHE_CONTROL, "no-store"),
            ],
            html,
        )
            .into_response();
    }

    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CACHE_CONTROL, "no-store"),
        ],
        Body::from(bytes),
    )
        .into_response()
}

/// The site's own 404 page, or a plain one if it has not got one.
async fn not_found(server: &Server) -> Response {
    let path = server.root.join("404.html");

    match tokio::fs::read_to_string(&path).await {
        Ok(html) => (
            StatusCode::NOT_FOUND,
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            inject(&html, server),
        )
            .into_response(),

        Err(_) => (StatusCode::NOT_FOUND, "Not found\n").into_response(),
    }
}

/// The file a request path names, if it is inside the served directory.
///
/// A path with a `..` in it is refused rather than normalized: the request is
/// trying to leave the directory, and the only honest answer to that is no.
fn resolve(root: &Path, request: &str) -> Option<PathBuf> {
    let request = request.split(['?', '#']).next().unwrap_or(request);
    let decoded = percent_decode(request);

    let mut path = root.to_path_buf();

    for segment in decoded.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }

        if segment == ".." || segment.contains('\\') {
            return None;
        }

        path.push(segment);
    }

    if path.is_dir() {
        path.push("index.html");
    }

    path.is_file().then_some(path)
}

/// Decode the `%xx` escapes in a request path.
fn percent_decode(path: &str) -> String {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();

            if let Some(byte) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(byte);
                index += 3;
                continue;
            }
        }

        out.push(bytes[index]);
        index += 1;
    }

    String::from_utf8_lossy(&out).into_owned()
}

/// The content type for a path's extension.
fn mime_of(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("ico") => "image/x-icon",
        Some("pdf") => "application/pdf",
        Some("txt") => "text/plain; charset=utf-8",
        Some("xml") => "application/xml",
        Some("woff2") => "font/woff2",
        Some("woff") => "font/woff",
        _ => "application/octet-stream",
    }
}

/// Add the reload script to a page.
fn inject(html: &str, server: &Server) -> String {
    if !server.watching {
        return html.to_string();
    }

    let counter = *server.reload.borrow();
    let script = reload_script(counter);

    match html.rfind("</body>") {
        Some(index) => format!("{}{script}{}", &html[..index], &html[index..]),
        None => format!("{html}{script}"),
    }
}

/// The script that waits for the counter to move and then reloads.
fn reload_script(counter: u64) -> String {
    format!(
        r"<script>
(function () {{
  var since = {counter}
  function wait () {{
    fetch('{ENDPOINT}?since=' + since).then(function (response) {{
      return response.text()
    }}).then(function (body) {{
      if (Number(body) > since) return location.reload()
      wait()
    }}).catch(function () {{
      // The server has gone away — or is restarting. Try again shortly rather
      // than giving up, so a rebuild that takes the server down with it still
      // brings the page back.
      setTimeout(wait, 1000)
    }})
  }}
  wait()
}})()
</script>
"
    )
}

/// Say what a build produced.
fn report(report: &antors_site::Report) {
    for problem in &report.problems {
        eprintln!("antors: {problem}");
    }

    println!(
        "antors: {} pages, {} other files",
        report.pages, report.files
    );
}

/// Say what a rebuild produced, with the time it happened.
fn report_changes(report: &antors_site::Report) {
    for problem in &report.problems {
        eprintln!("antors: {problem}");
    }

    println!(
        "antors: rebuilt — {} pages, {} other files",
        report.pages, report.files
    );
}

#[cfg(test)]
mod tests {
    use std::path::{
        Path,
        PathBuf,
    };

    use super::resolved;

    /// The working directory, in the form [`resolved`] hands back.
    fn here() -> PathBuf {
        std::env::current_dir()
            .expect("a working directory")
            .canonicalize()
            .expect("a working directory that exists")
    }

    #[test]
    fn a_relative_path_resolves_against_the_working_directory() {
        assert_eq!(resolved(Path::new("./src")), here().join("src"));
        assert_eq!(resolved(Path::new("src/../src")), here().join("src"));
    }

    #[test]
    fn a_path_that_does_not_exist_still_resolves() {
        // The output directory before the first build has written it, and the
        // path an event reports for a file that has just been removed.
        assert_eq!(
            resolved(Path::new("./build/site/index.html")),
            here().join("build/site/index.html")
        );
    }

    /// The bug this guards against: an output directory below a watched root
    /// went unrecognized, because one side of the comparison was canonical and
    /// the other was the path as the playbook spelled it, so every rebuild
    /// triggered the next one.
    #[test]
    fn output_below_a_watched_root_is_recognized_however_it_is_spelled() {
        let output = resolved(Path::new("./build/site"));
        let written = resolved(&Path::new("./src/..").join("build/site/index.html"));

        assert!(written.starts_with(&output));
    }
}
