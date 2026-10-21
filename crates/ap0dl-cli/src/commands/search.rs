use anyhow::Result;
use ap0dl_core::Config;

use super::engine;

pub async fn run(
    config: Config,
    query: &str,
    provider: Option<&str>,
    limit: usize,
    json: bool,
) -> Result<()> {
    let engine = engine(config)?;
    let results = engine
        .search(provider, query, 1, limit)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    if json {
        let values: Vec<_> = results
            .into_iter()
            .map(|(provider, result)| {
                serde_json::json!({
                    "provider": provider.id(),
                    "id": result.id,
                    "title": result.title,
                    "url": result.url,
                    "cover": result.cover,
                    "year": result.year,
                    "format": result.format,
                    "episodes": result.episodes,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&values)?);
        return Ok(());
    }

    if results.is_empty() {
        eprintln!("No results for {query:?}.");
        return Ok(());
    }

    for (index, (provider, result)) in results.iter().enumerate() {
        println!("{:>2}. [{}] {}", index + 1, provider.name(), result.title);
        println!("    {}", result.url);
        if let Some(year) = result.year {
            println!("    {year}");
        }
    }
    Ok(())
}
