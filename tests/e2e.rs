use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

fn base_cmd() -> (Command, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let mut cmd = Command::cargo_bin("terraforge").unwrap();
    cmd.arg("--out-dir").arg(temp_dir.path());
    (cmd, temp_dir)
}

#[test]
fn test_basic_generation() {
    let (mut cmd, temp_dir) = base_cmd();
    cmd.arg("--width").arg("64")
       .arg("--height").arg("64")
       .arg("--seed").arg("42");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Complete!"));

    assert!(temp_dir.path().join("heightmap_42.png").exists(), "Heightmap PNG was not created");
    assert!(temp_dir.path().join("params_42.json").exists(), "Params JSON was not created");
    assert!(temp_dir.path().join("README.md").exists(), "README was not created");
    assert!(fs::metadata(temp_dir.path().join("heightmap_42.png")).unwrap().len() > 0,
        "Heightmap PNG is empty");
}

#[test]
fn test_preset_generation() {
    let (mut cmd, temp_dir) = base_cmd();
    cmd.arg("--preset").arg("fantasy-island")
       .arg("--seed").arg("99")
       .arg("--width").arg("64")
       .arg("--height").arg("64");

    cmd.assert().success();
    assert!(temp_dir.path().join("heightmap_99.png").exists());
}

#[test]
fn test_demo_mode_caps_resolution() {
    let (mut cmd, temp_dir) = base_cmd();
    cmd.arg("--width").arg("2048")
       .arg("--height").arg("2048")
       .arg("--demo")
       .arg("--seed").arg("1");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("DEMO"));

    let readme = fs::read_to_string(temp_dir.path().join("README.md")).unwrap();
    assert!(readme.contains("DEMO"), "README should warn about demo limits");
}

#[test]
fn test_fantasy_map_generation() {
    let (mut cmd, temp_dir) = base_cmd();
    cmd.arg("--width").arg("128")
       .arg("--height").arg("128")
       .arg("--seed").arg("55")
       .arg("--fantasy-map");

    cmd.assert().success();
    assert!(temp_dir.path().join("fantasy_map_55.png").exists(),
        "Fantasy map was not generated");
}

#[test]
fn test_all_exports() {
    let (mut cmd, temp_dir) = base_cmd();
    cmd.arg("--width").arg("64")
       .arg("--height").arg("64")
       .arg("--seed").arg("777")
       .arg("--normal-map")
       .arg("--splatmap")
       .arg("--albedo")
       .arg("--thumbnail");

    cmd.assert().success();
    assert!(temp_dir.path().join("normal_777.png").exists());
    assert!(temp_dir.path().join("splatmap_777.png").exists());
    assert!(temp_dir.path().join("albedo_777.png").exists());
    assert!(temp_dir.path().join("thumbnail_777.png").exists());
}

#[test]
fn test_seed_explorer() {
    let (mut cmd, temp_dir) = base_cmd();
    cmd.arg("--explore").arg("2")
       .arg("--seed").arg("100")
       .arg("--width").arg("64")
       .arg("--height").arg("64");

    cmd.assert().success();
    let sheet = temp_dir.path().join("seed_explorer_grid.png");
    assert!(sheet.exists(), "Contact sheet was not generated");
    assert!(fs::metadata(&sheet).unwrap().len() > 1000,
        "Contact sheet is suspiciously small");
}

#[test]
fn test_invalid_preset_fails() {
    let mut cmd = Command::cargo_bin("terraforge").unwrap();
    cmd.arg("--preset").arg("invalid_preset_name");
    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("invalid value"));
}

#[test]
fn test_deterministic_output() {
    let (mut cmd1, temp1) = base_cmd();
    cmd1.arg("--width").arg("64").arg("--height").arg("64").arg("--seed").arg("42");
    cmd1.assert().success();

    let (mut cmd2, temp2) = base_cmd();
    cmd2.arg("--width").arg("64").arg("--height").arg("64").arg("--seed").arg("42");
    cmd2.assert().success();

    let file1 = fs::read(temp1.path().join("heightmap_42.png")).unwrap();
    let file2 = fs::read(temp2.path().join("heightmap_42.png")).unwrap();
    assert_eq!(file1, file2, "Same seed produced different outputs!");
}
