use std::path::PathBuf;

use clap::Args;
use filament_core::dto::{CreateCommon, CreateContentRequired, CreateEntityRequest};
use filament_core::error::{FilamentError, Result};
use filament_core::models::EntityType;

use super::helpers::{connect, output_json};
use crate::Cli;

#[derive(Args, Debug)]
pub struct SeedArgs {
    /// Path to a specific file to ingest as a Doc entity with `content_path`.
    #[arg(long)]
    file: Option<PathBuf>,
    /// Path to a text file listing file paths to ingest (one path per line).
    #[arg(long)]
    files: Option<PathBuf>,
    /// Dry run: show what would be created without creating anything.
    #[arg(long)]
    dry_run: bool,
}

struct SeedItem {
    name: String,
    summary: String,
    content_path: String,
}

pub async fn seed(cli: &Cli, args: &SeedArgs) -> Result<()> {
    let mut file_paths: Vec<PathBuf> = Vec::new();

    if let Some(ref csv_path) = args.files {
        let csv_content = std::fs::read_to_string(csv_path)
            .map_err(|e| FilamentError::Validation(format!("cannot read file list: {e}")))?;
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
            file_paths.push(path);
        }
    }

    if let Some(ref file_path) = args.file {
        if !file_path.exists() {
            return Err(FilamentError::Validation(format!(
                "file not found: {}",
                file_path.display()
            )));
        }
        file_paths.push(file_path.clone());
    }

    if file_paths.is_empty() {
        return Err(FilamentError::Validation(
            "specify --file or --files to seed Doc entities".into(),
        ));
    }

    let mut items: Vec<SeedItem> = Vec::new();
    for path in &file_paths {
        let filename = path.file_name().map_or_else(
            || path.to_string_lossy().to_string(),
            |n| n.to_string_lossy().to_string(),
        );
        let content_path = path.to_string_lossy().to_string();
        let summary = std::fs::read_to_string(path)
            .map_or_else(|_| String::new(), |content| extract_summary(&content));
        items.push(SeedItem {
            name: filename,
            summary,
            content_path,
        });
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
                    "type": "doc",
                    "summary": item.summary,
                    "content_path": item.content_path,
                })
            })
            .collect();
        output_json(&serde_json::json!({ "dry_run": true, "entities": entries }));
    } else {
        println!("Dry run — would create {} entities:", items.len());
        for item in items {
            println!(
                "  [doc] {}: {} ({})",
                item.name,
                truncate(&item.summary, 60),
                item.content_path,
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

        conn.create_entity(CreateEntityRequest::Doc(CreateContentRequired {
            common: CreateCommon {
                name: item.name.clone(),
                summary: Some(item.summary.clone()),
                priority: None,
                key_facts: None,
            },
            content_path: item.content_path.clone(),
        }))
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
    // seed_item construction from files
    // -----------------------------------------------------------------------

    fn write_temp_file(name: &str, content: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f.flush().unwrap();
        (dir, path)
    }

    #[test]
    fn seed_item_name_is_filename() {
        let (_dir, path) = write_temp_file("CLAUDE.md", "# Heading\nSome content");
        let filename = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap();
        assert_eq!(filename, "CLAUDE.md");
    }

    #[test]
    fn seed_item_summary_from_file_content() {
        let (_dir, path) = write_temp_file("notes.md", "First meaningful line\nSecond line");
        let content = std::fs::read_to_string(&path).unwrap();
        let summary = extract_summary(&content);
        assert_eq!(summary, "First meaningful line");
    }

    #[test]
    fn seed_item_content_path_is_file_path() {
        let (_dir, path) = write_temp_file("test.md", "content");
        let content_path = path.to_string_lossy().to_string();
        assert!(content_path.ends_with("test.md"));
    }

    #[test]
    fn seed_item_summary_skips_headings_in_file() {
        let (_dir, path) = write_temp_file("doc.md", "# Title\n## Section\nActual content");
        let content = std::fs::read_to_string(&path).unwrap();
        let summary = extract_summary(&content);
        assert_eq!(summary, "Actual content");
    }

    #[test]
    fn seed_item_empty_file_gives_empty_summary() {
        let (_dir, path) = write_temp_file("empty.md", "");
        let content = std::fs::read_to_string(&path).unwrap();
        let summary = extract_summary(&content);
        assert_eq!(summary, "");
    }
}
