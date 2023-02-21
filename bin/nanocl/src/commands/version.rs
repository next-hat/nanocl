use ntex::http::Client;
use nanocld_client::NanocldClient;
use ntex::http::client::error::SendRequestError;
use ntex::time::Millis;
use serde::Deserialize;

use crate::models::VersionArgs;
use crate::version;

use crate::error::CliError;

pub async fn exec_version(
  client: &NanocldClient,
  args: &VersionArgs,
) -> Result<(), CliError> {
  match args.command.is_some() {
    true => print_latest_version().await,
    false => print_version(client).await,
  }
}

async fn print_version(client: &NanocldClient) -> Result<(), CliError> {
  println!("=== [nanocli] ===");
  version::print_version();

  let daemon_version = client.get_version().await?;
  println!("=== [nanocld] ===");
  println!(
    "Arch: {}\nChannel: {}\nVersion: {}\nCommit ID: {}",
    daemon_version.arch,
    daemon_version.channel,
    daemon_version.version,
    daemon_version.commit_id
  );

  Ok(())
}

async fn print_latest_version() -> Result<(), CliError> {
  let version = get_latest_version().await?;
  match &version {
    Some(version) => {
      println!("{version}");
    }
    None => {
      // VERY LIKELY will never happen
      println!("Failed to find a last version");
    }
  }

  Ok(())
}

async fn get_latest_version() -> Result<Option<String>, CliError> {
  let tags = github_get_tags().await.map_err(|e| CliError::Custom {
    msg: format!("Failed to get tags from GitHub: {e}"),
  })?;
  let last_version = tags.first().map(|tag| tag.name.clone());

  Ok(last_version)
}

async fn github_get_tags() -> Result<Vec<GitHubAPITag>, SendRequestError> {
  const NANOCL_GITHUB_TAGS_URL: &str =
    "https://api.github.com/repos/nxthat/nanocl/tags";
  const MAX_BODY_SIZE: usize = 20_000_000;
  const HTTP_TIMEOUT: Millis = Millis::from_secs(5);

  let client = Client::default();

  let mut body = client
    .get(NANOCL_GITHUB_TAGS_URL)
    .header("User-Agent", "ntex::web")
    .send()
    .await?;

  let tags: Vec<GitHubAPITag> = body
    .json()
    .limit(MAX_BODY_SIZE)
    .timeout(HTTP_TIMEOUT)
    .await
    .map_err(|e| SendRequestError::Error(Box::new(e)))?;

  Ok(tags)
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct GitHubAPITag {
  name: String,
  node_id: String,
  zipball_url: String,
  tarball_url: String,
  commit: GitHubAPITagCommit,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Deserialize)]
struct GitHubAPITagCommit {
  sha: String,
  url: String,
}
