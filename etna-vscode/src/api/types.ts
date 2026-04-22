// Types matching the Etna server API

export interface ExperimentInfo {
  name: string;
  path: string;
  store: string;
  workloads: WorkloadMetadata[];
}

export interface WorkloadMetadata {
  name: string;
}

// Entry in the workload catalog (`/api/v1/workloads/available`). Matches
// `src/workload_index.rs::WorkloadEntry` on the Rust side.
export interface WorkloadEntry {
  name: string;
  url: string;
  language: string;
  description?: string | null;
  default_ref?: string | null;
  status: string;
  tags: string[];
}

export interface RefreshWorkloadIndexResponse {
  refreshed: boolean;
  entries: number;
}

// Parsed `etna.toml`. Matches `src/workload.rs::WorkloadManifest`.
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

// Shape returned by `GET /api/v1/experiments/{name}/workloads/{wl}`. Matches
// `src/server/handlers/workloads.rs::WorkloadDetailResponse`.
export interface WorkloadDetailResponse {
  manifest: WorkloadManifest;
  readme_md?: string | null;
  bugs_md?: string | null;
  tasks_md?: string | null;
  /** Contents of every `.patch` file referenced by `injection.patch`. */
  patches: Record<string, string>;
}

export interface CreateExperimentRequest {
  name: string;
  path?: string;
  overwrite?: boolean;
}

// Clone a remote experiment repo. Field names mirror the server payload
// (`src/server/handlers/experiments.rs::CloneExperimentRequest`).
export interface CloneExperimentRequest {
  url: string;
  ref?: string;
  path?: string;
}

export interface RunExperimentRequest {
  tests: string[];
  short_circuit?: boolean;
  parallel?: boolean;
  params?: [string, string][];
}

export interface RunExperimentResponse {
  job_id: string;
  status: string;
}

export interface JobInfo {
  id: string;
  job_type: string;
  status: JobStatus;
  created_at: string;
  started_at?: string;
  completed_at?: string;
  error?: string;
  metadata: Record<string, unknown>;
}

export type JobStatus = 'pending' | 'running' | 'completed' | 'failed' | 'cancelled';

export interface QueryResult {
  metrics: unknown[];
}

export interface WriteMetricRequest {
  hash: string;
  data: Record<string, unknown>;
}

export interface ConfigInfo {
  etna_dir: string;
  store_path: string;
  experiments_path: string;
  configured: boolean;
  version: number;
}

// Mutations API types

export interface MutationInfo {
  name: string;
  active: boolean;
  file: string;
  line: number;
  end_line: number;
}

export interface FileMutationsInfo {
  file: string;
  mutations: MutationInfo[];
}

export interface SetMutationRequest {
  path: string;
  variant: string;
  glob?: string;
}

export interface ResetMutationsRequest {
  path: string;
}

export interface MutationOperationResponse {
  success: boolean;
  message: string;
}

// Add workload request — field names must match the server handler payload
// (`src/server/handlers/workloads.rs::AddWorkloadRequest`). `spec` may be a
// catalog name or a git URL. `url` is still accepted as an alias server-side
// for older callers.
export interface AddWorkloadRequest {
  spec: string;
  ref?: string;
}

// API error response
export interface ApiError {
  error: string;
}

// Health check response
export interface HealthResponse {
  status: string;
}

// Test info
export interface TestInfo {
  name: string;
}

// Full test definition for editing
export interface TestDefinition {
  workload: string;
  trials: number;
  timeout: number;
  mutations: string[];
  cross?: boolean;
  params?: Record<string, unknown>;
  tasks?: { strategy?: string; property?: string; [key: string]: string | undefined }[];
}
