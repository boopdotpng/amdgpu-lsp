mod instructions;
mod model;
mod operand;
mod special_registers;

use crate::instructions::parse_instruction_file;
use crate::model::{InstructionDoc, IsaOutput, SpecialRegister};
use crate::special_registers::{
  compress_special_registers, is_ignored_special_register, normalize_special_register, parse_special_registers,
};
use std::collections::{BTreeMap, HashMap};
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

fn parse_args() -> (Vec<PathBuf>, Option<PathBuf>) {
  let args: Vec<String> = env::args().collect();
  let mut input_paths = Vec::new();
  let mut output = None;
  let mut idx = 1;
  while idx < args.len() {
    if args[idx] == "-o" || args[idx] == "--output" {
      if let Some(path) = args.get(idx + 1) {
        output = Some(PathBuf::from(path));
      }
      idx += 2;
      continue;
    }
    input_paths.push(PathBuf::from(&args[idx]));
    idx += 1;
  }
  if input_paths.is_empty() {
    input_paths.push(PathBuf::from("amd_gpu_xmls"));
    output = Some(PathBuf::from("data/isa.json"));
  }
  (input_paths, output)
}

fn normalize_architecture_name(raw: &str) -> String {
  let lower = raw.trim().to_ascii_lowercase();
  let tokens: Vec<&str> = lower.split_whitespace().collect();
  let mut family: Option<&str> = None;
  let mut version: Option<String> = None;
  for token in &tokens {
    if token.contains("rdna") {
      family = Some("rdna");
      if let Some(remainder) = token.strip_prefix("rdna") {
        if !remainder.is_empty() {
          version = Some(remainder.to_string());
        }
      }
      continue;
    }
    if token.contains("cdna") {
      family = Some("cdna");
      if let Some(remainder) = token.strip_prefix("cdna") {
        if !remainder.is_empty() {
          version = Some(remainder.to_string());
        }
      }
      continue;
    }
    if family.is_some() && version.is_none() && token.chars().any(|ch| ch.is_ascii_digit()) {
      version = Some(token.to_string());
    }
  }
  if let Some(family) = family {
    if let Some(version) = version {
      return format!("{family}{version}");
    }
    return family.to_string();
  }
  lower.replace(' ', "")
}

fn collect_xml_files(inputs: &[PathBuf]) -> Result<Vec<PathBuf>, Box<dyn Error>> {
  let mut xml_files = Vec::new();
  for input in inputs {
    if input.is_dir() {
      for entry in fs::read_dir(input)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("xml") {
          xml_files.push(path);
        }
      }
    } else {
      xml_files.push(input.clone());
    }
  }
  Ok(xml_files)
}

fn is_rdna_source(path: &Path) -> bool {
  path
    .file_name()
    .and_then(|name| name.to_str())
    .map(|name| name.contains("rdna"))
    .unwrap_or(false)
}

fn merge_instructions(
  merged: &mut Vec<InstructionDoc>,
  key_to_index: &mut HashMap<String, usize>,
  instructions: Vec<InstructionDoc>,
) {
  for inst in instructions {
    let key = format!(
      "{}|{}|{}|{}",
      inst.name,
      inst.description.clone().unwrap_or_default(),
      inst.args.join(","),
      inst.arg_types.join(",")
    );
    if let Some(&index) = key_to_index.get(&key) {
      let existing = &mut merged[index];
      for arch in inst.architectures {
        if !existing.architectures.contains(&arch) {
          existing.architectures.push(arch);
        }
      }
    } else {
      key_to_index.insert(key, merged.len());
      merged.push(inst);
    }
  }
}

fn main() -> Result<(), Box<dyn Error>> {
  let (input_paths, output) = parse_args();
  let xml_files = collect_xml_files(&input_paths)?;
  if xml_files.is_empty() {
    eprintln!("No XML files found. Usage: parse_isa <xml...> [-o output.json]");
    std::process::exit(2);
  }

  let mut merged: Vec<InstructionDoc> = Vec::new();
  let mut key_to_index: HashMap<String, usize> = HashMap::new();
  let mut special_registers_by_name: BTreeMap<String, SpecialRegister> = BTreeMap::new();

  for input in &xml_files {
    let (architecture_name, mut instructions) = parse_instruction_file(input)?;
    let normalized_architecture = normalize_architecture_name(&architecture_name);
    for inst in &mut instructions {
      if inst.architectures.is_empty() {
        inst.architectures.push(normalized_architecture.clone());
      } else {
        inst.architectures = inst
          .architectures
          .iter()
          .map(|arch| normalize_architecture_name(arch))
          .collect();
      }
    }
    merge_instructions(&mut merged, &mut key_to_index, instructions);

    if is_rdna_source(input) {
      if let Ok(registers) = parse_special_registers(input) {
        for reg in registers {
          let name_lower = reg.name.to_ascii_lowercase();
          if is_ignored_special_register(&name_lower) {
            continue;
          }
          let reg = normalize_special_register(reg);
          let key = reg.name.to_ascii_lowercase();
          if let Some(existing) = special_registers_by_name.get_mut(&key) {
            let SpecialRegister { description, .. } = reg;
            if let Some(description) = description {
              let should_replace = match &existing.description {
                Some(current) => description.len() > current.len(),
                None => true,
              };
              if should_replace {
                existing.description = Some(description);
              }
            }
          } else {
            special_registers_by_name.insert(key, reg);
          }
        }
      }
    }
  }

  let mut all_special_registers: Vec<SpecialRegister> = special_registers_by_name.into_values().collect();
  all_special_registers.sort_by(|a, b| a.name.cmp(&b.name));

  let isa_output = IsaOutput {
    instructions: merged,
    special_registers: compress_special_registers(all_special_registers),
  };
  let json = serde_json::to_string_pretty(&isa_output)?;

  if let Some(output_path) = output {
    if let Some(parent) = output_path.parent() {
      if !parent.as_os_str().is_empty() {
        fs::create_dir_all(parent)?;
      }
    }
    fs::write(output_path, json + "\n")?;
  } else {
    println!("{json}");
  }

  Ok(())
}
