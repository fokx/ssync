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

#[test]
fn test_keep_dirlinks_and_symlinked_parent() {
    let tmp = tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");

    fs::create_dir_all(src.join("real/sub")).unwrap();
    fs::create_dir_all(&dest).unwrap();
    fs::write(src.join("real/sub/file.txt"), b"in-tree content").unwrap();
    std::os::unix::fs::symlink("real", src.join("link")).unwrap();

    // 1. -aR through an in-tree symlink parent
    let walk_opts = WalkOptions {
        recursive: true,
        links: true,
        relative: true,
        ..Default::default()
    };
    let walker = Walker::new(walk_opts);
    let sources = vec![SourcePath::from_raw(&format!(
        "{}/./link/sub/file.txt",
        src.display()
    ))];
    let entries = walker.scan_sources(&sources).unwrap();

    let sync_opts = SyncOptions {
        relative: true,
        ..Default::default()
    };
    let engine = SyncEngine::new(sync_opts);
    engine.execute(&entries, &dest).unwrap();

    assert!(dest.join("link/sub/file.txt").is_file());
    assert_eq!(
        fs::read(dest.join("link/sub/file.txt")).unwrap(),
        b"in-tree content"
    );

    // 2. --keep-dirlinks: writes land through destination symlinked dir
    let dest2 = tmp.path().join("dest2");
    fs::create_dir_all(dest2.join("target")).unwrap();
    std::os::unix::fs::symlink("target", dest2.join("d")).unwrap();

    let src2 = tmp.path().join("src2");
    fs::create_dir_all(src2.join("d")).unwrap();
    fs::write(src2.join("d/item.txt"), b"through symlink").unwrap();

    let walk_opts2 = WalkOptions {
        recursive: true,
        ..Default::default()
    };
    let walker2 = Walker::new(walk_opts2);
    let sources2 = vec![SourcePath::from_raw(&format!("{}/", src2.display()))];
    let entries2 = walker2.scan_sources(&sources2).unwrap();

    let sync_opts2 = SyncOptions {
        keep_dirlinks: true,
        ..Default::default()
    };
    let engine2 = SyncEngine::new(sync_opts2);
    engine2.execute(&entries2, &dest2).unwrap();

    assert!(dest2.join("d").is_symlink());
    assert!(dest2.join("target/item.txt").is_file());
    assert_eq!(
        fs::read(dest2.join("target/item.txt")).unwrap(),
        b"through symlink"
    );
}

#[test]
fn test_dest_symlinked_dir_follows_symlink() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let real = tmp.path().join("realdest");
    let link = tmp.path().join("backup");

    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&real).unwrap();
    fs::write(src.join("f.txt"), b"content\n").unwrap();
    std::os::unix::fs::symlink(&real, &link).unwrap();

    let walk_opts = WalkOptions {
        recursive: true,
        ..Default::default()
    };
    let walker = Walker::new(walk_opts);
    let sources = vec![SourcePath::from_raw(&format!("{}/", src.display()))];
    let entries = walker.scan_sources(&sources).unwrap();

    let sync_opts = SyncOptions {
        preserve_times: true,
        preserve_perms: true,
        ..Default::default()
    };
    let engine = SyncEngine::new(sync_opts);
    engine.execute(&entries, &link).unwrap();

    assert!(link.is_symlink(), "backup should remain a symlink");
    assert!(real.join("f.txt").is_file(), "file must land in realdest");
    assert_eq!(fs::read(real.join("f.txt")).unwrap(), b"content\n");
}

#[test]
fn test_out_format_and_stats_execution() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");

    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("testfile.txt"), b"1234567890").unwrap();

    let walk_opts = WalkOptions {
        recursive: true,
        ..Default::default()
    };
    let walker = Walker::new(walk_opts);
    let sources = vec![SourcePath::from_raw(&format!("{}/", src.display()))];
    let entries = walker.scan_sources(&sources).unwrap();

    let sync_opts = SyncOptions {
        stats: true,
        out_format: Some("100%% done %f (%n, %l bytes)".to_string()),
        human_readable: 1,
        ..Default::default()
    };
    let engine = SyncEngine::new(sync_opts);
    engine.execute(&entries, &dest).unwrap();

    assert!(dest.join("testfile.txt").is_file());
    assert_eq!(fs::read(dest.join("testfile.txt")).unwrap(), b"1234567890");
}

#[test]
fn test_files_from_and_path_clamp() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");

    fs::create_dir_all(src.join("d1/d2/d3")).unwrap();
    fs::write(src.join("d1/f1"), b"f1").unwrap();
    fs::write(src.join("d1/d2/d3/f3"), b"f3").unwrap();
    fs::write(src.join("unlisted.txt"), b"unlisted").unwrap();
    fs::write(src.join("file"), b"in-src-file").unwrap();
    fs::write(src.join("keep"), b"in-src-keep").unwrap();

    let list_content = "\
# comment
; semi-comment

/file
d1/f1
d1/d2/d3/f3
../keep
";
    let list_file = tmp.path().join("files.lst");
    fs::write(&list_file, list_content).unwrap();

    let walk_opts = WalkOptions {
        recursive: true,
        ..Default::default()
    };
    let walker = Walker::new(walk_opts);
    let src_path = SourcePath::from_raw(&format!("{}/", src.display()));
    let entries = walker
        .scan_files_from(&src_path, list_file.to_str().unwrap(), false)
        .unwrap();

    let sync_opts = SyncOptions {
        ..Default::default()
    };
    let engine = SyncEngine::new(sync_opts);
    engine.execute(&entries, &dest).unwrap();

    assert!(dest.join("file").is_file());
    assert_eq!(fs::read(dest.join("file")).unwrap(), b"in-src-file");

    assert!(dest.join("d1/f1").is_file());
    assert_eq!(fs::read(dest.join("d1/f1")).unwrap(), b"f1");

    assert!(dest.join("d1/d2/d3/f3").is_file());
    assert_eq!(fs::read(dest.join("d1/d2/d3/f3")).unwrap(), b"f3");

    assert!(dest.join("keep").is_file());
    assert_eq!(fs::read(dest.join("keep")).unwrap(), b"in-src-keep");

    assert!(!dest.join("unlisted.txt").exists());
}

#[test]
fn test_backup_symlink_safe_links() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");

    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&dest).unwrap();

    fs::write(src.join("escape"), b"replacement").unwrap();
    std::os::unix::fs::symlink("../../sensitive", dest.join("escape")).unwrap();

    let walk_opts = WalkOptions {
        recursive: true,
        ..Default::default()
    };
    let walker = Walker::new(walk_opts);
    let sources = vec![SourcePath::from_raw(&format!("{}/", src.display()))];
    let entries = walker.scan_sources(&sources).unwrap();

    // 1. With safe_links: unsafe symlink is dropped from backup
    let sync_opts = SyncOptions {
        backup: true,
        suffix: ".bak".to_string(),
        safe_links: true,
        ..Default::default()
    };
    let engine = SyncEngine::new(sync_opts);
    engine.execute(&entries, &dest).unwrap();

    assert!(!dest.join("escape.bak").exists());
    assert!(dest.join("escape").is_file());

    // 2. Without safe_links: unsafe symlink is backed up
    fs::remove_file(dest.join("escape")).unwrap();
    std::os::unix::fs::symlink("../../sensitive", dest.join("escape")).unwrap();

    let sync_opts2 = SyncOptions {
        backup: true,
        suffix: ".bak".to_string(),
        safe_links: false,
        ..Default::default()
    };
    let engine2 = SyncEngine::new(sync_opts2);
    engine2.execute(&entries, &dest).unwrap();

    assert!(dest.join("escape.bak").is_symlink());
    assert_eq!(
        fs::read_link(dest.join("escape.bak")).unwrap(),
        std::path::PathBuf::from("../../sensitive")
    );
}

#[test]
fn test_fuzzy_basis_distance_and_selection() {
    use ssync_fs::sync::{UNIT, find_filename_suffix, fuzzy_distance};

    // 1. Suffix unit tests
    assert_eq!(find_filename_suffix("archive-v2.tar"), ".tar");
    assert_eq!(find_filename_suffix("foo.tar.gz"), ".gz");
    assert_eq!(find_filename_suffix("backup.bak"), "");
    assert_eq!(find_filename_suffix("old.orig"), "");
    assert_eq!(find_filename_suffix("file.c~"), ".c");

    // 2. Distance scoring unit tests
    assert_eq!(fuzzy_distance(b"foo.tar", b"foo.tar", 25 << 16), 0);
    let d1 = fuzzy_distance(b"archive-v1.tar", b"archive-v2.tar", 25 << 16);
    let d2 = fuzzy_distance(b"archive-old.tar", b"archive-v2.tar", 25 << 16);
    assert!(d1 < d2);
    assert_eq!(d1, UNIT + 1); // |'1' - '2'| = 1

    // 3. Integration sync with fuzzy basis
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&src).unwrap();
    fs::create_dir_all(&dest).unwrap();

    let mut base_data = vec![0x42u8; 100_000];
    base_data.extend_from_slice(b"new tail data added in v2");
    fs::write(src.join("archive-v2.tar"), &base_data).unwrap();

    // Destination has archive-v1.tar
    let mut v1_data = vec![0x42u8; 100_000];
    v1_data.extend_from_slice(b"older tail data");
    fs::write(dest.join("archive-v1.tar"), &v1_data).unwrap();

    let walker = Walker::new(WalkOptions {
        recursive: true,
        ..Default::default()
    });
    let sources = vec![SourcePath::from_raw(&format!("{}/", src.display()))];
    let entries = walker.scan_sources(&sources).unwrap();

    let sync_opts = SyncOptions {
        fuzzy: 1,
        no_whole_file: true,
        debug: Some("FUZZY".to_string()),
        ..Default::default()
    };
    let engine = SyncEngine::new(sync_opts);
    engine.execute(&entries, &dest).unwrap();

    // archive-v2.tar exists with correct content, archive-v1.tar remains
    assert_eq!(fs::read(dest.join("archive-v2.tar")).unwrap(), base_data);
    assert_eq!(fs::read(dest.join("archive-v1.tar")).unwrap(), v1_data);
}
