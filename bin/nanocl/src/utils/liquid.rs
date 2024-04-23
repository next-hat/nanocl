use std::{borrow::Cow, fs, path::Path};
use liquid::partials::PartialSource;
use url::Url;
use crate::{models::StateRoot, utils::state::download_statefile};

#[derive(Default, Debug, Clone)]
pub struct StateSource {
  pub root: StateRoot,
}

impl StateSource {
  pub fn fetch_partial<'a>(
    name: String,
    root: Option<String>,
  ) -> std::option::Option<std::borrow::Cow<'a, str>> {
    let url = if let Some(ref url) = root {
      Url::parse(url)
        .expect("Can't parse url ")
        .join(&name)
        .expect("Can't join base and root url")
    } else {
      Url::parse(&name).expect("Can't parse url ")
    }
    .as_str()
    .to_owned();
    std::thread::spawn(|| {
      ntex::rt::System::new(&url)
        .block_on(async move { download_statefile(&url).await })
    })
    .join()
    .unwrap()
    .ok()
    .map(|(_, data)| data.into())
  }

  pub fn read_partial<'a, RootPath: AsRef<Path>>(
    name: String,
    root: Option<RootPath>,
  ) -> Option<Cow<'a, str>> {
    let mut path = Path::new(&name).to_path_buf();
    if let Some(ref dir) = root {
      if !path.has_root() {
        let new_path =
          Path::new(dir.as_ref()).join(path.clone()).canonicalize();
        if let Ok(new_path) = new_path {
          if new_path.exists() && new_path.is_file() {
            path = new_path;
          }
        }
      }
    }
    match path.as_path().canonicalize() {
      Ok(path) => {
        let path = path.to_str().unwrap();
        match fs::read_to_string(path) {
          Ok(content) => Some(content.into()),
          Err(_) => None,
        }
      }
      Err(_) => None,
    }
  }
}

impl PartialSource for StateSource {
  fn contains(&self, _name: &str) -> bool {
    true
  }

  fn names(&self) -> Vec<&str> {
    vec![]
  }

  fn try_get<'a>(
    &'a self,
    name: &str,
  ) -> std::option::Option<std::borrow::Cow<'a, str>> {
    if name.starts_with("http://") || name.starts_with("https://") {
      return StateSource::fetch_partial(name.to_owned(), None);
    }
    match &self.root {
      StateRoot::File(root) => {
        StateSource::read_partial(name.to_owned(), Some(root))
      }
      StateRoot::Url(root) => {
        StateSource::fetch_partial(name.to_owned(), Some(root.to_string()))
      }
      StateRoot::None => {
        StateSource::read_partial::<String>(name.to_owned(), None)
      }
    }
  }
}
