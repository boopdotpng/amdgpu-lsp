use crate::types::InstructionEntry;

pub fn normalize_architecture_hint(raw: &str) -> String {
  let cleaned = raw.trim().to_ascii_lowercase().replace(' ', "");
  if let Some(rem) = cleaned.strip_prefix("rdna") {
    if rem.len() == 2 && rem.chars().all(|ch| ch.is_ascii_digit()) {
      let (major, minor) = rem.split_at(1);
      return format!("rdna{major}.{minor}");
    }
  }
  cleaned
}

pub fn architecture_filter(language_id: &str, override_arch: Option<&String>) -> Option<String> {
  if let Some(override_arch) = override_arch {
    if !override_arch.trim().is_empty() {
      return Some(normalize_architecture_hint(override_arch));
    }
  }
  match language_id {
    "rdna35" => Some("rdna3.5".to_string()),
    "rdna3" => Some("rdna3".to_string()),
    "rdna4" => Some("rdna4".to_string()),
    "cdna3" => Some("cdna3".to_string()),
    "cdna4" => Some("cdna4".to_string()),
    "rdna" => Some("rdna".to_string()),
    "cdna" => Some("cdna".to_string()),
    _ => None,
  }
}

pub fn entry_matches_arch(entry: &InstructionEntry, filter: &str) -> bool {
  if filter.starts_with("rdna") {
    if filter == "rdna" {
      return entry.architectures.iter().any(|arch| arch.starts_with("rdna"));
    }
    return entry.architectures.iter().any(|arch| arch == filter);
  }
  if filter.starts_with("cdna") {
    if filter == "cdna" {
      return entry.architectures.iter().any(|arch| arch.starts_with("cdna"));
    }
    return entry.architectures.iter().any(|arch| arch == filter);
  }
  entry.architectures.iter().any(|arch| arch == filter)
}
