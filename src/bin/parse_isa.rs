use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde::Serialize;
use std::collections::BTreeSet;
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Serialize, Clone)]
struct Operand {
  field_name: Option<String>,
  operand_type: Option<String>,
  data_format_name: Option<String>,
  size: Option<u32>,
  input: Option<bool>,
  output: Option<bool>,
  is_implicit: Option<bool>,
  order: Option<u32>,
}

#[derive(Debug, Default)]
struct InstructionEncoding {
  encoding_name: Option<String>,
  operands: Vec<Operand>,
}

#[derive(Debug, Default, Serialize)]
struct InstructionDoc {
  name: String,
  architectures: Vec<String>,
  description: Option<String>,
  args: Vec<String>,
  arg_types: Vec<String>,
  available_encodings: Vec<String>,
  #[serde(skip_serializing)]
  encodings: Vec<InstructionEncoding>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum TextTarget {
  InstructionName,
  ArchitectureName,
  Description,
  EncodingName,
  OperandFieldName,
  OperandType,
  OperandDataFormatName,
  OperandSize,
}

fn parse_bool(raw: &str) -> Option<bool> {
  match raw.to_ascii_lowercase().as_str() {
    "true" => Some(true),
    "false" => Some(false),
    _ => None,
  }
}

fn attr_value(attrs: &BytesStart<'_>, key: &[u8]) -> Option<String> {
  for attr in attrs.attributes().flatten() {
    if attr.key.as_ref() == key {
      if let Ok(value) = attr.unescape_value() {
        return Some(value.to_string());
      }
    }
  }
  None
}

fn parse_operand_attributes(attrs: &BytesStart<'_>) -> Operand {
  let mut operand = Operand::default();
  operand.input = attr_value(attrs, b"Input").as_deref().and_then(parse_bool);
  operand.output = attr_value(attrs, b"Output").as_deref().and_then(parse_bool);
  operand.is_implicit = attr_value(attrs, b"IsImplicit").as_deref().and_then(parse_bool);
  operand.order = attr_value(attrs, b"Order").and_then(|val| val.parse::<u32>().ok());
  operand
}

fn operand_label(operand: &Operand) -> Option<String> {
  if let Some(name) = &operand.field_name {
    return Some(name.clone());
  }
  if let Some(operand_type) = &operand.operand_type {
    return Some(operand_type.clone());
  }
  None
}

fn operand_kind(operand: &Operand) -> String {
  let operand_type = match &operand.operand_type {
    Some(value) => value.as_str(),
    None => return "unknown".to_string(),
  };
  if operand_type.starts_with("OPR_SIMM")
    || operand_type == "OPR_SMEM_OFFSET"
    || operand_type == "OPR_DELAY"
  {
    return "immediate".to_string();
  }
  if operand_type == "OPR_LABEL" {
    return "label".to_string();
  }
  if operand_type == "OPR_DSMEM" || operand_type == "OPR_FLAT_SCRATCH" {
    return "memory".to_string();
  }
  if matches!(
    operand_type,
    "OPR_VGPR"
      | "OPR_SREG"
      | "OPR_SDST"
      | "OPR_SSRC"
      | "OPR_SSRC_LANESEL"
      | "OPR_SSRC_SPECIAL_SCC"
      | "OPR_SRC"
      | "OPR_SRC_VGPR"
      | "OPR_SRC_VGPR_OR_INLINE"
      | "OPR_VCC"
      | "OPR_EXEC"
      | "OPR_SDST_EXEC"
      | "OPR_SDST_M0"
      | "OPR_SDST_NULL"
      | "OPR_PC"
      | "OPR_TGT"
  ) {
    if operand_type == "OPR_SRC_VGPR_OR_INLINE" {
      return "register_or_inline".to_string();
    }
    return "register".to_string();
  }
  if matches!(
    operand_type,
    "OPR_SENDMSG"
      | "OPR_SENDMSG_RTN"
      | "OPR_WAITCNT"
      | "OPR_WAITCNT_DEPCTR"
      | "OPR_WAIT_EVENT"
      | "OPR_HWREG"
      | "OPR_ATTR"
      | "OPR_VERSION"
      | "OPR_CLAUSE"
  ) {
    return "special".to_string();
  }
  "unknown".to_string()
}

fn build_args(encodings: &[InstructionEncoding]) -> (Vec<String>, Vec<String>) {
  if encodings.is_empty() {
    return (Vec::new(), Vec::new());
  }
  let mut args = Vec::new();
  let mut arg_types = Vec::new();
  let mut operands = encodings[0].operands.clone();
  operands.sort_by_key(|operand| operand.order.unwrap_or(u32::MAX));
  for operand in operands {
    if operand.is_implicit == Some(true) {
      continue;
    }
    let label = operand_label(&operand).unwrap_or_else(|| "operand".to_string());
    args.push(label);
    arg_types.push(operand_kind(&operand));
  }
  (args, arg_types)
}

fn parse_instruction_file(path: &Path) -> Result<(String, Vec<InstructionDoc>), Box<dyn Error>> {
  let file = fs::File::open(path)?;
  let mut reader = Reader::from_reader(std::io::BufReader::new(file));
  reader.config_mut().trim_text(true);

  let mut buf = Vec::new();
  let mut instructions: Vec<InstructionDoc> = Vec::new();
  let mut current_instruction: Option<InstructionDoc> = None;
  let mut current_encoding: Option<InstructionEncoding> = None;
  let mut current_operand: Option<Operand> = None;
  let mut text_target: Option<TextTarget> = None;
  let mut architecture_name: Option<String> = None;
  let mut in_aliased_names: bool = false;

  loop {
    match reader.read_event_into(&mut buf) {
      Ok(Event::Start(ref event)) => match event.local_name().as_ref() {
        b"Instruction" => {
          current_instruction = Some(InstructionDoc::default());
        }
        b"AliasedInstructionNames" => {
          in_aliased_names = true;
        }
        b"InstructionName" => {
          if !in_aliased_names {
            text_target = Some(TextTarget::InstructionName);
          }
        }
        b"ArchitectureName" => {
          text_target = Some(TextTarget::ArchitectureName);
        }
        b"Description" => {
          if current_instruction.is_some() {
            text_target = Some(TextTarget::Description);
          }
        }
        b"InstructionEncoding" => {
          current_encoding = Some(InstructionEncoding::default());
        }
        b"EncodingName" => {
          if current_encoding.is_some() {
            text_target = Some(TextTarget::EncodingName);
          }
        }
        b"Operand" => {
          current_operand = Some(parse_operand_attributes(event));
        }
        b"FieldName" => {
          text_target = Some(TextTarget::OperandFieldName);
        }
        b"OperandType" => {
          text_target = Some(TextTarget::OperandType);
        }
        b"DataFormatName" => {
          text_target = Some(TextTarget::OperandDataFormatName);
        }
        b"OperandSize" => {
          text_target = Some(TextTarget::OperandSize);
        }
        _ => {}
      },
      Ok(Event::End(ref event)) => match event.local_name().as_ref() {
        b"AliasedInstructionNames" => {
          in_aliased_names = false;
        }
        b"Instruction" => {
          if let Some(mut inst) = current_instruction.take() {
            let (args, arg_types) = build_args(&inst.encodings);
            inst.args = args;
            inst.arg_types = arg_types;
            // Collect unique encoding names
            let mut encodings_set = std::collections::BTreeSet::new();
            for enc in &inst.encodings {
              if let Some(name) = &enc.encoding_name {
                encodings_set.insert(name.clone());
              }
            }
            inst.available_encodings = encodings_set.into_iter().collect();
            if let Some(arch) = architecture_name.clone() {
              inst.architectures.push(arch);
            }
            instructions.push(inst);
          }
        }
        b"InstructionEncoding" => {
          if let (Some(inst), Some(enc)) = (&mut current_instruction, current_encoding.take()) {
            inst.encodings.push(enc);
          }
        }
        b"Operand" => {
          if let (Some(enc), Some(op)) = (&mut current_encoding, current_operand.take()) {
            enc.operands.push(op);
          }
        }
        b"InstructionName" | b"ArchitectureName" | b"Description"
        | b"EncodingName"
        | b"FieldName"
        | b"OperandType"
        | b"DataFormatName"
        | b"OperandSize" => {
          text_target = None;
        }
        _ => {}
      },
      Ok(Event::Text(event)) => {
        if let Some(target) = text_target {
          let text = event.unescape()?.to_string();
          match target {
            TextTarget::InstructionName => {
              if let Some(inst) = &mut current_instruction {
                inst.name = text;
              }
            }
            TextTarget::ArchitectureName => {
              if architecture_name.is_none() {
                architecture_name = Some(text);
              }
            }
            TextTarget::Description => {
              if let Some(inst) = &mut current_instruction {
                inst.description = Some(text);
              }
            }
            TextTarget::EncodingName => {
              if let Some(enc) = &mut current_encoding {
                enc.encoding_name = Some(text);
              }
            }
            TextTarget::OperandFieldName => {
              if let Some(op) = &mut current_operand {
                op.field_name = Some(text);
              }
            }
            TextTarget::OperandType => {
              if let Some(op) = &mut current_operand {
                op.operand_type = Some(text);
              }
            }
            TextTarget::OperandDataFormatName => {
              if let Some(op) = &mut current_operand {
                op.data_format_name = Some(text);
              }
            }
            TextTarget::OperandSize => {
              if let Some(op) = &mut current_operand {
                op.size = text.parse::<u32>().ok();
              }
            }
          }
        }
      }
      Ok(Event::Eof) => break,
      Err(err) => return Err(Box::new(err)),
      _ => {}
    }
    buf.clear();
  }

  Ok((architecture_name.unwrap_or_default(), instructions))
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

fn main() -> Result<(), Box<dyn Error>> {
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

  let mut xml_files = Vec::new();
  for input in &input_paths {
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
  if xml_files.is_empty() {
    eprintln!("No XML files found. Usage: parse_isa <xml...> [-o output.json]");
    std::process::exit(2);
  }

  let mut merged: Vec<InstructionDoc> = Vec::new();
  let mut seen = BTreeSet::new();
  for input in xml_files {
    let (architecture_name, mut instructions) = parse_instruction_file(&input)?;
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
    for inst in instructions {
      let key = format!(
        "{}|{}|{}|{}",
        inst.name,
        inst.description.clone().unwrap_or_default(),
        inst.args.join(","),
        inst.arg_types.join(",")
      );
      if seen.insert(key) {
        merged.push(inst);
      } else {
        if let Some(existing) = merged
          .iter_mut()
          .find(|existing| {
            existing.name == inst.name
              && existing.description == inst.description
              && existing.args == inst.args
              && existing.arg_types == inst.arg_types
          })
        {
          for arch in inst.architectures {
            if !existing.architectures.contains(&arch) {
              existing.architectures.push(arch);
            }
          }
        }
      }
    }
  }

  let json = serde_json::to_string_pretty(&merged)?;

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
