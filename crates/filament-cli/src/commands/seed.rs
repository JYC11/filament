use std::path::Path;

use clap::Args;
use filament_core::dto::CreateEntityRequest;
use filament_core::error::Result;
use filament_core::models::EntityType;

use super::helpers::{connect, find_project_root, output_json};
use crate::Cli;

#[derive(Args, Debug)]
pub struct SeedArgs {
    /// Seed from CLAUDE.md (parse sections as Doc entities).
    #[arg(long, default_value = "true")]
    claude_md: bool,
    /// Dry run: show what would be created without creating anything.
    #[arg(long)]
    dry_run: bool,
}

struct SeedItem {
    name: String,
    entity_type: EntityType,
    summary: String,
}

pub async fn seed(cli: &Cli, args: &SeedArgs) -> Result<()> {
    let root = find_project_root()?;
    let mut items: Vec<SeedItem> = Vec::new();

    if args.claude_md {
        items.extend(parse_claude_md(&root));
    }


    if items.is_empty() {
        if !cli.quiet {
            println!("No seed data found.");
        }
        return Ok(());
    }

    if args.dry_run {
        if cli.json {
            let entries: Vec<serde_json::Value> = items
                .iter()
                .map(|item| {
                    serde_json::json!({
                        "name": item.name,
                        "type": item.entity_type.as_str(),
                        "summary": item.summary,
                    })
                })
                .collect();
            output_json(&serde_json::json!({ "dry_run": true, "entities": entries }));
        } else {
            println!("Dry run — would create {} entities:", items.len());
            for item in &items {
                println!(
                    "  [{}] {}: {}",
                    item.entity_type.as_str(),
                    item.name,
                    truncate(&item.summary, 60)
                );
            }
        }
        return Ok(());
    }

    let mut conn = connect().await?;
    let mut created = 0u32;
    let mut skipped = 0u32;

    for item in &items {
        // Check if entity with same name already exists by trying to resolve
        let existing = conn
            .list_entities(Some(item.entity_type.clone()), None)
            .await?;
        if existing.iter().any(|e| e.name().as_str() == item.name) {
            skipped += 1;
            continue;
        }

        conn.create_entity(CreateEntityRequest {
            name: item.name.clone(),
            entity_type: item.entity_type.clone(),
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

fn parse_claude_md(root: &Path) -> Vec<SeedItem> {
    let claude_path = root.join("CLAUDE.md");
    let Ok(content) = std::fs::read_to_string(&claude_path) else {
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
