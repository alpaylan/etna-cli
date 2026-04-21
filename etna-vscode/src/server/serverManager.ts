import * as vscode from 'vscode';
import * as cp from 'child_process';
import axios from 'axios';
import { ensureDownloadedBinary } from './serverDownload';

let managedProcess: cp.ChildProcess | undefined;
let startingPromise: Promise<void> | undefined;
let outputChannel: vscode.OutputChannel | undefined;

function getOutputChannel(): vscode.OutputChannel {
  if (!outputChannel) {
    outputChannel = vscode.window.createOutputChannel('Etna Server');
  }
  return outputChannel;
}

async function isHealthy(serverUrl: string, timeoutMs = 1500): Promise<boolean> {
  try {
    await axios.get(`${serverUrl}/api/v1/health`, { timeout: timeoutMs });
    return true;
  } catch {
    return false;
  }
}

function isLocalHost(hostname: string): boolean {
  return hostname === 'localhost' || hostname === '127.0.0.1' || hostname === '::1';
}

export async function ensureServerRunning(context: vscode.ExtensionContext): Promise<boolean> {
  const cfg = vscode.workspace.getConfiguration('etna');
  const serverUrl = cfg.get<string>('serverUrl', 'http://localhost:3000');
  const autoStart = cfg.get<boolean>('autoStartServer', true);

  if (await isHealthy(serverUrl)) return true;
  if (!autoStart) return false;

  let url: URL;
  try {
    url = new URL(serverUrl);
  } catch {
    vscode.window.showErrorMessage(`Invalid etna.serverUrl: ${serverUrl}`);
    return false;
  }

  if (!isLocalHost(url.hostname)) {
    vscode.window.showErrorMessage(
      `Etna server at ${serverUrl} is not reachable (auto-start only supports local servers).`
    );
    return false;
  }

  if (startingPromise) {
    try {
      await startingPromise;
    } catch {
      // fall through to report health below
    }
    return await isHealthy(serverUrl);
  }

  startingPromise = Promise.resolve(
    vscode.window.withProgress(
      {
        location: vscode.ProgressLocation.Notification,
        title: 'Starting Etna server…',
        cancellable: false,
      },
      () => startServer(url, context)
    )
  );

  try {
    await startingPromise;
    return true;
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    vscode.window.showErrorMessage(`Failed to start Etna server: ${msg}`);
    return false;
  } finally {
    startingPromise = undefined;
  }
}

async function resolveBinary(context: vscode.ExtensionContext): Promise<string> {
  // Explicit override wins (dev / custom install). Empty string (the default)
  // means "auto-download the latest release binary into globalStorage".
  const override = vscode.workspace.getConfiguration('etna').get<string>('serverBinary', '').trim();
  if (override.length > 0) return override;
  return ensureDownloadedBinary(context);
}

async function startServer(url: URL, context: vscode.ExtensionContext): Promise<void> {
  const binary = await resolveBinary(context);
  const port = url.port || (url.protocol === 'https:' ? '443' : '3000');
  // etna-server's --host parses as an IP literal (no DNS). Node's URL
  // lowercases localhost, but the server can't resolve it — map to 127.0.0.1.
  const host = url.hostname === 'localhost' ? '127.0.0.1' : url.hostname;

  const channel = getOutputChannel();
  channel.appendLine(`[${new Date().toISOString()}] spawning: ${binary} --host ${host} --port ${port}`);

  const child = cp.spawn(binary, ['--host', host, '--port', port], {
    stdio: ['ignore', 'pipe', 'pipe'],
    detached: false,
    env: { ...process.env },
  });

  child.stdout?.on('data', (data: Buffer) => channel.append(data.toString()));
  child.stderr?.on('data', (data: Buffer) => channel.append(data.toString()));

  const spawnError = new Promise<never>((_, reject) => {
    child.once('error', (err) => reject(err));
    child.once('exit', (code, signal) => {
      if (managedProcess === child) managedProcess = undefined;
      reject(new Error(`etna-server exited early (code=${code}, signal=${signal}). See "Etna Server" output for details.`));
    });
  });

  managedProcess = child;

  const serverUrl = `${url.protocol}//${url.hostname}:${port}`;
  const deadline = Date.now() + 15000;
  const poll = (async () => {
    while (Date.now() < deadline) {
      if (await isHealthy(serverUrl, 800)) return;
      await new Promise((r) => setTimeout(r, 250));
    }
    throw new Error('Timed out waiting for Etna server to become healthy');
  })();

  try {
    await Promise.race([poll, spawnError]);
  } catch (err) {
    stopServer();
    throw err;
  }

  // Detach the early-exit rejection now that we're healthy; surface future exits via output channel only.
  child.removeAllListeners('exit');
  child.on('exit', (code, signal) => {
    channel.appendLine(`[${new Date().toISOString()}] etna-server exited (code=${code}, signal=${signal})`);
    if (managedProcess === child) managedProcess = undefined;
  });
}

export function stopServer(): void {
  if (managedProcess && !managedProcess.killed) {
    try {
      managedProcess.kill('SIGTERM');
    } catch {
      // ignore
    }
  }
  managedProcess = undefined;
}

export function disposeServerManager(): void {
  stopServer();
  outputChannel?.dispose();
  outputChannel = undefined;
}
