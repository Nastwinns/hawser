#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::PathBuf;

use keel_core::manifest::{Manifest, ManifestError};
use keel_core::resolver::{self, ResolveError};

const EXAMPLE: &str = r#"
[remote.internal]
url = "git@gitlab.company.com:firmware"

[remote.github]
url = "git@github.com:acme"

[brick.kernel]
remote = "internal"
repo = "kernel.git"
rev = "v6.1.2"
groups = ["firmware"]

[brick.hal]
remote = "internal"
repo = "hal.git"
rev = "main"
groups = ["firmware"]

[brick.app-mqtt]
remote = "github"
repo = "app-mqtt.git"
rev = "release/2.x"
path = "apps/mqtt"

[product.gateway]
bricks = ["kernel", "hal", "app-mqtt"]

[product.sensor-node]
bricks = ["kernel", "hal"]

[overlay.dev.brick.kernel]
rev = "main"
"#;

#[test]
fn parses_the_reference_example() {
    let manifest: Manifest = EXAMPLE.parse().unwrap();
    assert_eq!(manifest.remotes.len(), 2);
    assert_eq!(manifest.bricks.len(), 3);
    assert_eq!(manifest.products.len(), 2);
    assert_eq!(manifest.overlays.len(), 1);

    let kernel = &manifest.bricks["kernel"];
    assert_eq!(kernel.rev, "v6.1.2");
    assert_eq!(kernel.groups, ["firmware"]);
    assert_eq!(
        kernel.clone_url(&manifest.remotes).unwrap(),
        "git@gitlab.company.com:firmware/kernel.git"
    );
    assert_eq!(kernel.checkout_path("kernel"), PathBuf::from("kernel"));

    let mqtt = &manifest.bricks["app-mqtt"];
    assert_eq!(mqtt.checkout_path("app-mqtt"), PathBuf::from("apps/mqtt"));
}

#[test]
fn round_trips_through_toml() {
    let manifest: Manifest = EXAMPLE.parse().unwrap();
    let serialized = toml::to_string(&manifest).unwrap();
    let reparsed: Manifest = serialized.parse().unwrap();
    assert_eq!(manifest, reparsed);
}

#[test]
fn rejects_unknown_remote() {
    let err = r#"
[brick.a]
remote = "nope"
repo = "a.git"
rev = "main"
"#
    .parse::<Manifest>()
    .unwrap_err();
    assert!(matches!(err, ManifestError::UnknownRemote { .. }));
}

#[test]
fn rejects_brick_without_source() {
    let err = r#"
[brick.a]
rev = "main"
"#
    .parse::<Manifest>()
    .unwrap_err();
    assert!(matches!(err, ManifestError::MissingSource(name) if name == "a"));
}

#[test]
fn rejects_ambiguous_source() {
    let err = r#"
[remote.r]
url = "git@example.com:x"

[brick.a]
url = "git@example.com:x/a.git"
remote = "r"
repo = "a.git"
rev = "main"
"#
    .parse::<Manifest>()
    .unwrap_err();
    assert!(matches!(err, ManifestError::AmbiguousSource(name) if name == "a"));
}

#[test]
fn rejects_unknown_brick_in_product() {
    let err = r#"
[product.p]
bricks = ["ghost"]
"#
    .parse::<Manifest>()
    .unwrap_err();
    assert!(matches!(err, ManifestError::UnknownBrickInProduct { .. }));
}

#[test]
fn rejects_unknown_brick_in_overlay() {
    let err = r#"
[overlay.dev.brick.ghost]
rev = "main"
"#
    .parse::<Manifest>()
    .unwrap_err();
    assert!(matches!(err, ManifestError::UnknownBrickInOverlay { .. }));
}

#[test]
fn rejects_unknown_top_level_key() {
    assert!("[bricks.a]\nrev = \"main\"\n".parse::<Manifest>().is_err());
}

#[test]
fn resolves_a_product() {
    let manifest: Manifest = EXAMPLE.parse().unwrap();
    let resolution = resolver::resolve(&manifest, "gateway", &[]).unwrap();
    assert_eq!(resolution.product, "gateway");
    assert_eq!(resolution.bricks.len(), 3);

    let kernel = &resolution.bricks[0];
    assert_eq!(kernel.name, "kernel");
    assert_eq!(kernel.rev, "v6.1.2");
    assert_eq!(kernel.url, "git@gitlab.company.com:firmware/kernel.git");
    assert_eq!(kernel.path, PathBuf::from("kernel"));

    let mqtt = &resolution.bricks[2];
    assert_eq!(mqtt.path, PathBuf::from("apps/mqtt"));
    assert_eq!(mqtt.url, "git@github.com:acme/app-mqtt.git");
}

#[test]
fn overlay_overrides_rev() {
    let manifest: Manifest = EXAMPLE.parse().unwrap();
    let resolution = resolver::resolve(&manifest, "sensor-node", &["dev".into()]).unwrap();
    assert_eq!(resolution.bricks[0].rev, "main");
    assert_eq!(resolution.bricks[1].rev, "main");
}

#[test]
fn unknown_product_and_overlay_error() {
    let manifest: Manifest = EXAMPLE.parse().unwrap();
    assert!(matches!(
        resolver::resolve(&manifest, "ghost", &[]),
        Err(ResolveError::UnknownProduct(_))
    ));
    assert!(matches!(
        resolver::resolve(&manifest, "gateway", &["ghost".into()]),
        Err(ResolveError::UnknownOverlay(_))
    ));
}

#[test]
fn parses_repo_stack_lexicon_and_serializes_canonically() {
    let manifest: Manifest = r#"
[remote.r]
url = "git@example.com:org"

[repo.kernel]
remote = "r"
repo = "kernel.git"
rev = "main"

[stack.gateway]
repos = ["kernel"]

[overlay.dev.repo.kernel]
rev = "next"
"#
    .parse()
    .unwrap();
    assert_eq!(manifest.bricks.len(), 1);
    assert_eq!(manifest.products["gateway"].bricks, ["kernel"]);
    assert_eq!(
        manifest.overlays["dev"].bricks["kernel"].rev.as_deref(),
        Some("next")
    );

    let out = toml::to_string(&manifest).unwrap();
    assert!(out.contains("[repo.kernel]"), "canonical spelling is repo");
    assert!(
        out.contains("[stack.gateway]"),
        "canonical spelling is stack"
    );
    let reparsed: Manifest = out.parse().unwrap();
    assert_eq!(manifest, reparsed);
}
