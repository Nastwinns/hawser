use std::path::PathBuf;
use std::str::FromStr;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::ManifestError;

/// The parsed `keel.toml`: remotes, repos, stacks, overlays.
///
/// User-facing lexicon: `[repo.NAME]` and `[stack.NAME]`. The original
/// `brick`/`product` spellings still parse as aliases; serialization emits
/// the canonical `repo`/`stack`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(default, rename = "remote", skip_serializing_if = "IndexMap::is_empty")]
    pub remotes: IndexMap<String, Remote>,
    #[serde(
        default,
        rename = "repo",
        alias = "brick",
        skip_serializing_if = "IndexMap::is_empty"
    )]
    pub bricks: IndexMap<String, Brick>,
    #[serde(
        default,
        rename = "stack",
        alias = "product",
        skip_serializing_if = "IndexMap::is_empty"
    )]
    pub products: IndexMap<String, Product>,
    #[serde(
        default,
        rename = "overlay",
        skip_serializing_if = "IndexMap::is_empty"
    )]
    pub overlays: IndexMap<String, Overlay>,
}

/// A named base URL bricks can be cloned from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Remote {
    pub url: String,
}

/// One Git repository: a full autonomous clone at a declared path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Brick {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub rev: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<String>,
}

impl Brick {
    /// Checkout path in the workspace; defaults to the brick's name.
    pub fn checkout_path(&self, name: &str) -> PathBuf {
        self.path.clone().unwrap_or_else(|| PathBuf::from(name))
    }

    /// Full clone URL, either declared directly or joined from a named remote.
    pub fn clone_url(&self, remotes: &IndexMap<String, Remote>) -> Option<String> {
        if let Some(url) = &self.url {
            return Some(url.clone());
        }
        let remote = remotes.get(self.remote.as_deref()?)?;
        let repo = self.repo.as_deref()?;
        Some(format!("{}/{}", remote.url.trim_end_matches('/'), repo))
    }
}

/// A named composition (a "stack"): the set of repos it is built from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Product {
    #[serde(rename = "repos", alias = "bricks")]
    pub bricks: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Named per-brick overrides applied on top of the manifest at resolve time.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overlay {
    #[serde(
        default,
        rename = "repo",
        alias = "brick",
        skip_serializing_if = "IndexMap::is_empty"
    )]
    pub bricks: IndexMap<String, BrickOverride>,
}

/// The fields an overlay may override on one brick.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrickOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

impl Manifest {
    /// Check referential integrity: brick sources, remote names, product and
    /// overlay brick references.
    pub fn validate(&self) -> Result<(), ManifestError> {
        for (name, brick) in &self.bricks {
            match (&brick.url, &brick.remote, &brick.repo) {
                (Some(_), None, None) => {}
                (Some(_), _, _) => {
                    return Err(ManifestError::AmbiguousSource(name.clone()));
                }
                (None, Some(remote), Some(_)) => {
                    if !self.remotes.contains_key(remote) {
                        return Err(ManifestError::UnknownRemote {
                            brick: name.clone(),
                            remote: remote.clone(),
                        });
                    }
                }
                (None, _, _) => {
                    return Err(ManifestError::MissingSource(name.clone()));
                }
            }
        }
        for (name, product) in &self.products {
            for brick in &product.bricks {
                if !self.bricks.contains_key(brick) {
                    return Err(ManifestError::UnknownBrickInProduct {
                        product: name.clone(),
                        brick: brick.clone(),
                    });
                }
            }
        }
        for (name, overlay) in &self.overlays {
            for brick in overlay.bricks.keys() {
                if !self.bricks.contains_key(brick) {
                    return Err(ManifestError::UnknownBrickInOverlay {
                        overlay: name.clone(),
                        brick: brick.clone(),
                    });
                }
            }
        }
        Ok(())
    }
}

impl FromStr for Manifest {
    type Err = ManifestError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let manifest: Manifest =
            toml::from_str(text).map_err(|source| ManifestError::Parse(Box::new(source)))?;
        manifest.validate()?;
        Ok(manifest)
    }
}
