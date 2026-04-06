// Types matching the Etna server API

export interface ExperimentInfo {
  name: string;
  path: string;
  store: string;
  workloads: WorkloadMetadata[];
}

export interface WorkloadMetadata {
  name: string;
  language: string;
}

export interface CreateExperimentRequest {
  name: string;
  path?: string;
  overwrite?: boolean;
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
  repo_dir: string;
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

// Add workload request
export interface AddWorkloadRequest {
  language: string;
  name: string;
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
  language: string;
  workload: string;
  trials: number;
  timeout: number;
  mutations: string[];
  cross?: boolean;
  params?: Record<string, unknown>;
  tasks?: { strategy?: string; property?: string; [key: string]: string | undefined }[];
}
