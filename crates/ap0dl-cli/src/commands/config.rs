use anyhow::Result;
use ap0dl_core::Config;

pub async fn run(config: Config, path: bool, sample: bool, init: bool) -> Result<()> {
    if path {
        println!("{}", Config::default_config_path().display());
        return Ok(());
    }
    if sample {
        print!("{}", Config::sample());
        return Ok(());
    }
    if init {
        let path = Config::default_config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if path.exists() {
            anyhow::bail!("{} already exists", path.display());
        }
        std::fs::write(&path, Config::sample())?;
        println!("Wrote {}", path.display());
        return Ok(());
    }

    let text = toml::to_string_pretty(&config)?;
    println!("{text}");
    if let Some(path) = Config::find_config_file() {
        eprintln!("# loaded from {}", path.display());
    } else {
        eprintln!("# using built-in defaults (no config file found)");
    }
    Ok(())
}
