//! Benchmarks for git-mode modification times: the per-render
//! `FsStorage::mtime` cost when `mtimeSource` is `git`.
//!
//! The fixture has a few hundred commits that each touch one hot file, so the
//! other pages were last changed in the root commit and their history walk is
//! as long as the history.
//!
//! Local: cargo bench -p rw-vcs --bench mtime

use std::path::PathBuf;

use divan::counter::ItemsCount;
use divan::{Bencher, black_box};
use rw_git_fixture::GitFixture;
use rw_vcs::Vcs;

const PAGES: usize = 40;
const COMMITS: usize = 300;

fn main() {
    divan::main();
}

fn fixture() -> (GitFixture, Vec<PathBuf>) {
    let fixture = GitFixture::init();
    for page in 0..PAGES {
        fixture.write(&format!("page-{page}.md"), &format!("# Page {page}\n"));
    }
    fixture.commit_all("pages");
    for commit in 0..COMMITS {
        fixture.write("changelog.md", &format!("# Changelog {commit}\n"));
        fixture.commit_all("changelog");
    }
    // Packed like a clone; loose objects would make every walk several times slower.
    fixture.git(&["gc", "--quiet"]);
    let paths = (0..PAGES)
        .map(|page| fixture.path().join(format!("page-{page}.md")))
        .collect();
    (fixture, paths)
}

fn mtime_all(vcs: &Vcs, paths: &[PathBuf]) {
    for path in paths {
        black_box(vcs.mtime(&[path.as_path()]));
    }
}

/// Every page once on a long-lived `Vcs`, as a server renders them.
#[divan::bench(sample_count = 30)]
fn warm(bencher: Bencher) {
    let (fixture, paths) = fixture();
    let vcs = Vcs::new(fixture.path());
    bencher
        .counter(ItemsCount::new(PAGES))
        .bench(|| mtime_all(&vcs, &paths));
}

/// Every page once on a fresh `Vcs`: the first render of each page.
#[divan::bench(sample_count = 30)]
fn cold(bencher: Bencher) {
    let (fixture, paths) = fixture();
    let root = fixture.path();
    bencher
        .counter(ItemsCount::new(PAGES))
        .with_inputs(|| Vcs::new(root))
        .bench_refs(|vcs| mtime_all(vcs, &paths));
}
