pub fn parse_url(url: &str) -> Result<String, std::io::Error> {
  match url {
    url if url.starts_with("http") => Ok(url.to_owned()),
    _ => Err(std::io::Error::new(
      std::io::ErrorKind::InvalidInput,
      "Invalid url",
    )),
  }
}
