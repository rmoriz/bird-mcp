use std::path::PathBuf;

use bird_mcp::docs::Docs;
use tempfile::TempDir;

const SAMPLE: &str = "\
<chapt>Remote control
<label id=\"remote-control\">

<sect>Usage
<label id=\"cli-usage\">

<p>Here is a brief list of supported functions.

<descrip>
	<tag><label id=\"cli-show-route\">show route [<m/prefix/]</tag>
	Show contents of the routing table.
	Use <cf/table all/ to see every table.

	<tag><label id=\"cli-configure-check\">configure check [\"<m/config file/\"]</tag>
	Read and parse the given config file, but do not use it. Useful for checking
	syntactic and some semantic validity of the configuration.
</descrip>

<sect>Filters
<label id=\"filters\">

<p>Filters are written in BIRD's filter language.
";

fn docs() -> (TempDir, Docs) {
    let dir = tempfile::tempdir().expect("temp dir");
    let doc_dir = dir.path().join("doc");
    std::fs::create_dir_all(&doc_dir).expect("mkdir");
    std::fs::write(doc_dir.join("bird.sgml"), SAMPLE).expect("write sgml");
    std::fs::write(
        doc_dir.join("bird.conf.example"),
        "router id 192.0.2.1;\nprotocol device {}\n",
    )
    .expect("write example");
    let docs = Docs::locate(Some(doc_dir)).expect("docs");
    (dir, docs)
}

#[test]
fn search_finds_lines_case_insensitively() {
    let (_dir, docs) = docs();
    let hits = docs.search("CONFIG FILE", 10).expect("search");
    assert!(!hits.is_empty());
    assert_eq!(hits[0].file, "bird.sgml");
    assert!(hits[0].line > 1);
    assert!(hits[0].text.contains("config file"));
}

#[test]
fn search_requires_all_terms() {
    let (_dir, docs) = docs();
    let hits = docs.search("route table", 10).expect("search");
    assert!(hits.is_empty());
}

#[test]
fn search_rejects_empty_query() {
    let (_dir, docs) = docs();
    let err = docs.search("   ", 10).unwrap_err();
    assert!(err.to_string().contains("query is empty"));
}

#[test]
fn section_returns_the_tag_body() {
    let (_dir, docs) = docs();
    let section = docs.section("cli-configure-check").expect("section");
    assert_eq!(section.file, "bird.sgml");
    assert!(section.text.contains("configure check"));
    assert!(section.text.contains("do not use it"));
    assert!(!section.text.contains("Filters are written"));
}

#[test]
fn section_for_chapter_stops_at_next_section() {
    let (_dir, docs) = docs();
    let section = docs.section("remote-control").expect("section");
    assert!(section.text.contains("Remote control"));
    assert!(!section.text.contains("bfd"));
}

#[test]
fn unknown_section_lists_expected_shape() {
    let (_dir, docs) = docs();
    let err = docs.section("does-not-exist").unwrap_err();
    assert!(err.to_string().contains("cli-show-route"));
}

#[test]
fn read_returns_requested_lines() {
    let (_dir, docs) = docs();
    let chunk = docs.read("bird.sgml", 2, 3).expect("read");
    assert_eq!(chunk.start_line, 2);
    assert_eq!(chunk.end_line, 4);
    assert_eq!(chunk.total_lines, SAMPLE.lines().count());
    assert!(chunk.text.starts_with("<label id=\"remote-control\">"));
}

#[test]
fn read_rejects_path_traversal() {
    let (_dir, docs) = docs();
    let err = docs.read("../Cargo.toml", 1, 10).unwrap_err();
    assert!(err.to_string().contains("relative path"));
}

#[test]
fn locate_reports_a_bad_configured_directory() {
    let dir = tempfile::tempdir().expect("temp dir");
    let err = Docs::locate(Some(PathBuf::from(dir.path()))).unwrap_err();
    assert!(err.to_string().contains("bird.sgml"));
}
