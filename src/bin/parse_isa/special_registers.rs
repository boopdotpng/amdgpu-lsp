use crate::model::{
  SpecialRegister, SpecialRegisterRange, SpecialRegisterRangeOverride, SpecialRegistersOutput,
};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::path::Path;

fn is_see_above(desc: &str) -> bool {
  desc.trim() == "<p>See above.</p>" || desc.trim().eq_ignore_ascii_case("see above")
}

pub fn is_numeric_literal(name: &str) -> bool {
  name.parse::<f64>().is_ok()
}

pub fn is_plain_vector_or_scalar_register(name: &str) -> bool {
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

pub fn is_ignored_special_register(name: &str) -> bool {
  is_plain_vector_or_scalar_register(name) || is_numeric_literal(name)
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

pub fn compress_special_registers(all: Vec<SpecialRegister>) -> SpecialRegistersOutput {
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
    let is_contiguous = expected_count == items.len()
      && items
        .iter()
        .enumerate()
        .all(|(offset, (idx, _))| *idx == start + offset as u32);

    if !is_contiguous || items.len() < 3 {
      for (_idx, reg) in items {
        leftover_singles.push(reg);
      }
      continue;
    }

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

    let mut overrides: Vec<SpecialRegisterRangeOverride> = Vec::new();
    for (idx, r) in &items {
      let mut override_desc = None;
      if let Some(d) = &r.description {
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

pub fn normalize_special_register(mut reg: SpecialRegister) -> SpecialRegister {
  if let Some(desc) = &reg.description {
    if is_see_above(desc) {
      reg.description = None;
    }
  }
  if let Some(override_desc) = special_register_override(&reg.name.to_ascii_lowercase()) {
    reg.description = Some(override_desc.to_string());
  }
  reg
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum TextTarget {
  Name,
  Description,
}

pub fn parse_special_registers(path: &Path) -> Result<Vec<SpecialRegister>, Box<dyn Error>> {
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
            text_target = Some(TextTarget::Name);
          }
        }
        b"Description" => {
          if current_register.is_some() {
            text_target = Some(TextTarget::Description);
          }
        }
        b"Value" => {
          if current_register.is_some() {
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
            TextTarget::Name => {
              if let Some(reg) = &mut current_register {
                reg.name = text;
              }
            }
            TextTarget::Description => {
              if let Some(reg) = &mut current_register {
                reg.description = Some(text);
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

  Ok(special_registers)
}
