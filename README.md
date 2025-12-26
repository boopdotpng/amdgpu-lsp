# amdgpu-lsp

## features 
- Goto definition for labels inside branch instructions
- Autocomplete for all RDNA/CDNA instructions 
- Hover documentation for every instruction containing arguments, argument types, and info about encodings when present
- Syntax highlighting for rdna files
- Documentation for all special registers (exec, execz, etc)
- Instruction filtering by architecture (file type associations .rdna35, .cdna3, etc or you can set it globally in extension settings)

## todos
- [x] figure out why some instructions are missing from isa.json (different mnemonics or encodings or variations that we don't parse?)
- [x] autocomplete / suggestions (simple find / match first characters for now)
- [x] code review to make sure AI didn't write stupid code 
- [x] performance analysis / memory reduction / extension size reduction

Any work beyond this point is superflous and is not worth my time. This is 80% of what someone would need to start learning and writing RDNA. 

## documentation

### building 

`./build.sh`: 
- fetches the latest AMDGPU XML files containing instructions for each architecture (rdna 1-4, cdna 1-4)
- builds the parse_isa binary, which reads through every XML file, merges them, de-duplicates, and then writes to `data/isa.json`. This is the source of truth for the LSP.
- builds the main tower-lsp project. this is responsible for communication with the VS Code extension. 
- builds the extension, packages it into a .vsix file, and then installs it. it also removes the previously installed version of the extension. 

After this finishes, just reload VS Code (Developer: Reload Window) and you should see the extension. 

### xml parsing information 
The `parse_isa` binary reads AMDGPU XML files (from `amd_gpu_xmls/` by default), extracts a subset of fields, merges instructions across architectures, and writes `data/isa.json`. XML is parsed with `quick_xml` and trimmed text nodes.

#### instruction parsing
- `Instruction/InstructionName` (skips names inside `AliasedInstructionNames`)
- `Instruction/ArchitectureName` (first one in a file only; used as the file's architecture label)
- `Instruction/Description`
- `Instruction/InstructionEncoding/EncodingName`
- `Instruction/InstructionEncoding/Operand` attributes: `Input`, `Output`, `IsImplicit`, `Order`
- `Instruction/InstructionEncoding/Operand` text fields: `FieldName`, `OperandType`, `DataFormatName`, `OperandSize`

Derived fields:
- `args`, `arg_types`, `arg_data_types` are built from the first encoding only
- operands are sorted by `Order`; implicit operands are skipped
- `arg_types` is inferred from `OperandType` into: `immediate`, `label`, `memory`, `register`, `register_or_inline`,
  `special`, or `unknown`
- `available_encodings` is the set of `EncodingName` values (sorted)

#### architecture normalization
Architecture names are normalized to a compact `rdnaN`/`cdnaN` form:
- lowercased, whitespace trimmed
- tokens containing `rdna` or `cdna` set the family
- a version is taken from the same token (e.g. `rdna3`) or a later numeric token (e.g. `rdna 3`)
- if no family is found, the name is lowercased with spaces removed

#### special register parsing
Special registers are only parsed from RDNA XML files (file name contains `rdna`).
- `OperandPredefinedValues/PredefinedValue/Name`
- `OperandPredefinedValues/PredefinedValue/Description` (ignores `Value`)

Post-processing:
- drops numeric literals (e.g. `0`) and plain `sN`/`vN` registers
- drops empty descriptions or `<p>See above.</p>` placeholders
- overrides descriptions for core registers like `exec`, `scc`, `vcc`, `pc`, `flat_scratch`
- merges duplicates by name, preferring the longest description
- compresses contiguous ranges for `attr`, `param`, `mrt`, `pos`, `ttmp` when 3+ entries are present
- when not compressed, uses the first non-empty description as a fallback for that prefix

#### edge cases and current behavior
- If `ArchitectureName` is missing, the architecture label can be empty; the instruction still emits with an empty
  architecture tag after normalization.
- Aliased instruction names are ignored to avoid duplicates.
- If an instruction has multiple encodings, only the first encoding drives `args` and type inference.
- Missing operand fields yield `unknown` values (e.g. `OperandType` or `DataFormatName`).
- Descriptions from XML are not HTML-stripped; they are stored as-is unless filtered by the rules above.

#### data/isa.json format
Top-level shape:
```json
{
  "instructions": [ ... ],
  "special_registers": {
    "singles": [ ... ],
    "ranges": [ ... ]
  }
}
```

Instruction entry:
```json
{
  "name": "v_add_f32",
  "architectures": ["rdna3", "rdna35"],
  "description": "Adds two FP32 values.",
  "args": ["src0", "src1", "dst"],
  "arg_types": ["register", "register", "register"],
  "arg_data_types": ["f32", "f32", "f32"],
  "available_encodings": ["VOP2", "VOP3"]
}
```

Special register entries:
```json
{
  "singles": [
    { "name": "exec", "description": "Wavefront execution mask (64-bit). Each bit enables a lane." }
  ],
  "ranges": [
    {
      "prefix": "attr",
      "start": 0,
      "count": 32,
      "description": "Attribute register.",
      "overrides": [
        { "index": 7, "description": "Attribute register for XYZ." }
      ]
    }
  ]
}
```

### extension options 

Architecture: The extension registers file types (.rdna3, .rdna35, .cdna4, ... for each arch), but you can use the .rdna extension and set a default architecture for all files if you're only writing for one architecture.

Data Path: Path to `data/isa.json`. Set to the bundled json file inside the extension by default. 

Server Path: Path to the lsp binary, usually `target/debug/amdgpu-lsp` (or release, if you want). Set to the executable bundled in the extension by default.

### release versioning

To auto-sync versions when you push tags, enable the repo hook:

```bash
git config core.hooksPath scripts/git-hooks
```

When you push a tag like `v0.2.1`, the hook updates `Cargo.toml` and `vscode-extension/package.json` to match, then stops the push so you can commit the version bump.

### contributing

If you plan to push tags, enable the hook so version bumps are not missed:

```bash
git config core.hooksPath scripts/git-hooks
```

## resources 

To build `data/isa.json` I used files from [gpuopen](https://gpuopen.com/machine-readable-isa/). 

The documentation for the XML format used in those files is [here](https://github.com/GPUOpen-Tools/isa_spec_manager/blob/main/documentation/spec_documentation.md).

[RDNA 3.5 ISA](https://docs.amd.com/v/u/en-US/rdna35_instruction_set_architecture) (most of the development was focused on 3.5, since I have an AI 7 Framework 13).

[rdna playground](https://github.com/boopdotpng/rdna-playground) so I could write, run and disassemble RDNA programs easily.
