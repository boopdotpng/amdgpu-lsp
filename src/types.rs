use serde::Deserialize;
use std::collections::HashMap;
use tower_lsp::lsp_types::Url;

#[derive(Debug, Clone, Deserialize)]
pub struct InstructionEntry {
  pub name: String,
  pub architectures: Vec<String>,
  pub description: Option<String>,
  pub args: Vec<String>,
  pub arg_types: Vec<String>,
  pub arg_data_types: Vec<String>,
  pub available_encodings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpecialRegister {
  pub name: String,
  pub description: Option<String>,
  pub value: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IsaData {
  pub instructions: Vec<InstructionEntry>,
  pub special_registers: Vec<SpecialRegister>,
}

#[derive(Default)]
pub struct DocumentStore {
  pub docs: HashMap<Url, DocumentState>,
}

#[derive(Debug, Clone)]
pub struct DocumentState {
  pub text: String,
  pub language_id: String,
}

pub struct IsaLoadInfo {
  pub data_path: String,
  pub load_error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EncodingVariant {
  Native,
  E32,
  E64,
  Dpp,
  Sdwa,
  E64Dpp,
}

impl EncodingVariant {
}

pub struct SplitInstruction {
  pub base: String,
  pub variant: EncodingVariant,
}
