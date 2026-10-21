use anyhow::Result;
use ap0dl_core::Config;
use ap0dl_core::legacy;

use super::engine;

pub async fn run(config: Config, json: bool, show_legacy: bool) -> Result<()> {
    if show_legacy {
        let legacy = legacy::all();
        if json {
            println!("{}", serde_json::to_string_pretty(&legacy)?);
            return Ok(());
        }

        println!("{:<10} {:<22} {:<12} OBSERVED", "ID", "NAME", "STATUS");
        for info in legacy {
            println!(
                "{:<10} {:<22} {:<12} {}",
                info.id, info.name, info.status, info.observed
            );
            println!("           {}", info.evidence);
        }
        return Ok(());
    }

    let engine = engine(config)?;
    let infos = engine.provider_infos();
    if json {
        println!("{}", serde_json::to_string_pretty(&infos)?);
        return Ok(());
    }

    println!("{:<10} {:<18} {:<12} BASE", "ID", "NAME", "STATUS");
    for info in &infos {
        println!(
            "{:<10} {:<18} {:<12} {}",
            info.id,
            info.name,
            format!("{:?}", info.status).to_lowercase(),
            info.base_url
        );
        if !info.notes.is_empty() {
            println!("           {}", info.notes);
        }
    }

    println!(
        "\nLegacy/blocked: {}. Run `ap0dl providers --legacy` for observed HTTP evidence.",
        legacy::all()
            .iter()
            .map(|info| info.id)
            .collect::<Vec<_>>()
            .join(", ")
    );
    Ok(())
}
