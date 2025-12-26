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
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpecialRegisterRangeOverride {
  /// Numeric suffix value (e.g. 0 for "ttmp0")
  pub index: u32,
  pub description: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpecialRegisterRange {
  /// Name prefix (e.g. "param" -> param0, param1, ...)
  pub prefix: String,
  /// Starting numeric suffix (inclusive).
  pub start: u32,
  /// Number of entries in the range.
  pub count: u32,
  pub description: Option<String>,
  #[serde(default)]
  pub overrides: Vec<SpecialRegisterRangeOverride>,
}

impl SpecialRegisterRange {
  pub fn expand(&self) -> Vec<SpecialRegister> {
    let mut overrides_by_index: HashMap<u32, &SpecialRegisterRangeOverride> = HashMap::new();
    for ov in &self.overrides {
      overrides_by_index.insert(ov.index, ov);
    }
    let mut out = Vec::with_capacity(self.count as usize);
    for offset in 0..self.count {
      let idx = self.start + offset;
      let mut reg = SpecialRegister {
        name: format!("{}{}", self.prefix, idx),
        description: self.description.clone(),
      };
      if let Some(ov) = overrides_by_index.get(&idx) {
        if ov.description.is_some() {
          reg.description = ov.description.clone();
        }
      }
      out.push(reg);
    }
    out
  }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SpecialRegistersCompressed {
  pub singles: Vec<SpecialRegister>,
  pub ranges: Vec<SpecialRegisterRange>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SpecialRegistersData {
  Flat(Vec<SpecialRegister>),
  Compressed(SpecialRegistersCompressed),
}

#[derive(Debug, Clone, Deserialize)]
pub struct IsaData {
  pub instructions: Vec<InstructionEntry>,
  pub special_registers: SpecialRegistersData,
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
