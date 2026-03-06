use std::path::{Path, PathBuf};

use clap::Args;
use filament_core::dto::CreateEntityRequest;
use filament_core::error::{FilamentError, Result};
use filament_core::models::EntityType;

use super::helpers::{connect, find_project_root, output_json};
use crate::Cli;

#[derive(Args, Debug)]
pub struct SeedArgs {
    /// Seed from CLAUDE.md in the project root (default behavior).
    #[arg(long, default_value = "true")]
    claude_md: bool,
    /// Path to a specific markdown file to ingest (parses ## sections as Doc entities).
    #[arg(long)]
    file: Option<PathBuf>,
    /// Path to a text file listing markdown file paths to ingest (one path per line).
    #[arg(long)]
    files: Option<PathBuf>,
    /// Dry run: show what would be created without creating anything.
    #[arg(long)]
    dry_run: bool,
}

struct SeedItem {
    name: String,
    entity_type: EntityType,
    summary: String,
    source: String,
}

pub async fn seed(cli: &Cli, args: &SeedArgs) -> Result<()> {
    let root = find_project_root()?;
    let mut items: Vec<SeedItem> = Vec::new();

    // Collect files to parse
    let mut files_to_parse: Vec<PathBuf> = Vec::new();

    if let Some(ref csv_path) = args.files {
        let csv_content = std::fs::read_to_string(csv_path)
            .map_err(|e| FilamentError::Validation(format!("cannot read CSV file: {e}")))?;
        for line in csv_content.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            let path = PathBuf::from(trimmed);
            if !path.exists() {
                eprintln!("warning: skipping non-existent file: {trimmed}");
                continue;
            }
            files_to_parse.push(path);
        }
    }

    if let Some(ref file_path) = args.file {
        if !file_path.exists() {
            return Err(FilamentError::Validation(format!(
                "file not found: {}",
                file_path.display()
            )));
        }
        files_to_parse.push(file_path.clone());
    }

    // Parse explicit files
    for path in &files_to_parse {
        items.extend(parse_markdown_file(path));
    }

    // Parse project CLAUDE.md if no explicit files provided, or if --claude-md is set
    if files_to_parse.is_empty() && args.claude_md {
        items.extend(parse_markdown_file_with_source(
            &root.join("CLAUDE.md"),
            "CLAUDE.md",
        ));
    }

    if items.is_empty() {
        if !cli.quiet {
            println!("No seed data found.");
        }
        return Ok(());
    }

    if args.dry_run {
        print_dry_run(cli, &items);
        return Ok(());
    }

    create_seed_entities(cli, &items).await
}

fn print_dry_run(cli: &Cli, items: &[SeedItem]) {
    if cli.json {
        let entries: Vec<serde_json::Value> = items
            .iter()
            .map(|item| {
                serde_json::json!({
                    "name": item.name,
                    "type": item.entity_type.as_str(),
                    "summary": item.summary,
                    "source": item.source,
                })
            })
            .collect();
        output_json(&serde_json::json!({ "dry_run": true, "entities": entries }));
    } else {
        println!("Dry run — would create {} entities:", items.len());
        for item in items {
            println!(
                "  [{}] {}: {} (from {})",
                item.entity_type.as_str(),
                item.name,
                truncate(&item.summary, 60),
                item.source,
            );
        }
    }
}

async fn create_seed_entities(cli: &Cli, items: &[SeedItem]) -> Result<()> {
    let mut conn = connect().await?;
    let mut created = 0u32;
    let mut skipped = 0u32;

    let existing = conn.list_entities(Some(EntityType::Doc), None).await?;
    let existing_names: std::collections::HashSet<&str> =
        existing.iter().map(|e| e.name().as_str()).collect();

    for item in items {
        if existing_names.contains(item.name.as_str()) {
            skipped += 1;
            continue;
        }

        conn.create_entity(CreateEntityRequest {
            name: item.name.clone(),
            entity_type: item.entity_type,
            summary: Some(item.summary.clone()),
            key_facts: None,
            content_path: None,
            priority: None,
        })
        .await?;
        created += 1;
    }

    if cli.json {
        output_json(&serde_json::json!({
            "created": created,
            "skipped": skipped,
        }));
    } else {
        println!("Seeded {created} entities ({skipped} skipped as duplicates).");
    }
    Ok(())
}

fn parse_markdown_file(path: &Path) -> Vec<SeedItem> {
    let source = path.file_name().map_or_else(
        || path.to_string_lossy().to_string(),
        |n| n.to_string_lossy().to_string(),
    );
    parse_markdown_file_with_source(path, &source)
}

fn parse_markdown_file_with_source(path: &Path, source: &str) -> Vec<SeedItem> {
    let Ok(content) = std::fs::read_to_string(path) else {
        return Vec::new();
    };

    let mut items = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_body = String::new();

    for line in content.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            // Flush previous section
            if let Some(name) = current_heading.take() {
                let summary = extract_summary(&current_body);
                if !summary.is_empty() {
                    items.push(SeedItem {
                        name,
                        entity_type: EntityType::Doc,
                        summary,
                        source: source.to_string(),
                    });
                }
            }
            current_heading = Some(heading.trim().to_string());
            current_body.clear();
        } else if current_heading.is_some() {
            current_body.push_str(line);
            current_body.push('\n');
        }
    }

    // Flush last section
    if let Some(name) = current_heading.take() {
        let summary = extract_summary(&current_body);
        if !summary.is_empty() {
            items.push(SeedItem {
                name,
                entity_type: EntityType::Doc,
                summary,
                source: source.to_string(),
            });
        }
    }

    items
}

fn extract_summary(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    // Take first non-empty, non-heading, non-table-separator line
    let first_line = trimmed
        .lines()
        .find(|l| {
            let t = l.trim();
            !t.is_empty() && !t.starts_with('#') && !t.starts_with("---") && !t.starts_with("```")
        })
        .unwrap_or("");

    // Clean up markdown artifacts
    let cleaned = first_line
        .trim()
        .trim_start_matches("- ")
        .trim_start_matches("* ");

    truncate(cleaned, 200).to_string()
}

fn truncate(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        // Find a valid char boundary
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    // -----------------------------------------------------------------------
    // truncate
    // -----------------------------------------------------------------------

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_exact_length_unchanged() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_long_string() {
        assert_eq!(truncate("hello world", 5), "hello");
    }

    #[test]
    fn truncate_empty_string() {
        assert_eq!(truncate("", 10), "");
    }

    #[test]
    fn truncate_zero_max() {
        assert_eq!(truncate("hello", 0), "");
    }

    #[test]
    fn truncate_utf8_boundary() {
        // 'é' is 2 bytes (0xC3 0xA9); truncating at byte 1 must back up
        let s = "é";
        assert_eq!(s.len(), 2);
        let result = truncate(s, 1);
        assert_eq!(result, "");
    }

    #[test]
    fn truncate_multibyte_mid_string() {
        // "aé" = 3 bytes; truncate at 2 should keep only "a"
        let s = "aé";
        assert_eq!(truncate(s, 2), "a");
    }

    // -----------------------------------------------------------------------
    // extract_summary
    // -----------------------------------------------------------------------

    #[test]
    fn extract_summary_empty_body() {
        assert_eq!(extract_summary(""), "");
        assert_eq!(extract_summary("   \n  \n"), "");
    }

    #[test]
    fn extract_summary_plain_text() {
        assert_eq!(extract_summary("First line\nSecond line"), "First line");
    }

    #[test]
    fn extract_summary_skips_headings() {
        let body = "### Sub-heading\nActual content here";
        assert_eq!(extract_summary(body), "Actual content here");
    }

    #[test]
    fn extract_summary_skips_hr_and_code_fence_lines() {
        // extract_summary skips lines starting with ---, ```, or #
        // but content inside fences is still visible
        let body = "---\n```rust\nlet x = 1;\n```\nReal summary";
        // First non-skipped line is "let x = 1;" (inside the fence)
        assert_eq!(extract_summary(body), "let x = 1;");

        // If the first real content is after all skipped lines:
        let body2 = "---\n### Sub\nActual content";
        assert_eq!(extract_summary(body2), "Actual content");
    }

    #[test]
    fn extract_summary_strips_list_markers() {
        assert_eq!(extract_summary("- Bullet point"), "Bullet point");
        assert_eq!(extract_summary("* Star point"), "Star point");
    }

    #[test]
    fn extract_summary_skips_blank_lines() {
        let body = "\n\n\nActual content";
        assert_eq!(extract_summary(body), "Actual content");
    }

    #[test]
    fn extract_summary_truncates_long_lines() {
        let long = "x".repeat(300);
        let result = extract_summary(&long);
        assert_eq!(result.len(), 200);
    }

    // -----------------------------------------------------------------------
    // parse_markdown_file_with_source (via temp files)
    // -----------------------------------------------------------------------

    fn write_temp_md(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        f
    }

    #[test]
    fn parse_basic_headings() {
        let f = write_temp_md("## First\nContent one\n## Second\nContent two\n");
        let items = parse_markdown_file_with_source(f.path(), "test.md");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].name, "First");
        assert_eq!(items[0].summary, "Content one");
        assert_eq!(items[0].source, "test.md");
        assert_eq!(items[1].name, "Second");
        assert_eq!(items[1].summary, "Content two");
    }

    #[test]
    fn parse_no_headings_returns_empty() {
        let f = write_temp_md("Just some text without any headings.\nMore text.");
        let items = parse_markdown_file_with_source(f.path(), "test.md");
        assert!(items.is_empty());
    }

    #[test]
    fn parse_heading_with_empty_body_skipped() {
        let f = write_temp_md("## Empty\n\n## HasContent\nSome text");
        let items = parse_markdown_file_with_source(f.path(), "test.md");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "HasContent");
    }

    #[test]
    fn parse_h1_and_h3_ignored() {
        let f = write_temp_md("# H1\nBody\n### H3\nMore\n## H2\nActual");
        let items = parse_markdown_file_with_source(f.path(), "test.md");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "H2");
    }

    #[test]
    fn parse_heading_whitespace_trimmed() {
        let f = write_temp_md("##   Spaced Heading  \nBody text");
        let items = parse_markdown_file_with_source(f.path(), "test.md");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "Spaced Heading");
    }

    #[test]
    fn parse_nonexistent_file_returns_empty() {
        let items = parse_markdown_file_with_source(Path::new("/nonexistent/file.md"), "ghost.md");
        assert!(items.is_empty());
    }

    #[test]
    fn parse_markdown_file_uses_filename_as_source() {
        let f = write_temp_md("## Section\nBody");
        let items = parse_markdown_file(f.path());
        assert_eq!(items.len(), 1);
        // Source should be the filename, not the full path
        let filename = f.path().file_name().unwrap().to_string_lossy().to_string();
        assert_eq!(items[0].source, filename);
    }

    #[test]
    fn parse_body_with_only_code_fence_skipped() {
        let f = write_temp_md("## Code Only\n```\nlet x = 1;\n```\n");
        let items = parse_markdown_file_with_source(f.path(), "test.md");
        // The body's first non-skipped line is "let x = 1;" (inside the fence is not skipped by extract_summary rules)
        // Actually: extract_summary skips ``` lines but "let x = 1;" passes through
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].summary, "let x = 1;");
    }

    #[test]
    fn parse_multiple_sections_last_flushed() {
        let f = write_temp_md("## A\nContent A\n## B\nContent B\n## C\nContent C");
        let items = parse_markdown_file_with_source(f.path(), "test.md");
        assert_eq!(items.len(), 3);
        assert_eq!(items[2].name, "C");
        assert_eq!(items[2].summary, "Content C");
    }

    #[test]
    fn all_items_are_doc_type() {
        let f = write_temp_md("## One\nBody\n## Two\nBody");
        let items = parse_markdown_file_with_source(f.path(), "test.md");
        for item in &items {
            assert_eq!(item.entity_type, EntityType::Doc);
        }
    }
}
