use crate::types::{InstructionEntry, IsaData, IsaLoadInfo, SpecialRegister, SpecialRegistersData};
use std::collections::HashMap;
use std::env;
use std::fs;

pub fn load_isa_index() -> (
  HashMap<String, Vec<InstructionEntry>>,
  Vec<SpecialRegister>,
  IsaLoadInfo,
) {
  let data_path = env::var("AMDGPU_LSP_DATA").unwrap_or_else(|_| "data/isa.json".to_string());
  let contents = match fs::read_to_string(&data_path) {
    Ok(text) => text,
    Err(error) => {
      return (
        HashMap::new(),
        Vec::new(),
        IsaLoadInfo {
          data_path,
          load_error: Some(format!("Failed to read isa.json: {error}")),
        },
      );
    }
  };
  let isa_data: IsaData = match serde_json::from_str(&contents) {
    Ok(parsed) => parsed,
    Err(error) => {
      return (
        HashMap::new(),
        Vec::new(),
        IsaLoadInfo {
          data_path,
          load_error: Some(format!("Failed to parse isa.json: {error}")),
        },
      );
    }
  };
  let mut index: HashMap<String, Vec<InstructionEntry>> = HashMap::new();
  for entry in isa_data.instructions {
    index
      .entry(entry.name.to_ascii_lowercase())
      .or_default()
      .push(entry);
  }

  let mut special_registers: Vec<SpecialRegister> = match isa_data.special_registers {
    SpecialRegistersData::Flat(list) => list,
    SpecialRegistersData::Compressed(data) => {
      let mut expanded = data.singles;
      for range in data.ranges {
        expanded.extend(range.expand());
      }
      expanded
    }
  };
  // Keep stable ordering for predictable output and lookups.
  special_registers.sort_by(|a, b| a.name.cmp(&b.name));

  (
    index,
    special_registers,
    IsaLoadInfo {
      data_path,
      load_error: None,
    },
  )
}
