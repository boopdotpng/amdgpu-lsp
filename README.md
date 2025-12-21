# rdna-lsp

## todos
- [x] figure out why some instructions are missing from isa.json (different mnemonics or encodings or variations that we don't parse?)
- [x] autocomplete / suggestions (simple find / match first characters for now)
- [ ] operand count and basic type checking. linter errors before you run (goal is to catch >50% of flaws with code)
- [ ] jump to label definition, go to definition, find all references
- [ ] inlay hints of some kind? show type of register and the width?

Any work beyond this point is superflous and is not worth my time. This is 80% of what someone would need to start learning and writing RDNA. 


## documentation
This project is split into two parts. The LSP itself and the XML parser. 

Run `./fetch.sh` to get the latest XML files (or just run `./build.sh`, which will fetch them if needed), and then run 
```
cargo run --bin parse_isa
``` 
to generate `data/isa.json`. This is the source of truth for the LSP.

`main.rs` is the binary that the VS Code extension talks to. 

## resources 

To build `data/isa.json` we used files from [gpuopen](https://gpuopen.com/machine-readable-isa/). 

The documentation for the XML format used in those files is [here](https://github.com/GPUOpen-Tools/isa_spec_manager/blob/main/documentation/spec_documentation.md).

[RDNA 3.5 ISA](https://docs.amd.com/v/u/en-US/rdna35_instruction_set_architecture) (most of the development was focused on 3.5, since I have an AI 7 Framework 13).

[rdna playground](https://github.com/boopdotpng/rdna-playground) so I could write, run and disassemble RDNA programs easily.
