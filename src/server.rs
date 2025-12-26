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
          trigger_characters: None,
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
    self
      .client
      .log_message(
        MessageType::INFO,
        format!("didOpen: {} (language: {}, len: {})", uri, language_id, text.len()),
      )
      .await;
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
      if let Some(len) = new_len {
        self
          .client
          .log_message(MessageType::INFO, format!("didChange: {} (len: {})", uri, len))
          .await;
      }
    }
  }

  async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    self
      .client
      .log_message(
        MessageType::INFO,
        format!("hover request: {} @ {}:{}", uri, position.line, position.character),
      )
      .await;
    let doc = match self.get_document(&uri) {
      Some(doc) => doc,
      None => {
        self
          .client
          .log_message(MessageType::WARNING, format!("hover: no document for {}", uri))
          .await;
        return Ok(None);
      }
    };
    let word = match extract_word_at_position(&doc.text, position) {
      Some(word) => word,
      None => {
        self
          .client
          .log_message(MessageType::INFO, "hover: no word at position".to_string())
          .await;
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
      None => {
        self
          .client
          .log_message(MessageType::INFO, format!("hover: no entry for {word} (base: {})", split.base))
          .await;
        return Ok(None);
      }
    };
    let override_arch = self.architecture_override.lock().ok().and_then(|value| value.clone());
    if let Some(filter) = architecture_filter(&doc.language_id, override_arch.as_ref()) {
      if let Some(entry) = entries.iter().find(|entry| entry_matches_arch(entry, &filter)) {
        return Ok(Some(Hover {
          contents: format_hover(entry, &split.variant),
          range: None,
        }));
      }
      if entries.len() > 1 {
        self
          .client
          .log_message(
            MessageType::INFO,
            format!("hover: entry {word} filtered out by architecture {filter}"),
          )
          .await;
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
    self
      .client
      .log_message(
        MessageType::INFO,
        format!("signature_help request: {} @ {}:{}", uri, position.line, position.character),
      )
      .await;

    let doc = match self.get_document(&uri) {
      Some(doc) => doc,
      None => {
        self
          .client
          .log_message(MessageType::WARNING, format!("signature_help: no document for {}", uri))
          .await;
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

    // Find the instruction at the start of the line (before any spaces/commas)
    let instruction = line
      .trim_start()
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
        self
          .client
          .log_message(
            MessageType::INFO,
            format!("signature_help: no entry for {} (base: {})", instruction, split.base),
          )
          .await;
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

    let line_before_cursor = &line[..cursor_byte.min(line.len())];
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

    let prefix_lower = trimmed_prefix.to_ascii_lowercase();
    let start_char = byte_offset_to_utf16_position(line, prefix_start);
    let start = Position {
      line: position.line,
      character: start_char,
    };
    let range = Range { start, end: position };

    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::new();
    for (name, entries) in &self.index {
      if !name.starts_with(&prefix_lower) {
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
      is_incomplete: false,
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
