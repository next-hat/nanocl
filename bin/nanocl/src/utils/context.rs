use crate::{
  models::{Context, ContextRow},
  config,
};

impl Context {
  pub fn new() -> Self {
    Self::default()
  }

  pub fn ensure() -> std::io::Result<()> {
    let home = std::env::var("HOME").map_err(|_| {
      std::io::Error::new(std::io::ErrorKind::Other, "Could not get $HOME")
    })?;
    let path = format!("{home}/.nanocl/contexts");
    std::fs::create_dir_all(path)?;
    Ok(())
  }

  pub fn read(path: &str) -> std::io::Result<Context> {
    let s = std::fs::read_to_string(path)?;
    let context = serde_yaml::from_str::<Context>(&s).map_err(|err| {
      std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("Could not parse context {path}: {err}"),
      )
    })?;
    Ok(context)
  }

  pub fn read_by_name(name: &str) -> std::io::Result<Context> {
    let home = std::env::var("HOME").map_err(|_| {
      std::io::Error::new(std::io::ErrorKind::Other, "Could not get $HOME")
    })?;
    let path = format!("{home}/.nanocl/contexts/{name}.yml");
    let context = Self::read(&path)?;
    Ok(context)
  }

  pub fn write(context: &Context) -> std::io::Result<()> {
    let home = std::env::var("HOME").map_err(|_| {
      std::io::Error::new(std::io::ErrorKind::Other, "Could not get $HOME")
    })?;
    let path = format!("{home}/.nanocl/contexts/{}.yml", context.name);
    let s = serde_yaml::to_string(&context).map_err(|err| {
      std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("Could not serialize context {}: {err}", context.name),
      )
    })?;
    std::fs::write(path, s)?;
    Ok(())
  }

  pub fn list() -> std::io::Result<Vec<ContextRow>> {
    let home = std::env::var("HOME").map_err(|_| {
      std::io::Error::new(std::io::ErrorKind::Other, "Could not get $HOME")
    })?;
    let path = format!("{home}/.nanocl/contexts");
    let mut contexts = vec![ContextRow::from(Context::new())];
    for entry in std::fs::read_dir(path)? {
      let entry = entry?;
      let path = entry.path();
      let path = path.to_string_lossy().to_string();
      if let Ok(context) = Self::read(&path) {
        contexts.push(ContextRow::from(context));
      }
    }
    Ok(contexts)
  }

  pub fn r#use(name: &str) -> std::io::Result<()> {
    let home = std::env::var("HOME").map_err(|_| {
      std::io::Error::new(std::io::ErrorKind::Other, "Could not get $HOME")
    })?;
    if name != "default" {
      Context::read_by_name(name).map_err(|err| {
        std::io::Error::new(
          std::io::ErrorKind::InvalidData,
          format!("Could not read context {name}: {err}"),
        )
      })?;
    }
    let path = format!("{home}/.nanocl/conf.yml");
    let mut config = config::read();
    config.current_context = name.to_string();
    let s = serde_yaml::to_string(&config).map_err(|err| {
      std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("Could not serialize config: {err}"),
      )
    })?;
    std::fs::write(path, s)?;
    Ok(())
  }
}
