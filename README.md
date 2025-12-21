# rdna-lsp


## documentation
This project is split into two parts. The LSP itself and the XML parser. 

Run `./fetch.sh` first to get the latest XML files, and then run 
```
cargo run --bin parse-isa
``` 
to generate `data/isa.json`. This is the source of truth for the LSP.

`main.rs` is the binary that the VS Code extension talks to. 

## resources 

To build `data/isa.json` we used files from [gpuopen](https://gpuopen.com/machine-readable-isa/). 

The documentation for the XML format used in those files is [here](https://github.com/GPUOpen-Tools/isa_spec_manager/blob/main/documentation/spec_documentation.md).

[RDNA 3.5 ISA](https://docs.amd.com/v/u/en-US/rdna35_instruction_set_architecture) (most of the development was focused on 3.5, since I have an AI 7 Framework 13).

[rdna playground](https://github.com/boopdotpng/rdna-playground) so I could write, run and disassemble RDNA programs easily.

