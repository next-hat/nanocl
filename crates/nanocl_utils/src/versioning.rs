pub fn format_version(version: &str) -> String {
  let mut index = 0;
  let mut dot_count = 0;
  for (i, c) in version.chars().enumerate() {
    if c == '.' {
      dot_count += 1;
      if dot_count == 2 {
        index = i;
        break;
      }
    }
  }
  // Extract the substring up to the second dot
  version[..index].to_owned()
}
