import { handleBrowserMessage, isBrowserMode } from './browserShim';

// Type declarations for VSCode webview API
declare function acquireVsCodeApi(): VsCodeApi;

interface VsCodeApi {
  postMessage(message: unknown): void;
  getState(): unknown;
  setState(state: unknown): void;
}

// Singleton pattern for VSCode API
class VSCodeAPIWrapper {
  private readonly vsCodeApi: VsCodeApi | undefined;
  private readonly browser: boolean;

  constructor() {
    if (typeof acquireVsCodeApi === 'function') {
      this.vsCodeApi = acquireVsCodeApi();
    }
    this.browser = !this.vsCodeApi && isBrowserMode();
    if (this.browser) {
      console.info('[etna] Running in browser dev mode — talking to server directly.');
    }
  }

  public postMessage(message: unknown): void {
    if (this.vsCodeApi) {
      this.vsCodeApi.postMessage(message);
      return;
    }
    if (this.browser) {
      // Fire-and-forget; the shim dispatches the response back as a window message.
      void handleBrowserMessage(message as { type: string; [k: string]: unknown });
      return;
    }
    console.log('VSCode API not available, message:', message);
  }

  public getState<T>(): T | undefined {
    if (this.vsCodeApi) {
      return this.vsCodeApi.getState() as T | undefined;
    }
    return undefined;
  }

  public setState<T>(state: T): void {
    if (this.vsCodeApi) {
      this.vsCodeApi.setState(state);
    }
  }
}

// Export singleton instance
export const vscode = new VSCodeAPIWrapper();

// Message types
export interface WebviewMessage {
  type: string;
  data?: unknown;
  message?: string;
}

export type MessageHandler = (message: WebviewMessage) => void;

// Subscribe to messages from extension
export function onMessage(handler: MessageHandler): () => void {
  const listener = (event: MessageEvent<WebviewMessage>) => {
    handler(event.data);
  };
  window.addEventListener('message', listener);
  return () => window.removeEventListener('message', listener);
}

// API types (matching server types)
export interface ExperimentInfo {
  name: string;
  path: string;
  store: string;
  workloads: WorkloadMetadata[];
  /** Unix timestamp (seconds) of the most recent git commit touching the experiment path. */
  last_activity?: number | null;
}

export interface WorkloadMetadata {
  name: string;
}

export interface JobInfo {
  id: string;
  job_type: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';
  created_at: string;
  started_at?: string;
  completed_at?: string;
  error?: string;
  metadata: Record<string, unknown>;
}

export interface QueryResult {
  metrics: unknown[];
}

export interface ConfigInfo {
  etna_dir: string;
  store_path: string;
  experiments_path: string;
  configured: boolean;
  version: number;
}

export interface TestInfo {
  name: string;
}

export interface TestDefinition {
  workload: string;
  trials: number;
  timeout: number;
  mutations: string[];
  cross?: boolean;
  params?: Record<string, unknown>;
  tasks?: { strategy?: string; property?: string; [key: string]: string | undefined }[];
}
