use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::path::PathBuf;

fn generate_large_document(lines: usize) -> String {
    let mut doc = String::from("---\nid: bench-doc\ntitle: Benchmark Document\n---\n\n");
    for i in 0..lines {
        match i % 5 {
            0 => doc.push_str(&format!("# Heading {i}\n\n")),
            1 => doc.push_str(&format!(
                "This is a paragraph about [[page-{i}]] with #tag-{i}.\n\n"
            )),
            2 => doc.push_str(&format!("- List item {i} with a [[link-{i}|display text]]\n")),
            3 => doc.push_str(&format!("> Blockquote line {i}\n\n")),
            _ => doc.push_str(&format!(
                "Some text mentioning rope data structure and page {i}.\n\n"
            )),
        }
    }
    doc
}

fn setup_large_index(
    pages: usize,
) -> (tempfile::TempDir, bloom_core::index::SqliteIndex) {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("bench.db");
    let mut index = bloom_core::index::SqliteIndex::open(&db_path).unwrap();

    for i in 0..pages {
        let content = format!(
            "---\nid: page-{i}\ntitle: Page {i}\n---\n\n\
             This is the content of page {i}. It talks about rope data structure \
             and various algorithms. #topic-{tag}\n",
            i = i,
            tag = i % 10
        );
        let doc = bloom_core::parser::parse(&content).unwrap();
        let path = PathBuf::from(format!("pages/page-{i}.md"));
        index.index_document(&path, &doc).unwrap();
    }

    (tmp, index)
}

fn bench_parser_throughput(c: &mut Criterion) {
    let content = generate_large_document(1000);
    c.bench_function("parse_1000_lines", |b| {
        b.iter(|| bloom_core::parser::parse(black_box(&content)))
    });
}

fn bench_fts_query(c: &mut Criterion) {
    let (_tmp, index) = setup_large_index(1000);
    c.bench_function("fts_search_1000_pages", |b| {
        b.iter(|| index.search(black_box("rope data structure")))
    });
}

fn bench_display_hint_update(c: &mut Criterion) {
    // Build an in-memory store with 100 pages that all link to a target page.
    let tmp = tempfile::tempdir().unwrap();
    let pages_dir = tmp.path().join("pages");
    std::fs::create_dir_all(&pages_dir).unwrap();

    let target_id = "target-page";
    for i in 0..100 {
        let content = format!(
            "---\nid: page-{i}\ntitle: Page {i}\n---\n\n\
             This page references [[{target}|Old Title]] somewhere.\n",
            i = i,
            target = target_id
        );
        std::fs::write(pages_dir.join(format!("page-{i}.md")), &content).unwrap();
    }

    let store = bloom_core::store::LocalFileStore::new(tmp.path().to_path_buf()).unwrap();

    c.bench_function("display_hint_update_100_pages", |b| {
        b.iter(|| {
            bloom_core::hint_updater::update_display_hints(
                black_box(&store),
                black_box(target_id),
                black_box("New Title"),
            )
        })
    });
}

criterion_group!(
    benches,
    bench_parser_throughput,
    bench_fts_query,
    bench_display_hint_update
);
criterion_main!(benches);
