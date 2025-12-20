# RDNA LSP VS Code Extension

This VS Code extension launches the `rdna-lsp` server over stdio and provides hover documentation for AMD RDNA/CDNA instructions.

## Development

1. Install dependencies:

```bash
npm install
```

2. Build the extension:

```bash
npm run build
```

3. Launch the extension from VS Code:

- Open this folder in VS Code.
- Run "Run RDNA LSP Extension" from the Run and Debug panel.

## Configuration

- `rdnaLsp.serverPath`: Path to the `rdna-lsp` binary. Defaults to `rdna-lsp` in PATH.
- `rdnaLsp.dataPath`: Optional path to `isa.json`. If set, passed as `RDNA_LSP_DATA`.

## Packaging

```bash
npm run package
```

This uses `vsce` to build a `.vsix` file.
