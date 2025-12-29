use crate::architecture::{architecture_filter, entry_matches_arch, normalize_architecture_hint};
use crate::encoding::split_encoding_variant;
use crate::formatting::{format_hover, format_mnemonic, format_special_register_hover};
use crate::text_utils::{
  byte_offset_to_utf16_position, extract_word_at_position, extract_word_prefix_at_position,
  utf16_position_to_byte_offset,
};
use crate::types::{DocumentState, DocumentStore, InstructionEntry, IsaLoadInfo, SpecialRegister};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
  CompletionItem, CompletionItemKind, CompletionList, CompletionOptions, CompletionParams,
  CompletionResponse, CompletionTextEdit, Hover, HoverParams,
  GotoDefinitionParams, GotoDefinitionResponse, HoverProviderCapability, InitializeParams,
  InitializeResult, Location, MessageType, OneOf, ParameterInformation, ParameterLabel, Position,
  Range, ServerCapabilities, SignatureHelp, SignatureHelpOptions, SignatureHelpParams,
  SignatureInformation, TextDocumentContentChangeEvent, TextDocumentItem, TextDocumentSyncCapability,
  TextDocumentSyncKind, TextEdit, Url,
};
use tower_lsp::{Client, LanguageServer};

pub struct IsaServer {
  client: Client,
  docs: Arc<Mutex<DocumentStore>>,
  index: HashMap<String, Vec<InstructionEntry>>,
  special_registers: Vec<SpecialRegister>,
  architecture_override: Arc<Mutex<Option<String>>>,
  load_info: IsaLoadInfo,
}

impl IsaServer {
  pub fn new(
    client: Client,
    index: HashMap<String, Vec<InstructionEntry>>,
    special_registers: Vec<SpecialRegister>,
    load_info: IsaLoadInfo,
  ) -> Self {
    Self {
      client,
      docs: Arc::new(Mutex::new(DocumentStore::default())),
      index,
      special_registers,
      architecture_override: Arc::new(Mutex::new(None)),
      load_info,
    }
  }

  fn get_document(&self, uri: &Url) -> Option<DocumentState> {
    self.docs.lock().ok()?.docs.get(uri).cloned()
  }
}

#[tower_lsp::async_trait]
impl LanguageServer for IsaServer {
  async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
    if let Some(options) = params.initialization_options {
      if let Some(override_arch) = options.get("architectureOverride").and_then(|value| value.as_str()) {
        if let Ok(mut stored) = self.architecture_override.lock() {
          *stored = Some(normalize_architecture_hint(override_arch));
        }
      }
    }
    if let Some(error) = &self.load_info.load_error {
      self
        .client
        .log_message(MessageType::ERROR, format!("{error} (path: {})", self.load_info.data_path))
        .await;
    } else {
      let total_entries: usize = self.index.values().map(|entries| entries.len()).sum();
      self
        .client
        .log_message(
          MessageType::INFO,
          format!(
            "Loaded {} ISA entries ({} unique names) from {}",
            total_entries,
            self.index.len(),
            self.load_info.data_path
          ),
        )
        .await;
    }
    Ok(InitializeResult {
      capabilities: ServerCapabilities {
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
        signature_help_provider: Some(SignatureHelpOptions {
          trigger_characters: Some(vec![" ".to_string()]),
          retrigger_characters: None,
          work_done_progress_options: Default::default(),
        }),
        definition_provider: Some(OneOf::Left(true)),
        completion_provider: Some(CompletionOptions {
          trigger_characters: Some(vec!["_".to_string(), ".".to_string()]),
          resolve_provider: Some(false),
          work_done_progress_options: Default::default(),
          all_commit_characters: None,
          completion_item: None,
        }),
        ..ServerCapabilities::default()
      },
      ..InitializeResult::default()
    })
  }

  async fn did_open(&self, params: tower_lsp::lsp_types::DidOpenTextDocumentParams) {
    let TextDocumentItem {
      uri,
      text,
      language_id,
      ..
    } = params.text_document;
    if let Ok(mut store) = self.docs.lock() {
      store.docs.insert(
        uri,
        DocumentState {
          text,
          language_id,
        },
      );
    }
  }

  async fn did_change(&self, params: tower_lsp::lsp_types::DidChangeTextDocumentParams) {
    if let Some(TextDocumentContentChangeEvent { text, .. }) = params.content_changes.into_iter().last() {
      let uri = params.text_document.uri.clone();
      let mut new_len = None;
      if let Ok(mut store) = self.docs.lock() {
        let entry = store.docs.entry(uri.clone()).or_insert(DocumentState {
          text: String::new(),
          language_id: String::new(),
        });
        entry.text = text;
        new_len = Some(entry.text.len());
      }
      let _ = new_len;
    }
  }

  async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    let doc = match self.get_document(&uri) {
      Some(doc) => doc,
      None => {
        return Ok(None);
      }
    };
    let word = match extract_word_at_position(&doc.text, position) {
      Some(word) => word,
      None => {
        return Ok(None);
      }
    };
    if let Some(register) = self
      .special_registers
      .iter()
      .find(|register| register.name.eq_ignore_ascii_case(&word))
    {
      return Ok(Some(Hover {
        contents: format_special_register_hover(register),
        range: None,
      }));
    }
    // Split encoding variant from instruction name
    let split = split_encoding_variant(&word);
    let key = split.base.to_ascii_lowercase();
    let entries = match self.index.get(&key) {
      Some(entries) => entries,
      None => return Ok(None),
    };
    let override_arch = self.architecture_override.lock().ok().and_then(|value| value.clone());
    if let Some(filter) = architecture_filter(&doc.language_id, override_arch.as_ref()) {
      if let Some(entry) = entries.iter().find(|entry| entry_matches_arch(entry, &filter)) {
        return Ok(Some(Hover {
          contents: format_hover(entry, &split.variant),
          range: None,
        }));
      }
      return Ok(None);
    }
    Ok(Some(Hover {
      contents: format_hover(&entries[0], &split.variant),
      range: None,
    }))
  }

  async fn signature_help(&self, params: SignatureHelpParams) -> Result<Option<SignatureHelp>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    let doc = match self.get_document(&uri) {
      Some(doc) => doc,
      None => {
        return Ok(None);
      }
    };

    // Get the current line
    let line = match doc.text.lines().nth(position.line as usize) {
      Some(line) => line,
      None => return Ok(None),
    };
    let cursor_byte = utf16_position_to_byte_offset(line, position);
    if let Some(comment_start) = line.find(';') {
      if cursor_byte >= comment_start {
        return Ok(None);
      }
    }

    let (label_offset, line_after_label) = strip_leading_label(line);
    if cursor_byte < label_offset {
      return Ok(None);
    }

    // Find the instruction at the start of the line (before any spaces/commas)
    let instruction = line_after_label
      .split(|c: char| c.is_whitespace() || c == ',')
      .next()
      .unwrap_or("");

    if instruction.is_empty() {
      return Ok(None);
    }

    // Split encoding variant from instruction name
    let split = split_encoding_variant(instruction);
    let key = split.base.to_ascii_lowercase();
    let entries = match self.index.get(&key) {
      Some(entries) => entries,
      None => {
        return Ok(None);
      }
    };

    // Filter by architecture if needed
    let override_arch = self.architecture_override.lock().ok().and_then(|value| value.clone());
    let entry = if let Some(filter) = architecture_filter(&doc.language_id, override_arch.as_ref()) {
      match entries.iter().find(|entry| entry_matches_arch(entry, &filter)) {
        Some(entry) => entry,
        None => return Ok(None),
      }
    } else {
      &entries[0]
    };

    if entry.args.is_empty() {
      return Ok(None);
    }

    let line_before_cursor = &line[..cursor_byte.min(line.len())];
    let (_, line_before_cursor) = strip_leading_label(line_before_cursor);
    let trimmed_before_cursor = line_before_cursor.trim_start();
    let args_section = match trimmed_before_cursor
      .splitn(2, |c: char| c.is_whitespace())
      .nth(1)
    {
      Some(args_section) => args_section,
      None => return Ok(None),
    };
    let commas_before_cursor = args_section.chars().filter(|&c| c == ',').count();
    let active_parameter = if entry.args.is_empty() {
      None
    } else {
      let last_index = entry.args.len().saturating_sub(1);
      Some(commas_before_cursor.min(last_index) as u32)
    };

    // Build signature with parameter information
    let mut label = format_mnemonic(&entry.name);
    let mut parameters = Vec::new();

    if !entry.args.is_empty() {
      label.push(' ');
      let args_str = entry.args.join(", ");
      let base_len = label.len();
      label.push_str(&args_str);

      // Create parameter information for each argument
      let mut current_offset = base_len;
      for (i, arg) in entry.args.iter().enumerate() {
        let arg_type = entry.arg_types.get(i).map(|s| s.as_str()).unwrap_or("");
        let compact_type = arg_type.replace("register", "reg");

        parameters.push(ParameterInformation {
          label: ParameterLabel::LabelOffsets([current_offset as u32, (current_offset + arg.len()) as u32]),
          documentation: if !compact_type.is_empty() {
            Some(tower_lsp::lsp_types::Documentation::String(compact_type))
          } else {
            None
          },
        });

        current_offset += arg.len();
        if i < entry.args.len() - 1 {
          current_offset += 2; // ", "
        }
      }
    }

    let signature = SignatureInformation {
      label,
      documentation: entry.description.as_ref().map(|desc| {
        tower_lsp::lsp_types::Documentation::String(desc.clone())
      }),
      parameters: Some(parameters),
      active_parameter,
    };

    Ok(Some(SignatureHelp {
      signatures: vec![signature],
      active_signature: Some(0),
      active_parameter,
    }))
  }

  async fn goto_definition(
    &self,
    params: GotoDefinitionParams,
  ) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    let doc = match self.get_document(&uri) {
      Some(doc) => doc,
      None => return Ok(None),
    };
    let line = match doc.text.lines().nth(position.line as usize) {
      Some(line) => line,
      None => return Ok(None),
    };
    let cursor_byte = utf16_position_to_byte_offset(line, position);
    if let Some(comment_start) = line.find(';') {
      if cursor_byte >= comment_start {
        return Ok(None);
      }
    }
    let (label, _) = match extract_label_at_position(line, position) {
      Some(value) => value,
      None => return Ok(None),
    };
    let (def_line, def_start, def_end) = match find_label_definition(&doc.text, &label) {
      Some(value) => value,
      None => return Ok(None),
    };
    let def_text = match doc.text.lines().nth(def_line as usize) {
      Some(line) => line,
      None => return Ok(None),
    };
    let start = Position {
      line: def_line,
      character: byte_offset_to_utf16_position(def_text, def_start),
    };
    let end = Position {
      line: def_line,
      character: byte_offset_to_utf16_position(def_text, def_end),
    };
    Ok(Some(GotoDefinitionResponse::Scalar(Location {
      uri,
      range: Range { start, end },
    })))
  }

  async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    let doc = match self.get_document(&uri) {
      Some(doc) => doc,
      None => return Ok(None),
    };

    let (prefix, prefix_start) = match extract_word_prefix_at_position(&doc.text, position) {
      Some((prefix, prefix_start)) => (prefix, prefix_start),
      None => return Ok(None),
    };

    let trimmed_prefix = prefix.trim();
    if trimmed_prefix.len() < 2 {
      return Ok(None);
    }

    let line = match doc.text.lines().nth(position.line as usize) {
      Some(line) => line,
      None => return Ok(None),
    };

    // Only show completions for the first word on a line (the instruction)
    let line_before_prefix = &line[..prefix_start];
    let (label_offset, line_before_prefix) = strip_leading_label(line_before_prefix);
    if prefix_start < label_offset {
      return Ok(None);
    }
    let (_, line_before_prefix) = strip_leading_disasm_prefix(line_before_prefix);
    let trimmed_line_before = line_before_prefix.trim_start();
    if !trimmed_line_before.is_empty() {
      // There's already an instruction on this line, don't suggest more
      return Ok(None);
    }

    let prefix_lower = trimmed_prefix.to_ascii_lowercase();

    // If the prefix exactly matches a no-arg instruction, don't show completions
    // (the instruction is complete, nothing more to type)
    if let Some(entries) = self.index.get(&prefix_lower) {
      if let Some(entry) = entries.first() {
        if entry.name.eq_ignore_ascii_case(trimmed_prefix) && entry.args.is_empty() {
          return Ok(None);
        }
      }
    }

    let start_char = byte_offset_to_utf16_position(line, prefix_start);
    let start = Position {
      line: position.line,
      character: start_char,
    };
    let range = Range { start, end: position };

    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::new();
    for (name, entries) in &self.index {
      if !name.contains(&prefix_lower) {
        continue;
      }
      if let Some(entry) = entries.first() {
        let label = format_mnemonic(&entry.name);
        if seen.insert(label.clone()) {
          items.push(CompletionItem {
            label: label.clone(),
            kind: Some(CompletionItemKind::KEYWORD),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
              range: range.clone(),
              new_text: label,
            })),
            ..CompletionItem::default()
          });
        }
      }
    }

    items.sort_by(|a, b| a.label.cmp(&b.label));

    Ok(Some(CompletionResponse::List(CompletionList {
      is_incomplete: true,
      items,
    })))
  }

  async fn shutdown(&self) -> Result<()> {
    Ok(())
  }
}

fn is_label_start(b: u8) -> bool {
  (b as char).is_ascii_alphabetic() || b == b'_' || b == b'.' || b == b'$'
}

fn is_label_char(b: u8) -> bool {
  is_label_start(b) || (b as char).is_ascii_digit()
}

fn is_hex_digit(b: u8) -> bool {
  (b as char).is_ascii_hexdigit()
}

fn strip_leading_label(line: &str) -> (usize, &str) {
  let trimmed = line.trim_start();
  let trimmed_offset = line.len() - trimmed.len();
  let bytes = trimmed.as_bytes();
  if bytes.is_empty() {
    return (line.len(), "");
  }
  if !is_label_start(bytes[0]) {
    return (trimmed_offset, trimmed);
  }
  let mut idx = 1;
  while idx < bytes.len() && is_label_char(bytes[idx]) {
    idx += 1;
  }
  if idx < bytes.len() && bytes[idx] == b':' {
    let after_colon = &trimmed[idx + 1..];
    let after_ws = after_colon.trim_start();
    let after_ws_offset = trimmed_offset + idx + 1 + (after_colon.len() - after_ws.len());
    return (after_ws_offset, after_ws);
  }
  (trimmed_offset, trimmed)
}

fn strip_leading_disasm_prefix(line: &str) -> (usize, &str) {
  let trimmed = line.trim_start();
  let trimmed_offset = line.len() - trimmed.len();
  let bytes = trimmed.as_bytes();
  if bytes.is_empty() {
    return (line.len(), "");
  }

  let mut idx = 0;
  let mut hex_len = 0;
  while idx < bytes.len() && is_hex_digit(bytes[idx]) {
    idx += 1;
    hex_len += 1;
  }
  if hex_len >= 4 && idx < bytes.len() && bytes[idx] == b':' {
    idx += 1;
    while idx < bytes.len() && (bytes[idx] as char).is_ascii_whitespace() {
      idx += 1;
    }
  } else {
    idx = 0;
  }

  loop {
    if idx + 8 <= bytes.len() && bytes[idx..idx + 8].iter().all(|&b| is_hex_digit(b)) {
      let mut next = idx + 8;
      if next < bytes.len() && (bytes[next] as char).is_ascii_whitespace() {
        while next < bytes.len() && (bytes[next] as char).is_ascii_whitespace() {
          next += 1;
        }
        idx = next;
        continue;
      }
    }
    break;
  }

  (trimmed_offset + idx, &trimmed[idx..])
}

fn extract_label_at_position(line: &str, position: Position) -> Option<(String, usize)> {
  let byte_index = utf16_position_to_byte_offset(line, position);
  let bytes = line.as_bytes();
  if byte_index > bytes.len() {
    return None;
  }
  let mut start = byte_index;
  while start > 0 && is_label_char(bytes[start - 1]) {
    start -= 1;
  }
  let mut end = byte_index;
  while end < bytes.len() && is_label_char(bytes[end]) {
    end += 1;
  }
  if start == end || !is_label_start(bytes[start]) {
    return None;
  }
  Some((line[start..end].to_string(), start))
}

fn find_label_definition(text: &str, label: &str) -> Option<(u32, usize, usize)> {
  for (line_idx, line) in text.lines().enumerate() {
    let line_before_comment = line.splitn(2, ';').next().unwrap_or("");
    let trimmed = line_before_comment.trim_start();
    if trimmed.is_empty() {
      continue;
    }
    let colon_idx = match trimmed.find(':') {
      Some(idx) => idx,
      None => continue,
    };
    let name = trimmed[..colon_idx].trim_end();
    if name.is_empty() || name != label {
      continue;
    }
    if !name
      .as_bytes()
      .iter()
      .enumerate()
      .all(|(i, &b)| if i == 0 { is_label_start(b) } else { is_label_char(b) })
    {
      continue;
    }
    let trimmed_start = line_before_comment.len() - trimmed.len();
    let start = trimmed_start;
    let end = start + name.len();
    return Some((line_idx as u32, start, end));
  }
  None
}
