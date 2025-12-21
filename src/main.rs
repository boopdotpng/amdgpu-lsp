use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::sync::{Arc, Mutex};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
  Hover, HoverContents, HoverParams, HoverProviderCapability, InitializeParams, InitializeResult, MarkupContent,
  MarkupKind, MessageType, Position, ServerCapabilities, TextDocumentContentChangeEvent, TextDocumentItem,
  TextDocumentSyncCapability, TextDocumentSyncKind, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug, Clone, Deserialize)]
struct InstructionEntry {
  name: String,
  architectures: Vec<String>,
  description: Option<String>,
  args: Vec<String>,
  arg_types: Vec<String>,
}

#[derive(Default)]
struct DocumentStore {
  docs: HashMap<Url, DocumentState>,
}

#[derive(Debug, Clone)]
struct DocumentState {
  text: String,
  language_id: String,
}

struct IsaLoadInfo {
  data_path: String,
  load_error: Option<String>,
}

struct IsaServer {
  client: Client,
  docs: Arc<Mutex<DocumentStore>>,
  index: HashMap<String, Vec<InstructionEntry>>,
  architecture_override: Arc<Mutex<Option<String>>>,
  load_info: IsaLoadInfo,
}

impl IsaServer {
  fn new(client: Client, index: HashMap<String, Vec<InstructionEntry>>, load_info: IsaLoadInfo) -> Self {
    Self {
      client,
      docs: Arc::new(Mutex::new(DocumentStore::default())),
      index,
      architecture_override: Arc::new(Mutex::new(None)),
      load_info,
    }
  }

  fn get_document(&self, uri: &Url) -> Option<DocumentState> {
    self.docs.lock().ok()?.docs.get(uri).cloned()
  }
}

fn load_isa_index() -> (HashMap<String, Vec<InstructionEntry>>, IsaLoadInfo) {
  let data_path = env::var("RDNA_LSP_DATA").unwrap_or_else(|_| "data/isa.json".to_string());
  let contents = match fs::read_to_string(&data_path) {
    Ok(text) => text,
    Err(error) => {
      return (
        HashMap::new(),
        IsaLoadInfo {
          data_path,
          load_error: Some(format!("Failed to read isa.json: {error}")),
        },
      );
    }
  };
  let entries: Vec<InstructionEntry> = match serde_json::from_str(&contents) {
    Ok(parsed) => parsed,
    Err(error) => {
      return (
        HashMap::new(),
        IsaLoadInfo {
          data_path,
          load_error: Some(format!("Failed to parse isa.json: {error}")),
        },
      );
    }
  };
  let mut index: HashMap<String, Vec<InstructionEntry>> = HashMap::new();
  for entry in entries {
    index.entry(entry.name.to_ascii_lowercase()).or_default().push(entry);
  }
  (
    index,
    IsaLoadInfo {
      data_path,
      load_error: None,
    },
  )
}

fn utf16_position_to_byte_offset(line: &str, position: Position) -> usize {
  let mut utf16_count = 0;
  for (idx, ch) in line.char_indices() {
    if utf16_count >= position.character {
      return idx;
    }
    utf16_count += ch.len_utf16() as u32;
  }
  line.len()
}

fn extract_word_at_position(text: &str, position: Position) -> Option<String> {
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

fn normalize_architecture_hint(raw: &str) -> String {
  let cleaned = raw.trim().to_ascii_lowercase().replace(' ', "");
  if let Some(rem) = cleaned.strip_prefix("rdna") {
    if rem.len() == 2 && rem.chars().all(|ch| ch.is_ascii_digit()) {
      let (major, minor) = rem.split_at(1);
      return format!("rdna{major}.{minor}");
    }
  }
  cleaned
}

fn architecture_filter(language_id: &str, override_arch: Option<&String>) -> Option<String> {
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

fn entry_matches_arch(entry: &InstructionEntry, filter: &str) -> bool {
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

fn format_hover(entry: &InstructionEntry) -> HoverContents {
  let mut lines = Vec::new();
  lines.push(format!("**{}**", entry.name));
  if !entry.architectures.is_empty() {
    lines.push(format!("Architectures: {}", entry.architectures.join(", ")));
  }
  if !entry.args.is_empty() {
    let args = entry
      .args
      .iter()
      .zip(entry.arg_types.iter())
      .map(|(arg, arg_type)| format!("{arg} ({arg_type})"))
      .collect::<Vec<_>>()
      .join(", ");
    lines.push(format!("Args: {args}"));
  }
  if let Some(description) = &entry.description {
    if !description.is_empty() {
      lines.push(description.clone());
    }
  }
  HoverContents::Markup(MarkupContent {
    kind: MarkupKind::Markdown,
    value: lines.join("\n\n"),
  })
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
    let key = word.to_ascii_lowercase();
    let entries = match self.index.get(&key) {
      Some(entries) => entries,
      None => {
        self
          .client
          .log_message(MessageType::INFO, format!("hover: no entry for {word}"))
          .await;
        return Ok(None);
      }
    };
    let override_arch = self.architecture_override.lock().ok().and_then(|value| value.clone());
    if let Some(filter) = architecture_filter(&doc.language_id, override_arch.as_ref()) {
      if let Some(entry) = entries.iter().find(|entry| entry_matches_arch(entry, &filter)) {
        return Ok(Some(Hover {
          contents: format_hover(entry),
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
      contents: format_hover(&entries[0]),
      range: None,
    }))
  }

  async fn shutdown(&self) -> Result<()> {
    Ok(())
  }
}

#[tokio::main]
async fn main() {
  let (index, load_info) = load_isa_index();
  let stdin = tokio::io::stdin();
  let stdout = tokio::io::stdout();
  let (service, socket) = LspService::new(|client| IsaServer::new(client, index, load_info));
  Server::new(stdin, stdout, socket).serve(service).await;
}
