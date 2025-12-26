# amdgpu-lsp

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


### extension options 

Architecture: The extension registers file types (.rdna3, .rdna35, .cdna4, ... for each arch), but you can use the .rdna extension and set a default architecture for all files if you're only writing for one architecture.

** If you built locally, you will have to modify these.**

Data Path: Path to `data/isa.json`.

Server Path: Path to the lsp binary, usually `target/debug/amdgpu-lsp` (or release, if you want). 

## resources 

To build `data/isa.json` I used files from [gpuopen](https://gpuopen.com/machine-readable-isa/). 

The documentation for the XML format used in those files is [here](https://github.com/GPUOpen-Tools/isa_spec_manager/blob/main/documentation/spec_documentation.md).

[RDNA 3.5 ISA](https://docs.amd.com/v/u/en-US/rdna35_instruction_set_architecture) (most of the development was focused on 3.5, since I have an AI 7 Framework 13).

[rdna playground](https://github.com/boopdotpng/rdna-playground) so I could write, run and disassemble RDNA programs easily.
