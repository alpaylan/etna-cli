import * as vscode from 'vscode';
import { registerCommands } from './commands';
import { getMutationCodeLensProvider } from './codelens/mutationCodeLensProvider';
import { getMutationDecorationManager, disposeMutationDecorationManager } from './codelens/mutationDecorations';
import { openDashboard } from './webview/webviewManager';
import { getApiClient } from './api/client';
import { disposeServerManager } from './server/serverManager';

let statusBarItem: vscode.StatusBarItem;

export function activate(context: vscode.ExtensionContext): void {
  console.log('Etna extension is now active');

  // Register commands
  registerCommands(context);

  // Register dashboard command
  context.subscriptions.push(
    vscode.commands.registerCommand('etna.openDashboard', async () => {
      await openDashboard(context);
    })
  );

  // Register CodeLens provider for supported file types
  const codeLensProvider = getMutationCodeLensProvider();
  const supportedLanguages = [
    { language: 'rust' },
    { language: 'python' },
    { language: 'javascript' },
    { language: 'typescript' },
    { language: 'coq' },
    { language: 'ocaml' },
    { language: 'haskell' },
  ];

  context.subscriptions.push(
    vscode.languages.registerCodeLensProvider(supportedLanguages, codeLensProvider)
  );

  // Initialize decoration manager
  getMutationDecorationManager();

  // Create status bar item
  statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  statusBarItem.command = 'etna.openDashboard';
  statusBarItem.text = '$(beaker) Etna';
  statusBarItem.tooltip = 'Click to open Etna Dashboard';
  context.subscriptions.push(statusBarItem);

  // Check server health and update status bar
  checkServerHealth();

  // Periodically check server health
  const healthCheckInterval = setInterval(() => {
    checkServerHealth();
  }, 30000); // Check every 30 seconds

  context.subscriptions.push({
    dispose: () => {
      clearInterval(healthCheckInterval);
    },
  });

  // Show status bar
  statusBarItem.show();
}

async function checkServerHealth(): Promise<void> {
  try {
    const client = getApiClient();
    await client.healthCheck();
    statusBarItem.text = '$(beaker) Etna';
    statusBarItem.backgroundColor = undefined;
    statusBarItem.tooltip = 'Etna server is running. Click to open Dashboard.';
  } catch {
    statusBarItem.text = '$(beaker) Etna $(warning)';
    statusBarItem.backgroundColor = new vscode.ThemeColor('statusBarItem.warningBackground');
    statusBarItem.tooltip = 'Etna server is not responding. Click to open Dashboard.';
  }
}

export function deactivate(): void {
  disposeMutationDecorationManager();
  disposeServerManager();
}
