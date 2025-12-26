use serde::Serialize;

#[derive(Debug, Default, Serialize, Clone)]
pub struct Operand {
  pub field_name: Option<String>,
  pub operand_type: Option<String>,
  pub data_format_name: Option<String>,
  pub size: Option<u32>,
  pub input: Option<bool>,
  pub output: Option<bool>,
  pub is_implicit: Option<bool>,
  pub order: Option<u32>,
}

#[derive(Debug, Default, Clone)]
pub struct InstructionEncoding {
  pub encoding_name: Option<String>,
  pub operands: Vec<Operand>,
}

#[derive(Debug, Default, Serialize)]
pub struct InstructionDoc {
  pub name: String,
  pub architectures: Vec<String>,
  pub description: Option<String>,
  pub args: Vec<String>,
  pub arg_types: Vec<String>,
  pub arg_data_types: Vec<String>,
  pub available_encodings: Vec<String>,
  #[serde(skip_serializing)]
  pub encodings: Vec<InstructionEncoding>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SpecialRegister {
  pub name: String,
  pub description: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SpecialRegisterRangeOverride {
  pub index: u32,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub description: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct SpecialRegisterRange {
  pub prefix: String,
  pub start: u32,
  pub count: u32,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub description: Option<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub overrides: Vec<SpecialRegisterRangeOverride>,
}

#[derive(Debug, Default, Serialize)]
pub struct SpecialRegistersOutput {
  pub singles: Vec<SpecialRegister>,
  pub ranges: Vec<SpecialRegisterRange>,
}

#[derive(Debug, Default, Serialize)]
pub struct IsaOutput {
  pub instructions: Vec<InstructionDoc>,
  pub special_registers: SpecialRegistersOutput,
}
