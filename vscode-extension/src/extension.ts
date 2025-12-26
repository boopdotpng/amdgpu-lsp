import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | null = null;

function resolveBundledServerPath(context: vscode.ExtensionContext): string | undefined {
  const binaryName = "amdgpu-lsp";
  const candidate = context.asAbsolutePath(path.join("bin", binaryName));
  return fs.existsSync(candidate) ? candidate : undefined;
}

function resolveBundledDataPath(context: vscode.ExtensionContext): string | undefined {
  const candidate = context.asAbsolutePath(path.join("data", "isa.json"));
  return fs.existsSync(candidate) ? candidate : undefined;
}

function resolveServerPath(context: vscode.ExtensionContext): string {
  const config = vscode.workspace.getConfiguration("amdgpuLsp");
  const configured = config.get<string>("serverPath");
  if (configured && configured.trim().length > 0) {
    return configured;
  }
  const bundled = resolveBundledServerPath(context);
  return bundled ?? "amdgpu-lsp";
}

function resolveServerEnv(context: vscode.ExtensionContext): NodeJS.ProcessEnv {
  const config = vscode.workspace.getConfiguration("amdgpuLsp");
  const dataPath = config.get<string>("dataPath")?.trim() || resolveBundledDataPath(context);
  if (!dataPath) {
    return process.env;
  }
  return {
    ...process.env,
    AMDGPU_LSP_DATA: dataPath,
  };
}

function resolveArchitectureOverride(): string | undefined {
  const config = vscode.workspace.getConfiguration("amdgpuLsp");
  const override = config.get<string>("architecture")?.trim();
  return override ? override : undefined;
}

function resolveServerCwd(): string | undefined {
  const folders = vscode.workspace.workspaceFolders;
  if (folders && folders.length > 0) {
    return folders[0].uri.fsPath;
  }
  return undefined;
}

function validateServerPath(command: string): string | undefined {
  if (command.includes("/") || command.includes("\\")) {
    if (!fs.existsSync(command)) {
      return `AMDGPU LSP server not found at ${command}`;
    }
    try {
      const stat = fs.statSync(command);
      if (!stat.isFile()) {
        return `AMDGPU LSP server path is not a file: ${command}`;
      }
    } catch (error) {
      return `AMDGPU LSP server path is not accessible: ${command}`;
    }
  }
  return undefined;
}

function createClient(command: string, env: NodeJS.ProcessEnv): LanguageClient {
  const serverOptions: ServerOptions = {
    command,
    args: [],
    options: {
      env,
      cwd: resolveServerCwd(),
    },
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: "asm" },
      { scheme: "file", language: "amdisa" },
      { scheme: "file", language: "rdna" },
      { scheme: "file", language: "rdna3" },
      { scheme: "file", language: "rdna35" },
      { scheme: "file", language: "rdna4" },
      { scheme: "file", language: "cdna" },
      { scheme: "file", language: "cdna3" },
      { scheme: "file", language: "cdna4" },
    ],
    outputChannelName: "AMDGPU Language Server",
    initializationOptions: {
      architectureOverride: resolveArchitectureOverride(),
    },
  };

  return new LanguageClient("amdgpuLsp", "AMDGPU Language Server", serverOptions, clientOptions);
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const startClient = async () => {
    const command = resolveServerPath(context);
    const pathError = validateServerPath(command);
    if (pathError) {
      vscode.window.showErrorMessage(pathError);
      return;
    }
    const env = resolveServerEnv(context);
    client = createClient(command, env);
    client.start();
    context.subscriptions.push(client);
  };

  context.subscriptions.push(
    vscode.commands.registerCommand("amdgpuLsp.restart", async () => {
      if (client) {
        await client.stop();
        client = null;
      }
      await startClient();
    })
  );

  await startClient();
}

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
  }
}
