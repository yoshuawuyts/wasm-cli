#![allow(clippy::print_stdout, clippy::print_stderr)]

use bytesize::ByteSize;
use std::io::{Stdout, Write};

use anyhow::Result;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{CellAlignment, ContentArrangement, Table};
use wasm_metadata::{Metadata, Payload};
use wasm_package_manager::Reference;
use wasm_package_manager::manager::Manager;
use wasm_package_manager::oci::filter_wasm_layers;

/// Inspect the metadata of a package on the registry.
#[derive(clap::Args)]
pub(crate) struct InspectOpts {
    /// The reference to inspect (e.g., ghcr.io/example/component:tag)
    #[arg(value_parser = crate::util::parse_reference)]
    reference: Reference,

    /// Output in JSON encoding
    #[clap(long)]
    json: bool,
}

impl InspectOpts {
    pub(crate) async fn run(self, store: &Manager) -> Result<()> {
        let reference = self.reference;
        let pull_result = store.pull(reference.clone()).await?;

        let manifest = pull_result.manifest.as_ref().ok_or_else(|| {
            super::errors::InspectError::NoManifest {
                reference: reference.whole().clone(),
            }
        })?;

        let wasm_layers = filter_wasm_layers(&manifest.layers);
        let layer =
            wasm_layers
                .first()
                .ok_or_else(|| super::errors::InspectError::NoWasmLayer {
                    reference: reference.whole().clone(),
                })?;

        let data = store.get(&layer.digest).await?;
        let payload = Payload::from_binary(&data)?;

        let mut output = std::io::stdout();
        if self.json {
            write!(output, "{}", serde_json::to_string(&payload)?)?;
        } else {
            write_summary_table(&payload, &mut output)?;
            write_details_table(&payload, &mut output)?;
        }
        Ok(())
    }
}

/// Get the max value of the `range` field across a payload and all children.
fn find_range_max(max: &mut usize, payload: &Payload) {
    let range = &payload.metadata().range;
    if range.end > *max {
        *max = range.end;
    }

    if let Payload::Component { children, .. } = payload {
        for child in children {
            find_range_max(max, child);
        }
    }
}

/// Write a table containing a summarized overview of a wasm binary's metadata to
/// a writer.
fn write_summary_table(payload: &Payload, f: &mut Stdout) -> Result<()> {
    // Prepare a table and get the individual metadata
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(80)
        .set_header(vec!["KIND", "NAME", "SIZE", "SIZE%", "LANGUAGES", "PARENT"]);

    table
        .column_mut(2)
        .expect("This should be the SIZE column")
        .set_cell_alignment(CellAlignment::Right);

    table
        .column_mut(3)
        .expect("This should be the SIZE% column")
        .set_cell_alignment(CellAlignment::Right);

    let mut range_max = 0;
    find_range_max(&mut range_max, payload);

    // Recursively add all children to the table
    write_summary_table_inner(payload, "<root>", &mut 0, range_max, &mut table)?;

    // Write the table to the writer
    writeln!(f, "{table}")?;

    Ok(())
}

// The recursing inner function of `write_summary_table`
fn write_summary_table_inner(
    payload: &Payload,
    parent: &str,
    unknown_id: &mut u16,
    range_max: usize,
    table: &mut Table,
) -> Result<()> {
    let Metadata {
        name,
        range,
        producers,
        ..
    } = payload.metadata();

    let name = if let Some(name) = name.as_deref() {
        name.to_owned()
    } else {
        let name = format!("unknown({unknown_id})");
        *unknown_id += 1;
        name
    };
    let size_bytes = range.end - range.start;
    let size = ByteSize::b(u64::try_from(size_bytes).unwrap_or(u64::MAX))
        .display()
        .si_short()
        .to_string();

    let percent = if range_max == 0 {
        0usize
    } else {
        size_bytes.saturating_mul(100) / range_max
    };
    let usep = match u8::try_from(percent.min(100)).unwrap_or(100) {
        // If the item was truly empty, it wouldn't be part of the binary
        0..=1 => "<1%".to_string(),
        // We're hedging against the low-ends, this hedges against the high-ends.
        // Makes sure we don't see a mix of <1% and 100% in the same table, unless
        // the item is actually 100% of the binary.
        100 if range.end != range_max => ">99%".to_string(),
        usep => format!("{usep}%"),
    };
    let kind = match payload {
        Payload::Component { .. } => "component",
        Payload::Module(_) => "module",
    };
    let languages = match producers {
        Some(producers) => match producers.iter().find(|(name, _)| *name == "language") {
            Some((_, pairs)) => pairs
                .iter()
                .map(|(lang, _)| lang.to_owned())
                .collect::<Vec<_>>()
                .join(", "),
            None => "-".to_string(),
        },
        None => "-".to_string(),
    };

    table.add_row(vec![&kind, &*name, &*size, &usep, &languages, &parent]);

    // Recursively print any children
    if let Payload::Component { children, .. } = payload {
        for payload in children {
            write_summary_table_inner(payload, &name, unknown_id, range_max, table)?;
        }
    }

    Ok(())
}

/// Write a table containing a detailed overview of a wasm binary's metadata to
/// a writer.
fn write_details_table(payload: &Payload, f: &mut Stdout) -> Result<()> {
    // Prepare a table and get the individual metadata
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(80)
        .set_header(vec!["KIND", "VALUE"]);
    let Metadata {
        name,
        authors,
        description,
        producers,
        licenses,
        source,
        homepage,
        range,
        revision,
        version,
        dependencies,
    } = payload.metadata();

    // Add the basic information to the table first
    let name = name.as_deref().unwrap_or("<unknown>");
    table.add_row(vec!["name", &name]);
    let kind = match payload {
        Payload::Component { .. } => "component",
        Payload::Module(_) => "module",
    };
    table.add_row(vec!["kind", &kind]);
    table.add_row(vec![
        "range",
        &format!("0x{:x}..0x{:x}", range.start, range.end),
    ]);

    // Add the OCI annotations to the table
    if let Some(description) = description {
        table.add_row(vec!["description", &description.to_string()]);
    }
    if let Some(authors) = authors {
        table.add_row(vec!["authors", &authors.to_string()]);
    }
    if let Some(version) = version {
        table.add_row(vec!["version", &version.to_string()]);
    }
    if let Some(revision) = revision {
        table.add_row(vec!["revision", &revision.to_string()]);
    }
    if let Some(licenses) = licenses {
        table.add_row(vec!["licenses", &licenses.to_string()]);
    }
    if let Some(source) = source {
        table.add_row(vec!["source", &source.to_string()]);
    }
    if let Some(homepage) = homepage {
        table.add_row(vec!["homepage", &homepage.to_string()]);
    }

    // Add the producer section to the table
    if let Some(producers) = producers {
        // Ensure the "language" fields are listed first
        let mut producers = producers
            .iter()
            .map(|(n, p)| (n.clone(), p))
            .collect::<Vec<_>>();
        producers.sort_by(|(a, _), (b, _)| {
            if a == "language" {
                std::cmp::Ordering::Less
            } else if b == "language" {
                std::cmp::Ordering::Greater
            } else {
                a.cmp(b)
            }
        });

        // Add the producers to the table
        for (name, pairs) in &producers {
            for (field, version) in pairs.iter() {
                match version.len() {
                    0 => table.add_row(vec![name, field]),
                    _ => table.add_row(vec![name, &format!("{field} [{version}]")]),
                };
            }
        }
    }

    // Add child relationships to the table
    if let Payload::Component { children, .. } = &payload {
        for payload in children {
            let name = payload.metadata().name.as_deref().unwrap_or("<unknown>");
            let kind = match payload {
                Payload::Component { .. } => "component",
                Payload::Module(_) => "module",
            };
            table.add_row(vec!["child", &format!("{name} [{kind}]")]);
        }
    }

    // Add dependency packages to the table
    if let Some(dependencies) = dependencies {
        for package in &dependencies.version_info().packages {
            table.add_row(vec![
                "dependency",
                &format!("{} [{}]", package.name, package.version),
            ]);
        }
    }

    // Write the table to the writer
    writeln!(f, "{table}")?;

    // Recursively print any children
    if let Payload::Component { children, .. } = payload {
        for payload in children {
            write_details_table(payload, f)?;
        }
    }

    Ok(())
}
