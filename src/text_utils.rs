use tower_lsp::lsp_types::Position;

pub fn utf16_position_to_byte_offset(line: &str, position: Position) -> usize {
  let mut utf16_count = 0;
  for (idx, ch) in line.char_indices() {
    if utf16_count >= position.character {
      return idx;
    }
    utf16_count += ch.len_utf16() as u32;
  }
  line.len()
}

pub fn byte_offset_to_utf16_position(line: &str, byte_offset: usize) -> u32 {
  let mut utf16_count = 0;
  for (idx, ch) in line.char_indices() {
    if idx >= byte_offset {
      break;
    }
    utf16_count += ch.len_utf16() as u32;
  }
  utf16_count
}

pub fn extract_word_at_position(text: &str, position: Position) -> Option<String> {
  let line = text.lines().nth(position.line as usize)?;
  let byte_index = utf16_position_to_byte_offset(line, position);
  let bytes = line.as_bytes();
  if byte_index > bytes.len() {
    return None;
  }
  let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
  let mut start = byte_index;
  while start > 0 && is_word(bytes[start - 1]) {
    start -= 1;
  }
  let mut end = byte_index;
  while end < bytes.len() && is_word(bytes[end]) {
    end += 1;
  }
  if start == end {
    return None;
  }
  Some(line[start..end].to_string())
}

pub fn extract_word_prefix_at_position(text: &str, position: Position) -> Option<(String, usize)> {
  let line = text.lines().nth(position.line as usize)?;
  let byte_index = utf16_position_to_byte_offset(line, position);
  let bytes = line.as_bytes();
  if byte_index > bytes.len() {
    return None;
  }
  let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
  let mut start = byte_index;
  while start > 0 && is_word(bytes[start - 1]) {
    start -= 1;
  }
  if start == byte_index {
    return None;
  }
  Some((line[start..byte_index].to_string(), start))
}
