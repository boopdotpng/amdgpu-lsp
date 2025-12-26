use crate::types::{EncodingVariant, SplitInstruction};

pub fn split_encoding_variant(mnemonic: &str) -> SplitInstruction {
  // Order matters: check longer suffixes first to avoid partial matches
  const SUFFIXES: &[(&str, EncodingVariant)] = &[
    ("_e64_dpp", EncodingVariant::E64Dpp),
    ("_e32", EncodingVariant::E32),
    ("_e64", EncodingVariant::E64),
    ("_dpp", EncodingVariant::Dpp),
    ("_sdwa", EncodingVariant::Sdwa),
  ];

  let mnemonic_lower = mnemonic.to_ascii_lowercase();
  for (suffix, variant) in SUFFIXES {
    if mnemonic_lower.ends_with(suffix) {
      return SplitInstruction {
        base: mnemonic[..mnemonic.len() - suffix.len()].to_string(),
        variant: variant.clone(),
      };
    }
  }

  SplitInstruction {
    base: mnemonic.to_string(),
    variant: EncodingVariant::Native,
  }
}

pub fn get_encoding_description(encoding_name: &str) -> Option<&'static str> {
  match encoding_name {
    // Standard encodings
    "ENC_VOP1" => Some("VOP1 (32-bit): Vector ALU operation with one source"),
    "ENC_VOP2" => Some("VOP2 (32-bit): Vector ALU operation with two sources"),
    "ENC_VOPC" => Some("VOPC (32-bit): Vector ALU comparison operation"),
    "ENC_VOP3" => Some("VOP3 (64-bit): Extended vector ALU with modifiers and additional operand flexibility"),
    "ENC_VOP3P" => Some("VOP3P (64-bit): Packed vector ALU operation"),

    // DPP encodings
    "VOP1_VOP_DPP" | "VOP1_VOP_DPP16" => Some("VOP1 + DPP16: Data-parallel primitives with 16-lane swizzle"),
    "VOP1_VOP_DPP8" => Some("VOP1 + DPP8: Data-parallel primitives with 8-lane swizzle"),
    "VOP2_VOP_DPP" | "VOP2_VOP_DPP16" => Some("VOP2 + DPP16: Data-parallel primitives with 16-lane swizzle"),
    "VOP2_VOP_DPP8" => Some("VOP2 + DPP8: Data-parallel primitives with 8-lane swizzle"),
    "VOPC_VOP_DPP" | "VOPC_VOP_DPP16" => Some("VOPC + DPP16: Comparison with data-parallel primitives (16-lane)"),
    "VOPC_VOP_DPP8" => Some("VOPC + DPP8: Comparison with data-parallel primitives (8-lane)"),
    "VOP3_VOP_DPP16" => Some("VOP3 + DPP16: Extended VOP3 with data-parallel primitives (16-lane)"),
    "VOP3_VOP_DPP8" => Some("VOP3 + DPP8: Extended VOP3 with data-parallel primitives (8-lane)"),
    "VOP3P_VOP_DPP16" => Some("VOP3P + DPP16: Packed operation with data-parallel primitives (16-lane)"),
    "VOP3P_VOP_DPP8" => Some("VOP3P + DPP8: Packed operation with data-parallel primitives (8-lane)"),
    "VOP3_SDST_ENC_VOP_DPP16" => Some("VOP3 SDST + DPP16: VOP3 with scalar destination and DPP (16-lane)"),
    "VOP3_SDST_ENC_VOP_DPP8" => Some("VOP3 SDST + DPP8: VOP3 with scalar destination and DPP (8-lane)"),

    // SDWA encodings
    "VOP1_VOP_SDWA" => Some("VOP1 + SDWA: Sub-DWORD addressing for byte/word operations"),
    "VOP2_VOP_SDWA" => Some("VOP2 + SDWA: Sub-DWORD addressing for byte/word operations"),
    "VOPC_VOP_SDWA" => Some("VOPC + SDWA: Comparison with sub-DWORD addressing"),

    // Literal encodings
    "VOP1_INST_LITERAL" => Some("VOP1 + Literal (64-bit): Includes 32-bit inline constant"),
    "VOP2_INST_LITERAL" => Some("VOP2 + Literal (64-bit): Includes 32-bit inline constant"),
    "VOPC_INST_LITERAL" => Some("VOPC + Literal (64-bit): Includes 32-bit inline constant"),
    "VOP3_INST_LITERAL" => Some("VOP3 + Literal (96-bit): VOP3 with 32-bit inline constant"),
    "VOP3P_INST_LITERAL" => Some("VOP3P + Literal (96-bit): Packed operation with 32-bit inline constant"),
    "VOP3_SDST_ENC_INST_LITERAL" => Some("VOP3 SDST + Literal (96-bit): VOP3 with scalar destination and literal"),

    // Special VOP3 variants
    "VOP3_SDST_ENC" => Some("VOP3 SDST (64-bit): VOP3 with scalar destination"),

    // Scalar encodings
    "ENC_SOP1" => Some("SOP1 (32-bit): Scalar ALU operation with one source"),
    "ENC_SOP2" => Some("SOP2 (32-bit): Scalar ALU operation with two sources"),
    "ENC_SOPC" => Some("SOPC (32-bit): Scalar ALU comparison operation"),
    "ENC_SOPK" => Some("SOPK (32-bit): Scalar operation with 16-bit inline constant"),
    "ENC_SOPP" => Some("SOPP (32-bit): Scalar operation for program control"),
    "SOP1_INST_LITERAL" => Some("SOP1 + Literal (64-bit): Scalar operation with 32-bit inline constant"),
    "SOP2_INST_LITERAL" => Some("SOP2 + Literal (64-bit): Scalar operation with 32-bit inline constant"),
    "SOPC_INST_LITERAL" => Some("SOPC + Literal (64-bit): Scalar comparison with 32-bit inline constant"),
    "SOPK_INST_LITERAL" => Some("SOPK + Literal (64-bit): Scalar operation with extended constant"),

    // Memory encodings
    "ENC_SMEM" => Some("SMEM: Scalar memory operation"),
    "ENC_DS" => Some("DS: Data share (LDS/GDS) operation"),
    "ENC_MUBUF" => Some("MUBUF: Untyped buffer memory operation"),
    "ENC_MTBUF" => Some("MTBUF: Typed buffer memory operation"),
    "ENC_MIMG" => Some("MIMG: Image memory operation"),
    "MIMG_NSA1" => Some("MIMG NSA: Non-sequential address mode for images"),
    "ENC_FLAT" => Some("FLAT: Flat addressing (global/scratch/LDS)"),
    "ENC_FLAT_SCRATCH" => Some("FLAT Scratch: Flat addressing for scratch memory"),
    "ENC_FLAT_GLOBAL" => Some("FLAT Global: Flat addressing for global memory"),

    // Interpolation and other
    "ENC_VINTERP" => Some("VINTERP: Vector interpolation operation"),
    "ENC_LDSDIR" => Some("LDSDIR: LDS direct read operation"),
    "ENC_EXP" => Some("EXP: Export operation for pixel/vertex data"),
    "VOPDXY" => Some("VOPDXY: Vector operation with partial derivatives"),
    "VOPDXY_INST_LITERAL" => Some("VOPDXY + Literal: Vector partial derivative with inline constant"),

    _ => None,
  }
}

pub fn find_matching_encoding(available_encodings: &[String], variant: &EncodingVariant) -> Option<String> {
  // Map LLVM suffix variants to potential encoding name patterns
  match variant {
    EncodingVariant::Native => {
      // For native (no suffix), prefer the base encoding (ENC_VOP1/2/3, etc.)
      available_encodings
        .iter()
        .find(|enc| enc.starts_with("ENC_") && !enc.contains("LITERAL"))
        .cloned()
    }
    EncodingVariant::E32 => {
      // _e32 maps to base VOP1/VOP2/VOPC encodings
      available_encodings
        .iter()
        .find(|enc| matches!(enc.as_str(), "ENC_VOP1" | "ENC_VOP2" | "ENC_VOPC"))
        .cloned()
    }
    EncodingVariant::E64 => {
      // _e64 maps to VOP3 encoding
      available_encodings
        .iter()
        .find(|enc| enc.as_str() == "ENC_VOP3")
        .cloned()
    }
    EncodingVariant::Dpp => {
      // _dpp maps to DPP encodings (prefer DPP16 over DPP8)
      available_encodings
        .iter()
        .find(|enc| enc.contains("DPP16") || enc.contains("DPP"))
        .cloned()
    }
    EncodingVariant::Sdwa => {
      // _sdwa maps to SDWA encodings
      available_encodings
        .iter()
        .find(|enc| enc.contains("SDWA"))
        .cloned()
    }
    EncodingVariant::E64Dpp => {
      // _e64_dpp maps to VOP3 + DPP encodings
      available_encodings
        .iter()
        .find(|enc| enc.starts_with("VOP3") && (enc.contains("DPP16") || enc.contains("DPP")))
        .cloned()
    }
  }
}
