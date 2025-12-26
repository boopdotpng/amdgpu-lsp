use crate::model::{InstructionDoc, InstructionEncoding, Operand};
use crate::operand::{build_args, parse_operand_attributes};
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::Path;

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

pub fn parse_instruction_file(path: &Path) -> Result<(String, Vec<InstructionDoc>), Box<dyn Error>> {
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
  let mut in_aliased_names = false;

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
