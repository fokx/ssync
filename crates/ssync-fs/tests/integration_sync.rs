use ssync_fs::filter::FilterSet;
use ssync_fs::metadata::{FileMetadata, FileType};
use ssync_fs::path::SourcePath;
use ssync_fs::sync::{SyncEngine, SyncOptions};
use ssync_fs::walker::{WalkOptions, Walker};
use std::fs;
use tempfile::tempdir;

#[test]
fn test_recursive_tree_sync() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");

    fs::create_dir_all(src.join("subdir")).unwrap();
    fs::write(src.join("file1"), b"content 1").unwrap();
    fs::write(src.join("subdir/file2"), b"content 2").unwrap();

    let walk_opts = WalkOptions {
        recursive: true,
        links: true,
        ..Default::default()
    };
    let walker = Walker::new(walk_opts);
    let sources = vec![SourcePath::from_raw(&format!("{}/", src.display()))];
    let entries = walker.scan_sources(&sources).unwrap();

    let sync_opts = SyncOptions {
        preserve_perms: true,
        preserve_times: true,
        ..Default::default()
    };
    let engine = SyncEngine::new(sync_opts);
    engine.execute(&entries, &dest).unwrap();

    assert_eq!(fs::read(dest.join("file1")).unwrap(), b"content 1");
    assert_eq!(fs::read(dest.join("subdir/file2")).unwrap(), b"content 2");
}

#[test]
fn test_symlink_and_hardlink_preservation() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");

    fs::create_dir_all(&src).unwrap();
    let f1 = src.join("orig");
    fs::write(&f1, b"hardlink data").unwrap();
    let hl = src.join("hlink");
    fs::hard_link(&f1, &hl).unwrap();
    let sl = src.join("slink");
    std::os::unix::fs::symlink("orig", &sl).unwrap();

    let walk_opts = WalkOptions {
        recursive: true,
        links: true,
        ..Default::default()
    };
    let walker = Walker::new(walk_opts);
    let sources = vec![SourcePath::from_raw(&format!("{}/", src.display()))];
    let entries = walker.scan_sources(&sources).unwrap();

    let sync_opts = SyncOptions {
        hard_links: true,
        preserve_perms: true,
        preserve_times: true,
        ..Default::default()
    };
    let engine = SyncEngine::new(sync_opts);
    engine.execute(&entries, &dest).unwrap();

    // Verify symlink
    let dest_sl = dest.join("slink");
    assert!(dest_sl.is_symlink());
    assert_eq!(
        fs::read_link(&dest_sl).unwrap(),
        std::path::PathBuf::from("orig")
    );

    // Verify hardlink (same inode)
    let m1 = FileMetadata::from_path(&dest.join("orig"), false).unwrap();
    let m2 = FileMetadata::from_path(&dest.join("hlink"), false).unwrap();
    assert_eq!(m1.file_type, FileType::Regular);
    assert_eq!(m2.file_type, FileType::Regular);
    assert_eq!(m1.ino, m2.ino);
    assert_eq!(m1.dev, m2.dev);
}

#[test]
fn test_filter_and_deletion() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");

    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&dest).unwrap();

    fs::write(src.join("keep.txt"), b"keep me").unwrap();
    fs::write(src.join("exclude.log"), b"ignore me").unwrap();
    fs::write(dest.join("stray.txt"), b"delete me").unwrap();

    let mut filters = FilterSet::new();
    filters.add_exclude("*.log");

    let walk_opts = WalkOptions {
        recursive: true,
        filters,
        ..Default::default()
    };
    let walker = Walker::new(walk_opts);
    let sources = vec![SourcePath::from_raw(&format!("{}/", src.display()))];
    let entries = walker.scan_sources(&sources).unwrap();

    let sync_opts = SyncOptions {
        delete: true,
        ..Default::default()
    };
    let engine = SyncEngine::new(sync_opts);
    engine.execute(&entries, &dest).unwrap();

    assert!(dest.join("keep.txt").exists());
    assert!(!dest.join("exclude.log").exists());
    assert!(!dest.join("stray.txt").exists());
}

#[test]
fn test_mkpath_semantics() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest_deep = tmp.path().join("a/b/c/dest_file");

    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("file.txt"), b"mkpath payload").unwrap();

    let walk_opts = WalkOptions {
        dest_is_file: true,
        ..Default::default()
    };
    let walker = Walker::new(walk_opts);
    let sources = vec![SourcePath::from_raw(
        &src.join("file.txt").to_string_lossy(),
    )];
    let entries = walker.scan_sources(&sources).unwrap();

    // 1. Without mkpath, fails with ChangeDirFailed
    let sync_no_mkpath = SyncOptions {
        dest_is_file: true,
        mkpath: false,
        ..Default::default()
    };
    let engine1 = SyncEngine::new(sync_no_mkpath);
    let res = engine1.execute(&entries, &dest_deep);
    assert!(res.is_err());
    assert!(!dest_deep.parent().unwrap().exists());

    // 2. Dry-run with mkpath succeeds without creating dirs
    let sync_dry_mkpath = SyncOptions {
        dest_is_file: true,
        mkpath: true,
        dry_run: true,
        ..Default::default()
    };
    let engine2 = SyncEngine::new(sync_dry_mkpath);
    assert!(engine2.execute(&entries, &dest_deep).is_ok());
    assert!(!dest_deep.parent().unwrap().exists());

    // 3. Real run with mkpath succeeds and creates dirs + file
    let sync_real_mkpath = SyncOptions {
        dest_is_file: true,
        mkpath: true,
        ..Default::default()
    };
    let engine3 = SyncEngine::new(sync_real_mkpath);
    assert!(engine3.execute(&entries, &dest_deep).is_ok());
    assert!(dest_deep.exists());
    assert_eq!(fs::read(&dest_deep).unwrap(), b"mkpath payload");
}

#[test]
fn test_chmod_integration() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");

    fs::create_dir_all(src.join("sub")).unwrap();
    fs::write(src.join("sub/f.txt"), b"test").unwrap();
    fs::set_permissions(src.join("sub/f.txt"), fs::Permissions::from_mode(0o644)).unwrap();
    fs::set_permissions(src.join("sub"), fs::Permissions::from_mode(0o755)).unwrap();

    let chmod_set = ssync_fs::chmod::ChmodSet::parse("D710,F600").unwrap();

    let walk_opts = WalkOptions {
        recursive: true,
        ..Default::default()
    };
    let walker = Walker::new(walk_opts);
    let sources = vec![SourcePath::from_raw(&format!("{}/", src.display()))];
    let mut entries = walker.scan_sources(&sources).unwrap();

    for entry in &mut entries {
        let is_dir = entry.metadata.file_type == ssync_fs::metadata::FileType::Directory;
        entry.metadata.mode = chmod_set.apply(entry.metadata.mode, is_dir);
    }

    let sync_opts = SyncOptions {
        preserve_perms: true,
        ..Default::default()
    };
    let engine = SyncEngine::new(sync_opts);
    engine.execute(&entries, &dest).unwrap();

    let file_mode = fs::metadata(dest.join("sub/f.txt"))
        .unwrap()
        .permissions()
        .mode()
        & 0o7777;
    assert_eq!(file_mode, 0o600);

    let dir_mode = fs::metadata(dest.join("sub")).unwrap().permissions().mode() & 0o7777;
    assert_eq!(dir_mode, 0o710);
}
