//! Manifest + product + overlays -> the concrete set of bricks to materialize.

use std::path::PathBuf;

use crate::manifest::{Brick, Manifest, Overlay};

/// One brick after resolution: where to clone from, what to check out, where to put it.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedBrick {
    pub name: String,
    pub url: String,
    pub rev: String,
    pub path: PathBuf,
    pub groups: Vec<String>,
}

/// True when `groups` passes the `filter`: an empty filter matches everything,
/// otherwise at least one group must match.
pub fn group_match(groups: &[String], filter: &[String]) -> bool {
    filter.is_empty() || groups.iter().any(|g| filter.contains(g))
}

/// Drop bricks whose groups don't match the filter.
pub fn filter_groups(resolution: &mut Resolution, filter: &[String]) {
    resolution
        .bricks
        .retain(|brick| group_match(&brick.groups, filter));
}

/// The bricks of one product with all overlays applied.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolution {
    pub product: String,
    pub bricks: Vec<ResolvedBrick>,
}

/// Errors produced while resolving a product.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("unknown stack `{0}`")]
    UnknownProduct(String),
    #[error("unknown overlay `{0}`")]
    UnknownOverlay(String),
    #[error("stack `{product}` references unknown repo `{brick}`")]
    UnknownBrick { product: String, brick: String },
    #[error("repo `{0}` has no usable source")]
    UnsourcedBrick(String),
}

fn active_overlays<'m>(
    manifest: &'m Manifest,
    overlays: &[String],
) -> Result<Vec<&'m Overlay>, ResolveError> {
    let mut active = Vec::with_capacity(overlays.len());
    for name in overlays {
        let overlay = manifest
            .overlays
            .get(name)
            .ok_or_else(|| ResolveError::UnknownOverlay(name.clone()))?;
        active.push(overlay);
    }
    Ok(active)
}

fn resolve_one(
    manifest: &Manifest,
    name: &str,
    brick: &Brick,
    active: &[&Overlay],
) -> Result<ResolvedBrick, ResolveError> {
    let mut rev = brick.rev.clone();
    let mut path = brick.checkout_path(name);
    for overlay in active {
        if let Some(over) = overlay.bricks.get(name) {
            if let Some(r) = &over.rev {
                rev = r.clone();
            }
            if let Some(p) = &over.path {
                path = p.clone();
            }
        }
    }
    let url = brick
        .clone_url(&manifest.remotes)
        .ok_or_else(|| ResolveError::UnsourcedBrick(name.to_string()))?;
    Ok(ResolvedBrick {
        name: name.to_string(),
        url,
        rev,
        path,
        groups: brick.groups.clone(),
    })
}

/// Resolve `product` against `manifest`, applying `overlays` in order
/// (later overlays win).
pub fn resolve(
    manifest: &Manifest,
    product: &str,
    overlays: &[String],
) -> Result<Resolution, ResolveError> {
    let spec = manifest
        .products
        .get(product)
        .ok_or_else(|| ResolveError::UnknownProduct(product.to_string()))?;
    let active = active_overlays(manifest, overlays)?;

    let mut bricks = Vec::with_capacity(spec.bricks.len());
    for name in &spec.bricks {
        let brick = manifest
            .bricks
            .get(name)
            .ok_or_else(|| ResolveError::UnknownBrick {
                product: product.to_string(),
                brick: name.clone(),
            })?;
        bricks.push(resolve_one(manifest, name, brick, &active)?);
    }

    Ok(Resolution {
        product: product.to_string(),
        bricks,
    })
}

/// Resolve every brick in the manifest (manifest order), applying `overlays`.
/// This is what lockfile generation uses: the lock covers all bricks.
pub fn resolve_all(
    manifest: &Manifest,
    overlays: &[String],
) -> Result<Vec<ResolvedBrick>, ResolveError> {
    let active = active_overlays(manifest, overlays)?;
    let mut bricks = Vec::with_capacity(manifest.bricks.len());
    for (name, brick) in &manifest.bricks {
        bricks.push(resolve_one(manifest, name, brick, &active)?);
    }
    Ok(bricks)
}
