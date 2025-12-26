use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
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
  arg_data_types: Vec<String>,
  available_encodings: Vec<String>,
  #[serde(skip_serializing)]
  encodings: Vec<InstructionEncoding>,
}

#[derive(Debug, Serialize, Clone)]
struct SpecialRegister {
  name: String,
  description: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct SpecialRegisterRangeOverride {
  index: u32,
  #[serde(skip_serializing_if = "Option::is_none")]
  description: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
struct SpecialRegisterRange {
  prefix: String,
  start: u32,
  count: u32,
  #[serde(skip_serializing_if = "Option::is_none")]
  description: Option<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  overrides: Vec<SpecialRegisterRangeOverride>,
}

#[derive(Debug, Default, Serialize)]
struct SpecialRegistersOutput {
  singles: Vec<SpecialRegister>,
  ranges: Vec<SpecialRegisterRange>,
}

#[derive(Debug, Default, Serialize)]
struct IsaOutput {
  instructions: Vec<InstructionDoc>,
  special_registers: SpecialRegistersOutput,
}

fn is_see_above(desc: &str) -> bool {
  desc.trim() == "<p>See above.</p>" || desc.trim().eq_ignore_ascii_case("see above")
}

fn is_numeric_literal(name: &str) -> bool {
  name.parse::<f64>().is_ok()
}

fn special_register_override(name: &str) -> Option<&'static str> {
  match name {
    "exec" => Some("Wavefront execution mask (64-bit). Each bit enables a lane."),
    "exec_lo" => Some("Lower 32 bits of EXEC (lane execution mask)."),
    "exec_hi" => Some("Upper 32 bits of EXEC (lane execution mask)."),
    "scc" => Some("Scalar condition code (single-bit compare result)."),
    "src_scc" => Some("Scalar condition code (single-bit compare result)."),
    "vcc" => Some("Vector condition code register (64-bit). Per-lane compare results."),
    "vcc_lo" => Some("Lower 32 bits of VCC (vector condition codes)."),
    "vcc_hi" => Some("Upper 32 bits of VCC (vector condition codes)."),
    "pc" => Some("Program counter (64-bit)."),
    "flat_scratch" => Some("Flat scratch base/size pair (64-bit)."),
    "flat_scratch_lo" => Some("Lower 32 bits of FLAT_SCRATCH (base/size)."),
    "flat_scratch_hi" => Some("Upper 32 bits of FLAT_SCRATCH (base/size)."),
    _ => None,
  }
}

fn split_numeric_suffix(name: &str) -> Option<(&str, u32)> {
  let mut split_at = None;
  for (i, ch) in name.char_indices() {
    if ch.is_ascii_digit() {
      split_at = Some(i);
      break;
    }
  }
  let i = split_at?;
  let (prefix, digits) = name.split_at(i);
  if prefix.is_empty() || digits.is_empty() {
    return None;
  }
  let num = digits.parse::<u32>().ok()?;
  Some((prefix, num))
}

fn compress_special_registers(all: Vec<SpecialRegister>) -> SpecialRegistersOutput {
  // Compress contiguous numeric families; values are not retained in the output.
  let mut groups: BTreeMap<String, Vec<(u32, SpecialRegister)>> = BTreeMap::new();
  let mut singles: Vec<SpecialRegister> = Vec::new();

  for reg in all {
    if let Some((prefix, idx)) = split_numeric_suffix(&reg.name) {
      groups
        .entry(prefix.to_string())
        .or_default()
        .push((idx, reg));
    } else {
      singles.push(reg);
    }
  }

  // These are the big families we know are ranges in the ISA docs.
  let compress_prefixes: BTreeSet<&'static str> = ["attr", "param", "mrt", "pos", "ttmp"]
    .into_iter()
    .collect();

  let mut ranges: Vec<SpecialRegisterRange> = Vec::new();
  let mut leftover_singles: Vec<SpecialRegister> = Vec::new();

  for (prefix, mut items) in groups {
    if !compress_prefixes.contains(prefix.as_str()) {
      let fallback = items
        .iter()
        .filter_map(|(_idx, reg)| reg.description.as_ref())
        .find(|desc| !desc.trim().is_empty() && !is_see_above(desc))
        .cloned();
      // Keep as singles, filling in "See above" or empty descriptions when possible.
      for (_idx, mut reg) in items {
        let needs_fallback = reg
          .description
          .as_deref()
          .map(|desc| desc.trim().is_empty() || is_see_above(desc))
          .unwrap_or(true);
        if needs_fallback {
          reg.description = fallback.clone();
        }
        if reg
          .description
          .as_deref()
          .map(|desc| desc.trim().is_empty())
          .unwrap_or(true)
        {
          continue;
        }
        leftover_singles.push(reg);
      }
      continue;
    }

    items.sort_by_key(|(idx, _)| *idx);
    if items.is_empty() {
      continue;
    }

    let start = items[0].0;
    let end = items[items.len() - 1].0;
    let expected_count = (end - start + 1) as usize;
    let is_contiguous = expected_count == items.len() && items
      .iter()
      .enumerate()
      .all(|(offset, (idx, _))| *idx == start + offset as u32);

    // Require a real range (3+).
    if !is_contiguous || items.len() < 3 {
      for (_idx, reg) in items {
        leftover_singles.push(reg);
      }
      continue;
    }

    // Choose the most common non-empty, non-"See above" description from the family.
    // This yields compact overrides for cases like TTMP where one entry has extra detail.
    let mut desc_counts: BTreeMap<String, u32> = BTreeMap::new();
    for (_idx, r) in &items {
      if let Some(d) = &r.description {
        if !d.trim().is_empty() && !is_see_above(d) {
          *desc_counts.entry(d.clone()).or_insert(0) += 1;
        }
      }
    }
    let range_description: Option<String> = desc_counts
      .into_iter()
      .max_by(|a, b| a.1.cmp(&b.1))
      .map(|(d, _count)| d);

    // Add per-index overrides when the description differs materially.
    let mut overrides: Vec<SpecialRegisterRangeOverride> = Vec::new();
    for (idx, r) in &items {
      let mut override_desc = None;
      if let Some(d) = &r.description {
        // Only keep an override if it's non-empty and different from the range description.
        if !d.trim().is_empty() {
          let differs = match &range_description {
            Some(rd) => rd != d,
            None => true,
          };
          if differs && !is_see_above(d) {
            override_desc = Some(d.clone());
          }
        }
      }
      if override_desc.is_some() {
        overrides.push(SpecialRegisterRangeOverride {
          index: *idx,
          description: override_desc,
        });
      }
    }

    ranges.push(SpecialRegisterRange {
      prefix,
      start,
      count: items.len() as u32,
      description: range_description,
      overrides,
    });
  }

  singles.extend(leftover_singles);
  singles.retain(|reg| {
    reg
      .description
      .as_deref()
      .map(|desc| !desc.trim().is_empty() && !is_see_above(desc))
      .unwrap_or(false)
  });
  singles.sort_by(|a, b| a.name.cmp(&b.name));
  ranges.sort_by(|a, b| a.prefix.cmp(&b.prefix));

  SpecialRegistersOutput { singles, ranges }
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

fn build_args(encodings: &[InstructionEncoding]) -> (Vec<String>, Vec<String>, Vec<String>) {
  if encodings.is_empty() {
    return (Vec::new(), Vec::new(), Vec::new());
  }
  let mut args = Vec::new();
  let mut arg_types = Vec::new();
  let mut arg_data_types = Vec::new();
  let mut operands = encodings[0].operands.clone();
  operands.sort_by_key(|operand| operand.order.unwrap_or(u32::MAX));
  for operand in operands {
    if operand.is_implicit == Some(true) {
      continue;
    }
    let label = operand_label(&operand).unwrap_or_else(|| "operand".to_string());
    args.push(label);
    arg_types.push(operand_kind(&operand));
    arg_data_types.push(
      operand
        .data_format_name
        .clone()
        .unwrap_or_else(|| "unknown".to_string()),
    );
  }
  (args, arg_types, arg_data_types)
}

fn parse_special_registers(path: &Path) -> Result<Vec<SpecialRegister>, Box<dyn Error>> {
  let file = fs::File::open(path)?;
  let mut reader = Reader::from_reader(std::io::BufReader::new(file));
  reader.config_mut().trim_text(true);

  let mut buf = Vec::new();
  let mut special_registers: Vec<SpecialRegister> = Vec::new();
  let mut current_register: Option<SpecialRegister> = None;
  let mut in_predefined_values = false;
  let mut text_target: Option<TextTarget> = None;

  loop {
    match reader.read_event_into(&mut buf) {
      Ok(Event::Start(ref event)) => match event.local_name().as_ref() {
        b"OperandPredefinedValues" => {
          in_predefined_values = true;
        }
        b"PredefinedValue" => {
          if in_predefined_values {
            current_register = Some(SpecialRegister {
              name: String::new(),
              description: None,
            });
          }
        }
        b"Name" => {
          if current_register.is_some() {
            text_target = Some(TextTarget::InstructionName);
          }
        }
        b"Description" => {
          if current_register.is_some() {
            text_target = Some(TextTarget::Description);
          }
        }
        b"Value" => {
          if current_register.is_some() {
            // We intentionally ignore numeric encodings in the output.
            text_target = None;
          }
        }
        _ => {}
      },
      Ok(Event::End(ref event)) => match event.local_name().as_ref() {
        b"OperandPredefinedValues" => {
          in_predefined_values = false;
        }
        b"PredefinedValue" => {
          if let Some(reg) = current_register.take() {
            if !reg.name.is_empty() {
              special_registers.push(reg);
            }
          }
        }
        b"Name" | b"Description" | b"Value" => {
          text_target = None;
        }
        _ => {}
      },
      Ok(Event::Text(event)) => {
        if let Some(target) = text_target {
          let text = event.unescape()?.to_string();
          match target {
            TextTarget::InstructionName => {
              if let Some(reg) = &mut current_register {
                reg.name = text;
              }
            }
            TextTarget::Description => {
              if let Some(reg) = &mut current_register {
                reg.description = Some(text);
              }
            }
            TextTarget::OperandSize => {
              // Numeric encodings are intentionally not stored in isa.json.
            }
            _ => {}
          }
        }
      }
      Ok(Event::Eof) => break,
      Err(err) => return Err(Box::new(err)),
      _ => {}
    }
    buf.clear();
  }

  Ok(special_registers)
}

fn is_plain_vector_or_scalar_register(name: &str) -> bool {
  let mut chars = name.chars();
  let prefix = match chars.next() {
    Some(prefix) => prefix,
    None => return false,
  };
  if prefix != 'v' && prefix != 's' {
    return false;
  }
  let rest = chars.as_str();
  !rest.is_empty() && rest.chars().all(|ch| ch.is_ascii_digit())
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
            let (args, arg_types, arg_data_types) = build_args(&inst.encodings);
            inst.args = args;
            inst.arg_types = arg_types;
            inst.arg_data_types = arg_data_types;
            // Collect unique encoding names
            inst.available_encodings = inst
              .encodings
              .iter()
              .filter_map(|enc| enc.encoding_name.clone())
              .collect::<BTreeSet<_>>()
              .into_iter()
              .collect();
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

    // Parse special registers (only from RDNA files to avoid duplicates)
    if input
      .file_name()
      .and_then(|n| n.to_str())
      .map(|s| s.contains("rdna"))
      .unwrap_or(false)
    {
      if let Ok(registers) = parse_special_registers(input) {
        for reg in registers {
          if is_plain_vector_or_scalar_register(&reg.name.to_ascii_lowercase()) {
            continue;
          }
          if is_numeric_literal(&reg.name) {
            continue;
          }
          let mut reg = reg;
          if let Some(desc) = &reg.description {
            if is_see_above(desc) {
              reg.description = None;
            }
          }
          if let Some(override_desc) = special_register_override(&reg.name.to_ascii_lowercase()) {
            reg.description = Some(override_desc.to_string());
          }
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

  // Sort special registers by name for consistent output
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
