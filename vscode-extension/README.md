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

You also can (and probably should) use Bun. 

## Configuration (Settings inside VS Code)

- `rdnaLsp.serverPath`: Path to the `rdna-lsp` binary. Defaults to `rdna-lsp` in PATH.
- `rdnaLsp.dataPath`: Optional path to `isa.json`. If set, passed as `RDNA_LSP_DATA`.
- `rdnaLsp.architecture`: Optional architecture override (e.g. `rdna3.5`, `rdna4`, `cdna4`).

## Packaging

```bash
npm run package
```
or
```
bunx vsce package
```

This uses `vsce` to build a `.vsix` file.

Install the extension with
```
code --install-extension /path/to/extension.vsix
```

You might have to uninstall the old one and reload the window, otherwise it might launch the extension multiple times. todo: find a better way to do this. uninstall extension first? 

## todo! 
Analyze bundle sizes and reduce bloat. 1.3 mb is too big. 
