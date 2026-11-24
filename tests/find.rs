//! Testes da biblioteca sobre árvores reais de arquivos criadas em pastas temporárias.

use std::fs;
use std::path::{Path, PathBuf};

use dupfind::{DuplicateGroup, ScanOptions, find_duplicates};
use tempfile::TempDir;

fn write(root: &Path, relative: &str, contents: &[u8]) -> PathBuf {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, contents).unwrap();
    path
}

fn options() -> ScanOptions {
    ScanOptions {
        min_size: 1,
        ignore_hidden: false,
    }
}

/// Caminhos do grupo relativos à raiz, sempre com `/`, para comparar igual em todo SO.
fn relative(group: &DuplicateGroup, root: &Path) -> Vec<String> {
    group
        .paths
        .iter()
        .map(|path| {
            path.strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect()
}

fn root_of(dir: &TempDir) -> Vec<PathBuf> {
    vec![dir.path().to_path_buf()]
}

#[test]
fn groups_identical_files_across_directories() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a/one.txt", b"hello");
    write(dir.path(), "b/nested/two.txt", b"hello");
    write(dir.path(), "c/other.txt", b"world");

    let report = find_duplicates(&root_of(&dir), &options());

    assert_eq!(report.scanned_files, 3);
    assert_eq!(report.groups.len(), 1);
    let group = &report.groups[0];
    assert_eq!(
        relative(group, dir.path()),
        ["a/one.txt", "b/nested/two.txt"]
    );
    assert_eq!(group.size, 5);
    assert_eq!(group.hash, blake3::hash(b"hello"));
    assert!(report.errors.is_empty());
}

#[test]
fn large_files_that_differ_only_after_the_prefix_are_not_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let base = vec![7u8; 64 * 1024];
    let mut changed_tail = base.clone();
    *changed_tail.last_mut().unwrap() = 8;

    write(dir.path(), "original.bin", &base);
    write(dir.path(), "copy.bin", &base);
    write(dir.path(), "changed.bin", &changed_tail);

    let report = find_duplicates(&root_of(&dir), &options());

    assert_eq!(report.groups.len(), 1);
    assert_eq!(
        relative(&report.groups[0], dir.path()),
        ["copy.bin", "original.bin"]
    );
    // O hash do grupo é o do arquivo inteiro, não o do prefixo.
    assert_eq!(report.groups[0].hash, blake3::hash(&base));
}

#[test]
fn files_with_same_content_but_different_sizes_never_match() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "short.txt", b"abc");
    write(dir.path(), "long.txt", b"abcabc");

    assert!(
        find_duplicates(&root_of(&dir), &options())
            .groups
            .is_empty()
    );
}

#[test]
fn empty_files_are_skipped_unless_min_size_is_zero() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "empty-1", b"");
    write(dir.path(), "empty-2", b"");

    let report = find_duplicates(&root_of(&dir), &options());
    assert_eq!(report.scanned_files, 0);
    assert!(report.groups.is_empty());

    let with_empty = ScanOptions {
        min_size: 0,
        ..options()
    };
    assert_eq!(find_duplicates(&root_of(&dir), &with_empty).groups.len(), 1);
}

#[test]
fn min_size_filters_small_files() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "small-1", b"abc");
    write(dir.path(), "small-2", b"abc");
    write(dir.path(), "big-1", b"abcdef");
    write(dir.path(), "big-2", b"abcdef");

    let report = find_duplicates(
        &root_of(&dir),
        &ScanOptions {
            min_size: 4,
            ..options()
        },
    );

    assert_eq!(report.scanned_files, 2);
    assert_eq!(report.groups.len(), 1);
    assert_eq!(relative(&report.groups[0], dir.path()), ["big-1", "big-2"]);
}

#[test]
fn hidden_files_and_directories_can_be_ignored() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "visible.txt", b"same");
    write(dir.path(), ".dotfile", b"same");
    write(dir.path(), ".cache/inside.txt", b"same");

    let all = find_duplicates(&root_of(&dir), &options());
    assert_eq!(all.groups[0].paths.len(), 3);

    let visible_only = find_duplicates(
        &root_of(&dir),
        &ScanOptions {
            ignore_hidden: true,
            ..options()
        },
    );
    assert_eq!(visible_only.scanned_files, 1);
    assert!(visible_only.groups.is_empty());
}

#[test]
fn overlapping_roots_do_not_report_a_file_as_its_own_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "sub/a.txt", b"payload");
    write(dir.path(), "b.txt", b"payload");

    let roots = vec![
        dir.path().join("sub"),
        dir.path().to_path_buf(),
        dir.path().to_path_buf(),
    ];
    let report = find_duplicates(&roots, &options());

    assert_eq!(report.scanned_files, 2);
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].paths.len(), 2);
}

#[test]
fn single_files_can_be_passed_as_roots() {
    let dir = tempfile::tempdir().unwrap();
    let first = write(dir.path(), "first.txt", b"twin");
    let second = write(dir.path(), "second.txt", b"twin");

    let report = find_duplicates(&[first, second], &options());

    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].paths.len(), 2);
}

#[test]
fn groups_are_sorted_by_reclaimable_space() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "small-1", b"tiny");
    write(dir.path(), "small-2", b"tiny");
    for name in ["big-1", "big-2", "big-3"] {
        write(dir.path(), name, &[1u8; 1000]);
    }

    let report = find_duplicates(&root_of(&dir), &options());

    assert_eq!(report.groups.len(), 2);
    assert_eq!(report.groups[0].reclaimable_bytes(), 2000);
    assert_eq!(report.groups[1].reclaimable_bytes(), 4);

    let summary = report.summary();
    assert_eq!(summary.duplicate_groups, 2);
    assert_eq!(summary.redundant_files, 3);
    assert_eq!(summary.reclaimable_bytes, 2004);
}

#[test]
fn missing_roots_become_errors_without_aborting_the_scan() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a", b"dup");
    write(dir.path(), "b", b"dup");

    let roots = vec![dir.path().to_path_buf(), dir.path().join("does-not-exist")];
    let report = find_duplicates(&roots, &options());

    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.errors.len(), 1);
}
