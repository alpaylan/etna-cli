import { handleBrowserMessage, isBrowserMode } from './browserShim';
import { handleStaticMessage, isStaticMode } from './staticShim';

// Type declarations for VSCode webview API
declare function acquireVsCodeApi(): VsCodeApi;

interface VsCodeApi {
  postMessage(message: unknown): void;
  getState(): unknown;
  setState(state: unknown): void;
}

type Transport = 'vscode' | 'static' | 'browser' | 'none';

// Singleton pattern for VSCode API
class VSCodeAPIWrapper {
  private readonly vsCodeApi: VsCodeApi | undefined;
  private readonly transport: Transport;

  constructor() {
    if (typeof acquireVsCodeApi === 'function') {
      this.vsCodeApi = acquireVsCodeApi();
    }
    if (this.vsCodeApi) {
      this.transport = 'vscode';
    } else if (isStaticMode()) {
      this.transport = 'static';
      console.info('[etna] Static mode — reading catalog JSON from ./data/.');
    } else if (isBrowserMode()) {
      this.transport = 'browser';
      console.info('[etna] Browser dev mode — talking to server directly.');
    } else {
      this.transport = 'none';
    }
  }

  public postMessage(message: unknown): void {
    const msg = message as { type: string; [k: string]: unknown };
    switch (this.transport) {
      case 'vscode':
        this.vsCodeApi!.postMessage(message);
        return;
      case 'static':
        void handleStaticMessage(msg);
        return;
      case 'browser':
        // Fire-and-forget; the shim dispatches the response back as a window message.
        void handleBrowserMessage(msg);
        return;
      case 'none':
        console.log('VSCode API not available, message:', message);
    }
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

export interface WorkloadManifest {
  name: string;
  description?: string | null;
  language: string;
  crate?: string | null;
  base_commit?: string | null;
  tasks: ManifestTaskGroup[];
  dropped?: DroppedCandidate[];
}

export interface ManifestTaskGroup {
  mutations: string[];
  tasks: ManifestTask[];
  source?: SourceContext | null;
  injection?: InjectionSpec | null;
  bug?: BugDetails | null;
}

export interface ManifestTask {
  property: string;
  witnesses?: Witness[];
}

export type Witness =
  | { input: string; note?: string | null }
  | { test_fn: string; note?: string | null };

export interface SourceContext {
  repo: string;
  commits: string[];
  commit_subjects?: string[];
  prs?: number[];
  issues?: number[];
  discussion?: string | null;
  origin?: string | null;
  summary: string;
}

export interface InjectionSpec {
  kind: 'marauders' | 'patch';
  files: string[];
  locations?: FileLoc[];
  patch?: string | null;
}

export interface FileLoc {
  file: string;
  line?: number | null;
  symbol?: string | null;
}

export interface BugDetails {
  short_name: string;
  invariant: string;
  how_triggered: string;
}

export interface DroppedCandidate {
  commit: string;
  reason: string;
  subject?: string | null;
}

export interface WorkloadDetail {
  manifest: WorkloadManifest;
  readme_md?: string | null;
  bugs_md?: string | null;
  tasks_md?: string | null;
  patches?: Record<string, string>;
}

/** One row from `/api/v1/workloads/available`. */
export interface WorkloadEntry {
  name: string;
  url: string;
  language: string;
  description?: string | null;
  default_ref?: string | null;
  status: string;
  tags: string[];
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
