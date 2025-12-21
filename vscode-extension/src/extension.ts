import * as fs from "fs";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient | null = null;

function resolveServerPath(): string {
  const config = vscode.workspace.getConfiguration("rdnaLsp");
  const configured = config.get<string>("serverPath");
  if (configured && configured.trim().length > 0) {
    return configured;
  }
  return "rdna-lsp";
}

function resolveServerEnv(): NodeJS.ProcessEnv {
  const config = vscode.workspace.getConfiguration("rdnaLsp");
  const dataPath = config.get<string>("dataPath")?.trim();
  if (!dataPath) {
    return process.env;
  }
  return {
    ...process.env,
    RDNA_LSP_DATA: dataPath,
  };
}

function resolveArchitectureOverride(): string | undefined {
  const config = vscode.workspace.getConfiguration("rdnaLsp");
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
      return `RDNA LSP server not found at ${command}`;
    }
    try {
      const stat = fs.statSync(command);
      if (!stat.isFile()) {
        return `RDNA LSP server path is not a file: ${command}`;
      }
    } catch (error) {
      return `RDNA LSP server path is not accessible: ${command}`;
    }
  }
  return undefined;
}

function createClient(command: string): LanguageClient {
  const serverOptions: ServerOptions = {
    command,
    args: [],
    options: {
      env: resolveServerEnv(),
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
    outputChannelName: "RDNA LSP",
    initializationOptions: {
      architectureOverride: resolveArchitectureOverride(),
    },
  };

  return new LanguageClient("rdnaLsp", "RDNA LSP", serverOptions, clientOptions);
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const startClient = async () => {
    const command = resolveServerPath();
    const pathError = validateServerPath(command);
    if (pathError) {
      vscode.window.showErrorMessage(pathError);
      return;
    }
    client = createClient(command);
    client.start();
    context.subscriptions.push(client);
  };

  context.subscriptions.push(
    vscode.commands.registerCommand("rdnaLsp.restart", async () => {
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
