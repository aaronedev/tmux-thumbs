use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::state::Match;

/// Category of a match, used to pick a hint color variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
  None,
  File,
  Directory,
  Github,
  Url,
}

/// Assign a kind to every match, caching filesystem lookups by token text.
pub fn assign(matches: &mut [Match<'_>], cwd: Option<&str>) {
  let mut cache: HashMap<String, Kind> = HashMap::new();

  for mat in matches.iter_mut() {
    let kind = match cache.get(mat.text) {
      Some(kind) => *kind,
      None => {
        let kind = classify(mat.text, cwd);
        cache.insert(mat.text.to_string(), kind);
        kind
      }
    };

    mat.kind = kind;
  }
}

pub fn classify(text: &str, cwd: Option<&str>) -> Kind {
  let trimmed = trim_wrappers(text);

  if trimmed.is_empty() {
    return Kind::None;
  }

  if let Some(kind) = classify_url(trimmed) {
    return kind;
  }

  classify_path(trimmed, cwd)
}

/// Strip wrapping punctuation that terminals and prose add around tokens.
fn trim_wrappers(text: &str) -> &str {
  text
    .trim_matches(|c: char| matches!(c, '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | '<' | '>' | ','))
    .trim()
}

fn classify_url(text: &str) -> Option<Kind> {
  let url = text.trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ':' | '!' | '?'));

  if let Some(host) = url_host(url) {
    return Some(if is_github_host(&host) { Kind::Github } else { Kind::Url });
  }

  if url.starts_with("git@github.com:") || url.starts_with("git@www.github.com:") {
    return Some(Kind::Github);
  }

  None
}

fn url_host(url: &str) -> Option<String> {
  let rest = ["http://", "https://", "git://", "ssh://", "ftp://"]
    .iter()
    .find_map(|scheme| url.strip_prefix(scheme))?;

  let authority = rest.split(|c| c == '/' || c == '?' || c == '#').next().unwrap_or("");
  let host = authority.rsplit('@').next().unwrap_or("");
  let host = host.split(':').next().unwrap_or("");

  if host.is_empty() {
    None
  } else {
    Some(host.to_ascii_lowercase())
  }
}

fn is_github_host(host: &str) -> bool {
  host == "github.com" || host.ends_with(".github.com")
}

fn classify_path(text: &str, cwd: Option<&str>) -> Kind {
  if text.contains("://") || text.starts_with("git@") {
    return Kind::None;
  }

  let candidate: PathBuf = if text == "~" || text.starts_with("~/") {
    match std::env::var("HOME") {
      Ok(home) => {
        let rest = text.strip_prefix("~/").unwrap_or("");
        Path::new(&home).join(rest)
      }
      Err(_) => return Kind::None,
    }
  } else {
    let path = Path::new(text);

    if path.is_absolute() {
      path.to_path_buf()
    } else {
      match cwd {
        Some(dir) if !dir.is_empty() => Path::new(dir).join(path),
        _ => return Kind::None,
      }
    }
  };

  match fs::metadata(&candidate) {
    Ok(metadata) => {
      if metadata.is_dir() {
        Kind::Directory
      } else {
        Kind::File
      }
    }
    Err(_) => Kind::None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn github_urls() {
    assert_eq!(classify("https://github.com/a/b", None), Kind::Github);
    assert_eq!(classify("https://gist.github.com/a/b", None), Kind::Github);
    assert_eq!(classify("git@github.com:a/b.git", None), Kind::Github);
    assert_eq!(classify("https://github.com", None), Kind::Github);
  }

  #[test]
  fn other_urls() {
    assert_eq!(classify("https://example.com/a?b=1", None), Kind::Url);
    assert_eq!(classify("<https://example.com/a>", None), Kind::Url);
    assert_eq!(classify("https://example.com/a.", None), Kind::Url);
  }

  #[test]
  fn files_and_directories() {
    let root = std::env::temp_dir().join(format!("thumbs-kind-{}", std::process::id()));
    fs::create_dir_all(root.join("dir")).unwrap();
    fs::write(root.join("file.txt"), "").unwrap();

    let cwd = root.to_str().unwrap();

    assert_eq!(classify("file.txt", Some(cwd)), Kind::File);
    assert_eq!(classify("dir", Some(cwd)), Kind::Directory);
    assert_eq!(classify("./dir", Some(cwd)), Kind::Directory);
    assert_eq!(classify("missing.txt", Some(cwd)), Kind::None);
    assert_eq!(classify(root.join("file.txt").to_str().unwrap(), None), Kind::File);
    assert_eq!(classify("\"file.txt\"", Some(cwd)), Kind::File);

    fs::remove_dir_all(&root).ok();
  }

  #[test]
  fn plain_tokens_stay_uncolored() {
    assert_eq!(classify("clearly-not-a-path", None), Kind::None);
    assert_eq!(classify("clearly-not-a-path", Some("/")), Kind::None);
    assert_eq!(classify("", Some("/")), Kind::None);
  }
}
