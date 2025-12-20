import * as path from "path";
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node";

let client: LanguageClient | null = null;

function resolveServerPath(context: vscode.ExtensionContext): string {
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

function createClient(context: vscode.ExtensionContext): LanguageClient {
  const command = resolveServerPath(context);
  const serverOptions: ServerOptions = {
    command,
    args: [],
    options: {
      env: resolveServerEnv(),
    },
    transport: TransportKind.stdio,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: "asm" },
      { scheme: "file", language: "amdisa" },
    ],
    outputChannelName: "RDNA LSP",
  };

  return new LanguageClient("rdnaLsp", "RDNA LSP", serverOptions, clientOptions);
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  const startClient = async () => {
    client = createClient(context);
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
