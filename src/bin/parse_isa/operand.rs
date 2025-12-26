use crate::model::{InstructionEncoding, Operand};
use quick_xml::events::BytesStart;

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

pub fn parse_operand_attributes(attrs: &BytesStart<'_>) -> Operand {
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
  operand.operand_type.clone()
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

pub fn build_args(encodings: &[InstructionEncoding]) -> (Vec<String>, Vec<String>, Vec<String>) {
  if encodings.is_empty() {
    return (Vec::new(), Vec::new(), Vec::new());
  }
  let mut operands = encodings[0].operands.clone();
  operands.sort_by_key(|operand| operand.order.unwrap_or(u32::MAX));

  let mut args = Vec::new();
  let mut arg_types = Vec::new();
  let mut arg_data_types = Vec::new();
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
