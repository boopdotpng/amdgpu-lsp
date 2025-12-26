use crate::encoding::{find_matching_encoding, get_encoding_description};
use crate::types::{EncodingVariant, InstructionEntry, SpecialRegister};
use tower_lsp::lsp_types::{HoverContents, MarkupContent, MarkupKind};

pub fn format_mnemonic(name: &str) -> String {
  name.to_ascii_lowercase()
}

fn format_arg_type(arg_type: &str) -> Option<String> {
  match arg_type {
    "register" => Some("reg".to_string()),
    "register_or_inline" => Some("reg/inline".to_string()),
    "immediate" => Some("imm".to_string()),
    "unknown" => None,
    _ => Some(arg_type.to_string()),
  }
}

fn format_data_type(data_type: &str) -> Option<&'static str> {
  match data_type {
    "FMT_NUM_B32" => Some("b32"),
    "FMT_NUM_B64" => Some("b64"),
    "FMT_NUM_F16" => Some("f16"),
    "FMT_NUM_F32" => Some("f32"),
    "FMT_NUM_F64" => Some("f64"),
    "FMT_NUM_BF16" => Some("bf16"),
    "FMT_NUM_I8" => Some("i8"),
    "FMT_NUM_I16" => Some("i16"),
    "FMT_NUM_I32" => Some("i32"),
    "FMT_NUM_I64" => Some("i64"),
    "FMT_NUM_U16" => Some("u16"),
    "FMT_NUM_U32" => Some("u32"),
    "FMT_NUM_U64" => Some("u64"),
    "FMT_ANY" => Some("any"),
    _ => None,
  }
}

pub fn format_hover(entry: &InstructionEntry, variant: &EncodingVariant) -> HoverContents {
  let mut lines = Vec::new();
  lines.push(format!("**{}**", format_mnemonic(&entry.name)));

  if !entry.args.is_empty() {
    let args = entry
      .args
      .iter()
      .enumerate()
      .map(|(index, arg)| {
        let arg_type = entry.arg_types.get(index).map(|value| value.as_str()).unwrap_or("unknown");
        let arg_type = format_arg_type(arg_type);
        let data_type = entry
          .arg_data_types
          .get(index)
          .map(|value| value.as_str())
          .and_then(format_data_type);
        let type_label = match (arg_type, data_type) {
          (Some(arg_type), Some(data_type)) => format!("{arg_type} {data_type}"),
          (Some(arg_type), None) => arg_type,
          (None, Some(data_type)) => data_type.to_string(),
          (None, None) => String::new(),
        };
        if type_label.is_empty() {
          arg.to_string()
        } else {
          format!("{arg}: {type_label}")
        }
      })
      .collect::<Vec<_>>()
      .join(", ");
    lines.push(args);
  }
  if let Some(description) = &entry.description {
    if !description.is_empty() {
      lines.push(description.clone());
    }
  }

  if *variant != EncodingVariant::Native {
    if let Some(encoding_name) = find_matching_encoding(&entry.available_encodings, variant) {
      if let Some(desc) = get_encoding_description(&encoding_name) {
        lines.push(format!("Encoding: {}", desc));
      } else {
        lines.push(format!("Encoding: {}", encoding_name));
      }
    }
  }

  HoverContents::Markup(MarkupContent {
    kind: MarkupKind::Markdown,
    value: lines.join("\n\n"),
  })
}

pub fn format_special_register_hover(register: &SpecialRegister) -> HoverContents {
  let mut lines = Vec::new();
  lines.push(format!("**{}**", register.name));

  if let Some(description) = &register.description {
    if !description.is_empty() {
      lines.push(description.clone());
    }
  }

  HoverContents::Markup(MarkupContent {
    kind: MarkupKind::Markdown,
    value: lines.join("\n\n"),
  })
}
