use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::sync::{Arc, Mutex};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
  CompletionItem, CompletionItemKind, CompletionList, CompletionOptions, CompletionParams, CompletionResponse,
  CompletionTextEdit, Hover, HoverContents, HoverParams, HoverProviderCapability, InitializeParams, InitializeResult,
  MarkupContent, MarkupKind, MessageType, ParameterInformation, ParameterLabel, Position, Range, ServerCapabilities,
  SignatureHelp,
  SignatureHelpOptions, SignatureHelpParams, SignatureInformation, TextDocumentContentChangeEvent, TextDocumentItem,
  TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Url,
};
use tower_lsp::{Client, LanguageServer, LspService, Server};

#[derive(Debug, Clone, Deserialize)]
struct InstructionEntry {
  name: String,
  architectures: Vec<String>,
  description: Option<String>,
  args: Vec<String>,
  arg_types: Vec<String>,
  available_encodings: Vec<String>,
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

fn byte_offset_to_utf16_position(line: &str, byte_offset: usize) -> u32 {
  let mut utf16_count = 0;
  for (idx, ch) in line.char_indices() {
    if idx >= byte_offset {
      break;
    }
    utf16_count += ch.len_utf16() as u32;
  }
  utf16_count
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

fn extract_word_prefix_at_position(text: &str, position: Position) -> Option<(String, usize)> {
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum EncodingVariant {
  Native,
  E32,
  E64,
  Dpp,
  Sdwa,
  E64Dpp,
}

impl EncodingVariant {
  fn label(&self) -> Option<&'static str> {
    match self {
      EncodingVariant::Native => None,
      EncodingVariant::E32 => Some("VOP1/VOP2/VOPC (32-bit)"),
      EncodingVariant::E64 => Some("VOP3 (64-bit)"),
      EncodingVariant::Dpp => Some("DPP"),
      EncodingVariant::Sdwa => Some("SDWA"),
      EncodingVariant::E64Dpp => Some("VOP3 DPP"),
    }
  }
}

struct SplitInstruction {
  base: String,
  variant: EncodingVariant,
}

fn split_encoding_variant(mnemonic: &str) -> SplitInstruction {
  // Order matters: check longer suffixes first to avoid partial matches
  const SUFFIXES: &[(&str, EncodingVariant)] = &[
    ("_e64_dpp", EncodingVariant::E64Dpp),
    ("_e32", EncodingVariant::E32),
    ("_e64", EncodingVariant::E64),
    ("_dpp", EncodingVariant::Dpp),
    ("_sdwa", EncodingVariant::Sdwa),
  ];

  let mnemonic_lower = mnemonic.to_ascii_lowercase();
  for (suffix, variant) in SUFFIXES {
    if mnemonic_lower.ends_with(suffix) {
      return SplitInstruction {
        base: mnemonic[..mnemonic.len() - suffix.len()].to_string(),
        variant: variant.clone(),
      };
    }
  }

  SplitInstruction {
    base: mnemonic.to_string(),
    variant: EncodingVariant::Native,
  }
}

fn get_encoding_description(encoding_name: &str) -> Option<&'static str> {
  match encoding_name {
    // Standard encodings
    "ENC_VOP1" => Some("VOP1 (32-bit): Vector ALU operation with one source"),
    "ENC_VOP2" => Some("VOP2 (32-bit): Vector ALU operation with two sources"),
    "ENC_VOPC" => Some("VOPC (32-bit): Vector ALU comparison operation"),
    "ENC_VOP3" => Some("VOP3 (64-bit): Extended vector ALU with modifiers and additional operand flexibility"),
    "ENC_VOP3P" => Some("VOP3P (64-bit): Packed vector ALU operation"),

    // DPP encodings
    "VOP1_VOP_DPP" | "VOP1_VOP_DPP16" => Some("VOP1 + DPP16: Data-parallel primitives with 16-lane swizzle"),
    "VOP1_VOP_DPP8" => Some("VOP1 + DPP8: Data-parallel primitives with 8-lane swizzle"),
    "VOP2_VOP_DPP" | "VOP2_VOP_DPP16" => Some("VOP2 + DPP16: Data-parallel primitives with 16-lane swizzle"),
    "VOP2_VOP_DPP8" => Some("VOP2 + DPP8: Data-parallel primitives with 8-lane swizzle"),
    "VOPC_VOP_DPP" | "VOPC_VOP_DPP16" => Some("VOPC + DPP16: Comparison with data-parallel primitives (16-lane)"),
    "VOPC_VOP_DPP8" => Some("VOPC + DPP8: Comparison with data-parallel primitives (8-lane)"),
    "VOP3_VOP_DPP16" => Some("VOP3 + DPP16: Extended VOP3 with data-parallel primitives (16-lane)"),
    "VOP3_VOP_DPP8" => Some("VOP3 + DPP8: Extended VOP3 with data-parallel primitives (8-lane)"),
    "VOP3P_VOP_DPP16" => Some("VOP3P + DPP16: Packed operation with data-parallel primitives (16-lane)"),
    "VOP3P_VOP_DPP8" => Some("VOP3P + DPP8: Packed operation with data-parallel primitives (8-lane)"),
    "VOP3_SDST_ENC_VOP_DPP16" => Some("VOP3 SDST + DPP16: VOP3 with scalar destination and DPP (16-lane)"),
    "VOP3_SDST_ENC_VOP_DPP8" => Some("VOP3 SDST + DPP8: VOP3 with scalar destination and DPP (8-lane)"),

    // SDWA encodings
    "VOP1_VOP_SDWA" => Some("VOP1 + SDWA: Sub-DWORD addressing for byte/word operations"),
    "VOP2_VOP_SDWA" => Some("VOP2 + SDWA: Sub-DWORD addressing for byte/word operations"),
    "VOPC_VOP_SDWA" => Some("VOPC + SDWA: Comparison with sub-DWORD addressing"),

    // Literal encodings
    "VOP1_INST_LITERAL" => Some("VOP1 + Literal (64-bit): Includes 32-bit inline constant"),
    "VOP2_INST_LITERAL" => Some("VOP2 + Literal (64-bit): Includes 32-bit inline constant"),
    "VOPC_INST_LITERAL" => Some("VOPC + Literal (64-bit): Includes 32-bit inline constant"),
    "VOP3_INST_LITERAL" => Some("VOP3 + Literal (96-bit): VOP3 with 32-bit inline constant"),
    "VOP3P_INST_LITERAL" => Some("VOP3P + Literal (96-bit): Packed operation with 32-bit inline constant"),
    "VOP3_SDST_ENC_INST_LITERAL" => Some("VOP3 SDST + Literal (96-bit): VOP3 with scalar destination and literal"),

    // Special VOP3 variants
    "VOP3_SDST_ENC" => Some("VOP3 SDST (64-bit): VOP3 with scalar destination"),

    // Scalar encodings
    "ENC_SOP1" => Some("SOP1 (32-bit): Scalar ALU operation with one source"),
    "ENC_SOP2" => Some("SOP2 (32-bit): Scalar ALU operation with two sources"),
    "ENC_SOPC" => Some("SOPC (32-bit): Scalar ALU comparison operation"),
    "ENC_SOPK" => Some("SOPK (32-bit): Scalar operation with 16-bit inline constant"),
    "ENC_SOPP" => Some("SOPP (32-bit): Scalar operation for program control"),
    "SOP1_INST_LITERAL" => Some("SOP1 + Literal (64-bit): Scalar operation with 32-bit inline constant"),
    "SOP2_INST_LITERAL" => Some("SOP2 + Literal (64-bit): Scalar operation with 32-bit inline constant"),
    "SOPC_INST_LITERAL" => Some("SOPC + Literal (64-bit): Scalar comparison with 32-bit inline constant"),
    "SOPK_INST_LITERAL" => Some("SOPK + Literal (64-bit): Scalar operation with extended constant"),

    // Memory encodings
    "ENC_SMEM" => Some("SMEM: Scalar memory operation"),
    "ENC_DS" => Some("DS: Data share (LDS/GDS) operation"),
    "ENC_MUBUF" => Some("MUBUF: Untyped buffer memory operation"),
    "ENC_MTBUF" => Some("MTBUF: Typed buffer memory operation"),
    "ENC_MIMG" => Some("MIMG: Image memory operation"),
    "MIMG_NSA1" => Some("MIMG NSA: Non-sequential address mode for images"),
    "ENC_FLAT" => Some("FLAT: Flat addressing (global/scratch/LDS)"),
    "ENC_FLAT_SCRATCH" => Some("FLAT Scratch: Flat addressing for scratch memory"),
    "ENC_FLAT_GLOBAL" => Some("FLAT Global: Flat addressing for global memory"),

    // Interpolation and other
    "ENC_VINTERP" => Some("VINTERP: Vector interpolation operation"),
    "ENC_LDSDIR" => Some("LDSDIR: LDS direct read operation"),
    "ENC_EXP" => Some("EXP: Export operation for pixel/vertex data"),
    "VOPDXY" => Some("VOPDXY: Vector operation with partial derivatives"),
    "VOPDXY_INST_LITERAL" => Some("VOPDXY + Literal: Vector partial derivative with inline constant"),

    _ => None,
  }
}

fn find_matching_encoding(available_encodings: &[String], variant: &EncodingVariant) -> Option<String> {
  // Map LLVM suffix variants to potential encoding name patterns
  match variant {
    EncodingVariant::Native => {
      // For native (no suffix), prefer the base encoding (ENC_VOP1/2/3, etc.)
      available_encodings
        .iter()
        .find(|enc| enc.starts_with("ENC_") && !enc.contains("LITERAL"))
        .cloned()
    }
    EncodingVariant::E32 => {
      // _e32 maps to base VOP1/VOP2/VOPC encodings
      available_encodings
        .iter()
        .find(|enc| matches!(enc.as_str(), "ENC_VOP1" | "ENC_VOP2" | "ENC_VOPC"))
        .cloned()
    }
    EncodingVariant::E64 => {
      // _e64 maps to VOP3 encoding
      available_encodings
        .iter()
        .find(|enc| enc.as_str() == "ENC_VOP3")
        .cloned()
    }
    EncodingVariant::Dpp => {
      // _dpp maps to DPP encodings (prefer DPP16 over DPP8)
      available_encodings
        .iter()
        .find(|enc| enc.contains("DPP16") || enc.contains("DPP"))
        .cloned()
    }
    EncodingVariant::Sdwa => {
      // _sdwa maps to SDWA encodings
      available_encodings
        .iter()
        .find(|enc| enc.contains("SDWA"))
        .cloned()
    }
    EncodingVariant::E64Dpp => {
      // _e64_dpp maps to VOP3 + DPP encodings
      available_encodings
        .iter()
        .find(|enc| enc.starts_with("VOP3") && (enc.contains("DPP16") || enc.contains("DPP")))
        .cloned()
    }
  }
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

fn format_architectures(architectures: &[String]) -> String {
  // Define all known architectures (including older generations)
  const ALL_ARCHS: &[&str] = &[
    "rdna1", "rdna2", "rdna3", "rdna3.5", "rdna4",
    "cdna1", "cdna2", "cdna3", "cdna4"
  ];

  // Check if all architectures are present
  if architectures.len() >= ALL_ARCHS.len() {
    let has_all = ALL_ARCHS.iter().all(|arch| architectures.contains(&arch.to_string()));
    if has_all {
      return "all".to_string();
    }
  }

  // Separate rdna and cdna versions
  let mut rdna_versions: Vec<&str> = architectures
    .iter()
    .filter_map(|arch| arch.strip_prefix("rdna"))
    .collect();

  let mut cdna_versions: Vec<&str> = architectures
    .iter()
    .filter_map(|arch| arch.strip_prefix("cdna"))
    .collect();

  // Collect any other architectures that don't match rdna/cdna
  let other_archs: Vec<&String> = architectures
    .iter()
    .filter(|arch| !arch.starts_with("rdna") && !arch.starts_with("cdna"))
    .collect();

  let mut parts = Vec::new();

  // Format rdna versions if any
  if !rdna_versions.is_empty() {
    rdna_versions.sort_by(|a, b| {
      let a_num: f32 = a.parse().unwrap_or(0.0);
      let b_num: f32 = b.parse().unwrap_or(0.0);
      a_num.partial_cmp(&b_num).unwrap_or(std::cmp::Ordering::Equal)
    });
    if rdna_versions.len() > 1 {
      parts.push(format!("rdna{{{}}}", rdna_versions.join(" | ")));
    } else {
      parts.push(format!("rdna{}", rdna_versions[0]));
    }
  }

  // Format cdna versions if any
  if !cdna_versions.is_empty() {
    cdna_versions.sort_by(|a, b| {
      let a_num: f32 = a.parse().unwrap_or(0.0);
      let b_num: f32 = b.parse().unwrap_or(0.0);
      a_num.partial_cmp(&b_num).unwrap_or(std::cmp::Ordering::Equal)
    });
    if cdna_versions.len() > 1 {
      parts.push(format!("cdna{{{}}}", cdna_versions.join(" | ")));
    } else {
      parts.push(format!("cdna{}", cdna_versions[0]));
    }
  }

  // Add any other architectures
  for arch in other_archs {
    parts.push(arch.clone());
  }

  parts.join(" | ")
}

fn format_hover(entry: &InstructionEntry, variant: &EncodingVariant) -> HoverContents {
  let mut lines = Vec::new();
  lines.push(format!("**{}**", entry.name));

  if !entry.architectures.is_empty() {
    lines.push(format!("Architectures: {}", format_architectures(&entry.architectures)));
  }
  if !entry.args.is_empty() {
    let args = entry
      .args
      .iter()
      .zip(entry.arg_types.iter())
      .map(|(arg, arg_type)| {
        let compact_type = arg_type.replace("register", "reg");
        format!("{arg} ({compact_type})")
      })
      .collect::<Vec<_>>()
      .join(", ");
    lines.push(format!("Args: {args}"));
  }
  if let Some(description) = &entry.description {
    if !description.is_empty() {
      lines.push(description.clone());
    }
  }

  // Try to find the matching encoding from the instruction's available encodings (show last)
  if let Some(encoding_name) = find_matching_encoding(&entry.available_encodings, variant) {
    if let Some(desc) = get_encoding_description(&encoding_name) {
      lines.push(format!("Encoding: {}", desc));
    } else {
      // Fallback: show the encoding name if we don't have a description
      lines.push(format!("Encoding: {}", encoding_name));
    }
  } else if let Some(fallback_label) = variant.label() {
    // If we can't find a matching encoding, use the simple label
    lines.push(format!("Encoding: {}", fallback_label));
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
        signature_help_provider: Some(SignatureHelpOptions {
          trigger_characters: Some(vec![" ".to_string()]),
          retrigger_characters: None,
          work_done_progress_options: Default::default(),
        }),
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

    // Build signature with parameter information
    let mut label = entry.name.clone();
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
      active_parameter: None,
    };

    Ok(Some(SignatureHelp {
      signatures: vec![signature],
      active_signature: Some(0),
      active_parameter: None,
    }))
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
        if seen.insert(entry.name.clone()) {
          items.push(CompletionItem {
            label: entry.name.clone(),
            kind: Some(CompletionItemKind::KEYWORD),
            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
              range: range.clone(),
              new_text: entry.name.clone(),
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

#[tokio::main]
async fn main() {
  let (index, load_info) = load_isa_index();
  let stdin = tokio::io::stdin();
  let stdout = tokio::io::stdout();
  let (service, socket) = LspService::new(|client| IsaServer::new(client, index, load_info));
  Server::new(stdin, stdout, socket).serve(service).await;
}
