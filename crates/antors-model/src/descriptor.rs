//! `antora.yml`: what one directory of content says about itself.
//!
//! A component descriptor is the only thing that turns a directory of
//! `AsciiDoc` into a *component version*. It names the component, says which
//! version of it this directory is, and lists the navigation files — and
//! because it travels with the content rather than with the build, two branches
//! of one repository describe two versions of one component without the
//! playbook knowing either exists.

use std::collections::BTreeMap;

use serde::Deserialize;

/// The contents of one `antora.yml`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Descriptor {
    /// The component name, used in resource IDs and in the URL.
    pub name: String,

    /// What the UI shows in place of the name. Defaults to the name.
    #[serde(default)]
    pub title: Option<String>,

    /// The version this directory contributes.
    ///
    /// Absent is not the same as `~`. `~` says the component is unversioned —
    /// its pages sit at `/{name}/…` with no version segment and no version
    /// selector. Saying nothing at all leaves the question to the content
    /// source's own `version:` key, which is how one `antora.yml` on several
    /// branches describes several versions without being edited on each.
    #[serde(default, deserialize_with = "version_field")]
    pub version: VersionField,

    /// What the version selector shows instead of the bare version.
    #[serde(default)]
    pub display_version: Option<String>,

    /// Whether this version is a prerelease, and so is never *latest* even
    /// when it is the greatest.
    ///
    /// The value may be a flag or a label (`-beta`), which the UI appends to
    /// the displayed version.
    #[serde(default)]
    pub prerelease: Option<Prerelease>,

    /// The page the component's own name resolves to, as a resource ID.
    #[serde(default)]
    pub start_page: Option<String>,

    /// The navigation files, in the order their lists should appear.
    #[serde(default)]
    pub nav: Vec<String>,

    /// `AsciiDoc` settings for every page in this component version.
    #[serde(default)]
    pub asciidoc: Asciidoc,

    /// The version worked out from [`version`](Self::version) and the content
    /// source, once [`resolve_version`](Self::resolve_version) has run.
    #[serde(skip)]
    resolved_version: String,
}

/// Neither the descriptor nor the content source said what version this is.
#[derive(Clone, Debug, thiserror::Error)]
#[error(
    "`{component}` does not say what version it is: give `antora.yml` a `version:` key, or the \
     content source a `version:` key for the refs it reads"
)]
pub struct MissingVersion {
    /// The component that said nothing.
    pub component: String,
}

/// What a descriptor's `version:` key says, including saying nothing.
#[derive(Clone, Debug, Default)]
pub enum VersionField {
    /// The key is not there, and the content source decides.
    #[default]
    Absent,

    /// The key is there and says what the version is.
    Present(crate::playbook::VersionSpec),
}

/// Read a descriptor's `version:`, telling `~` apart from absent.
fn version_field<'de, D: serde::Deserializer<'de>>(de: D) -> Result<VersionField, D::Error> {
    use serde::Deserialize as _;

    let value = serde_yaml_ng::Value::deserialize(de)?;

    Ok(match value {
        // `version: ~` is a component that says it has no versions.
        serde_yaml_ng::Value::Null => {
            VersionField::Present(crate::playbook::VersionSpec::Flag(false))
        }

        other => VersionField::Present(
            crate::playbook::VersionSpec::deserialize(other).map_err(serde::de::Error::custom)?,
        ),
    })
}

impl Descriptor {
    /// The version as the rest of the model spells it: the empty string for an
    /// unversioned component.
    pub fn version(&self) -> String {
        self.resolved_version.clone()
    }

    /// Work out this component version's version, from the descriptor if it
    /// says and from the content source if it does not.
    ///
    /// `refname` is the branch or tag the descriptor was read from, which is
    /// what `version: true` means.
    pub fn resolve_version(
        &mut self,
        source: Option<&crate::playbook::VersionSpec>,
        refname: &str,
    ) -> Result<(), MissingVersion> {
        let spec = match &self.version {
            VersionField::Present(spec) => spec,

            // The descriptor said nothing, so the playbook must. Neither
            // saying anything is an error rather than a default: a component
            // published at a URL with no version in it, because nobody
            // mentioned one, is not something to guess at.
            VersionField::Absent => source.ok_or(MissingVersion {
                component: self.name.clone(),
            })?,
        };

        self.resolved_version = spec.version_of(refname).unwrap_or_default();

        Ok(())
    }

    /// What the UI should show for this version.
    ///
    /// A `display_version` wins outright. Failing that a prerelease *label* is
    /// appended to the version, which is how `2.0` plus `prerelease: -beta`
    /// comes out as `2.0-beta` without the version itself changing — the URL
    /// stays `2.0`, and only the reader is told.
    pub fn display_version(&self) -> String {
        if let Some(display) = &self.display_version {
            return display.clone();
        }

        let version = self.version();

        match &self.prerelease {
            Some(Prerelease::Label(label)) => format!("{version}{label}"),
            _ => version,
        }
    }

    /// Whether this version is kept out of the running for *latest*.
    pub fn is_prerelease(&self) -> bool {
        match &self.prerelease {
            None => false,
            Some(Prerelease::Flag(flag)) => *flag,
            Some(Prerelease::Label(label)) => !label.is_empty(),
        }
    }

    /// The title the UI shows, falling back to the name.
    pub fn title(&self) -> String {
        self.title.clone().unwrap_or_else(|| self.name.clone())
    }
}

/// `prerelease:` written either as a flag or as the label to append.
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum Prerelease {
    /// `prerelease: true`.
    Flag(bool),

    /// `prerelease: -beta` — also the label shown after the version.
    Label(String),
}

/// The `asciidoc:` key of a descriptor or a playbook.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asciidoc {
    /// Attributes set for every page this applies to.
    #[serde(default)]
    pub attributes: BTreeMap<String, AttributeValue>,

    /// Asciidoctor extensions. Read so a real playbook parses; they are Node
    /// modules and are not loaded.
    #[serde(default)]
    pub extensions: Vec<serde_yaml_ng::Value>,

    /// Whether the parse keeps a source map.
    #[serde(default)]
    pub sourcemap: bool,
}

/// One entry of an `attributes:` map.
///
/// The YAML type carries the meaning, which is Antora's convention rather than
/// something a reader can guess: `~` and `false` *unset* an attribute rather
/// than setting it to nothing, and a string ending in `@` is a default the page
/// may overrule instead of a value it may not.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum AttributeValue {
    /// `name: true` sets the attribute; `name: false` unsets it and keeps the
    /// page from setting it.
    Bool(bool),

    /// A number, written into the document as its decimal form.
    Number(f64),

    /// A string. A trailing `@` makes it soft — see [`Self::is_soft`].
    Text(String),

    /// `name: ~` — unset, and locked.
    Null,
}

impl AttributeValue {
    /// Whether the page may override this value.
    ///
    /// The marker is a trailing `@` on the string, which is Asciidoctor's own
    /// notation for a soft-set attribute and is why it is spelled this way
    /// rather than with a key of its own.
    pub fn is_soft(&self) -> bool {
        matches!(self, Self::Text(text) if text.ends_with('@'))
    }

    /// The value as the parser should see it, or `None` when this entry unsets
    /// the attribute instead of setting it.
    pub fn text(&self) -> Option<String> {
        match self {
            Self::Bool(true) => Some(String::new()),
            Self::Bool(false) | Self::Null => None,
            Self::Number(number) => Some(format_number(*number)),
            Self::Text(text) => Some(text.strip_suffix('@').unwrap_or(text).to_string()),
        }
    }
}

/// Write a YAML number the way it was most likely written, so a `toclevels: 3`
/// does not reach the document as `3.0`.
pub(crate) fn format_number(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "guarded above: the value has no fractional part and fits"
        )]
        return (number as i64).to_string();
    }

    number.to_string()
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used, reason = "a failed parse is the test failing")]

    use super::*;

    /// Parse a descriptor and settle its version, the way a build does.
    ///
    /// `refname` is the branch it was read from, which is what `version: true`
    /// on either side means.
    fn parse_on(yaml: &str, refname: &str) -> Descriptor {
        let mut descriptor: Descriptor = serde_yaml_ng::from_str(yaml).unwrap();

        descriptor
            .resolve_version(Some(&crate::playbook::VersionSpec::Flag(true)), refname)
            .unwrap();

        descriptor
    }

    fn parse(yaml: &str) -> Descriptor {
        parse_on(yaml, "main")
    }

    #[test]
    fn a_null_version_is_unversioned() {
        let descriptor = parse("name: sidecar\nversion: ~\n");

        assert_eq!(descriptor.version(), "");
        assert_eq!(descriptor.display_version(), "");
    }

    #[test]
    fn a_descriptor_that_says_nothing_takes_the_content_source_s_answer() {
        // The usual shape for a component versioned by branch: one `antora.yml`
        // with no `version:`, on several branches, and `version: true` in the
        // playbook.
        assert_eq!(parse_on("name: a\n", "develop").version(), "develop");

        // A ref name that cannot be a URL segment is flattened rather than
        // rejected.
        assert_eq!(
            parse_on("name: a\n", "release/2.0").version(),
            "release-2.0"
        );
    }

    #[test]
    fn a_descriptor_that_does_say_wins() {
        assert_eq!(
            parse_on("name: a\nversion: '2.0'\n", "develop").version(),
            "2.0"
        );
    }

    #[test]
    fn a_component_nobody_versioned_is_an_error() {
        let mut descriptor: Descriptor = serde_yaml_ng::from_str("name: a\n").unwrap();

        // Neither the descriptor nor the content source said, and a version
        // segment silently missing from every URL is not a thing to guess at.
        assert!(descriptor.resolve_version(None, "main").is_err());
    }

    #[test]
    fn a_numeric_version_is_read_as_a_string() {
        // `version: 2.0` is a YAML float, and `2` a YAML integer. Neither may
        // reach a URL as `2.0` or `2` by accident of formatting.
        assert_eq!(parse("name: a\nversion: 2.0\n").version(), "2.0");
        assert_eq!(parse("name: a\nversion: 2\n").version(), "2");
        assert_eq!(parse("name: a\nversion: '2.0'\n").version(), "2.0");
    }

    #[test]
    fn display_version_wins_over_everything() {
        let descriptor =
            parse("name: a\nversion: '2.0'\ndisplay_version: '2.0 (current)'\nprerelease: -beta\n");

        assert_eq!(descriptor.display_version(), "2.0 (current)");
    }

    #[test]
    fn a_prerelease_label_is_appended_to_the_version() {
        let descriptor = parse("name: a\nversion: '2.0'\nprerelease: -beta\n");

        assert_eq!(descriptor.version(), "2.0");
        assert_eq!(descriptor.display_version(), "2.0-beta");
        assert!(descriptor.is_prerelease());
    }

    #[test]
    fn a_prerelease_flag_leaves_the_display_alone() {
        let descriptor = parse("name: a\nversion: '2.0'\nprerelease: true\n");

        assert_eq!(descriptor.display_version(), "2.0");
        assert!(descriptor.is_prerelease());
    }

    #[test]
    fn attribute_values_carry_their_meaning_in_their_type() {
        let descriptor =
            parse(
                "name: a\nasciidoc:\n  attributes:\n    hard: value\n    soft: value@\n    on: \
                 true\n    off: false\n    gone: ~\n    count: 3\n",
            );

        let attributes = &descriptor.asciidoc.attributes;

        assert_eq!(attributes["hard"].text().as_deref(), Some("value"));
        assert!(!attributes["hard"].is_soft());

        assert_eq!(attributes["soft"].text().as_deref(), Some("value"));
        assert!(attributes["soft"].is_soft());

        assert_eq!(attributes["on"].text().as_deref(), Some(""));
        assert_eq!(attributes["off"].text(), None);
        assert_eq!(attributes["gone"].text(), None);
        assert_eq!(attributes["count"].text().as_deref(), Some("3"));
    }
}
