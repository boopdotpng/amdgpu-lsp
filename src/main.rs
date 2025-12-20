use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::sync::{Arc, Mutex};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
  Hover, HoverContents, HoverParams, InitializeParams, InitializeResult, MarkupContent, MarkupKind, Position,
  TextDocumentContentChangeEvent, TextDocumentItem, Url,
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
  docs: HashMap<Url, String>,
}

struct IsaServer {
  client: Client,
  docs: Arc<Mutex<DocumentStore>>,
  index: HashMap<String, InstructionEntry>,
}

impl IsaServer {
  fn new(client: Client, index: HashMap<String, InstructionEntry>) -> Self {
    Self {
      client,
      docs: Arc::new(Mutex::new(DocumentStore::default())),
      index,
    }
  }

  fn get_text(&self, uri: &Url) -> Option<String> {
    self.docs.lock().ok()?.docs.get(uri).cloned()
  }
}

fn load_isa_index() -> HashMap<String, InstructionEntry> {
  let data_path = env::var("RDNA_LSP_DATA").unwrap_or_else(|_| "data/isa.json".to_string());
  let contents = match fs::read_to_string(&data_path) {
    Ok(text) => text,
    Err(_) => return HashMap::new(),
  };
  let entries: Vec<InstructionEntry> = match serde_json::from_str(&contents) {
    Ok(parsed) => parsed,
    Err(_) => return HashMap::new(),
  };
  let mut index = HashMap::new();
  for entry in entries {
    index.insert(entry.name.to_ascii_lowercase(), entry);
  }
  index
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
  async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
    Ok(InitializeResult::default())
  }

  async fn did_open(&self, params: tower_lsp::lsp_types::DidOpenTextDocumentParams) {
    let TextDocumentItem { uri, text, .. } = params.text_document;
    if let Ok(mut store) = self.docs.lock() {
      store.docs.insert(uri, text);
    }
  }

  async fn did_change(&self, params: tower_lsp::lsp_types::DidChangeTextDocumentParams) {
    if let Some(TextDocumentContentChangeEvent { text, .. }) = params.content_changes.into_iter().last() {
      if let Ok(mut store) = self.docs.lock() {
        store.docs.insert(params.text_document.uri, text);
      }
    }
  }

  async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    let text = match self.get_text(&uri) {
      Some(text) => text,
      None => return Ok(None),
    };
    let word = match extract_word_at_position(&text, position) {
      Some(word) => word,
      None => return Ok(None),
    };
    let key = word.to_ascii_lowercase();
    let entry = match self.index.get(&key) {
      Some(entry) => entry,
      None => return Ok(None),
    };
    Ok(Some(Hover {
      contents: format_hover(entry),
      range: None,
    }))
  }
}

#[tokio::main]
async fn main() {
  let index = load_isa_index();
  let stdin = tokio::io::stdin();
  let stdout = tokio::io::stdout();
  let (service, socket) = LspService::new(|client| IsaServer::new(client, index));
  Server::new(stdin, stdout, socket).serve(service).await;
}
