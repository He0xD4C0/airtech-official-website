use std::{collections::BTreeSet, env, fs, path::PathBuf};

const LEGACY_SQLX_LAST_VERSION: i64 = 10;

fn main() {
    println!("cargo:rerun-if-changed=../../migrations");

    let migrations = PathBuf::from("../../migrations");
    let mut versions = BTreeSet::new();
    for entry in fs::read_dir(&migrations).expect("Flyway migration directory must exist") {
        let path = entry
            .expect("Flyway migration entry must be readable")
            .path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !name.ends_with(".sql") {
            continue;
        }
        let version = name
            .strip_prefix('V')
            .and_then(|name| name.split_once("__"))
            .and_then(|(version, _)| version.parse::<i64>().ok())
            .unwrap_or_else(|| panic!("invalid Flyway versioned migration name: {name}"));
        assert!(version > 0, "Flyway migration versions must be positive");
        assert!(
            versions.insert(version),
            "duplicate Flyway migration version: {version}"
        );
    }

    let latest = versions
        .last()
        .copied()
        .expect("at least one Flyway migration is required");
    assert!(
        latest >= LEGACY_SQLX_LAST_VERSION,
        "Flyway migration history V1-V10 must remain present"
    );
    for version in 1..=LEGACY_SQLX_LAST_VERSION {
        assert!(
            versions.contains(&version),
            "legacy SQLx migration V{version} must remain in the Flyway history"
        );
    }
    let expected = (1..=latest).collect::<BTreeSet<_>>();
    assert_eq!(
        versions, expected,
        "Flyway migration versions must be contiguous from V1"
    );

    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo OUT_DIR is set"))
        .join("flyway_version.rs");
    fs::write(
        output,
        format!("pub const REQUIRED_SCHEMA_VERSION: i64 = {latest};\n"),
    )
    .expect("generated Flyway version constant must be writable");
}
