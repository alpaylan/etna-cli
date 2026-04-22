import axios, { AxiosInstance, AxiosError } from 'axios';
import * as vscode from 'vscode';
import {
  ExperimentInfo,
  CreateExperimentRequest,
  CloneExperimentRequest,
  RunExperimentRequest,
  RunExperimentResponse,
  JobInfo,
  QueryResult,
  ConfigInfo,
  FileMutationsInfo,
  SetMutationRequest,
  ResetMutationsRequest,
  MutationOperationResponse,
  AddWorkloadRequest,
  HealthResponse,
  ApiError,
  TestInfo,
  TestDefinition,
  WorkloadMetadata,
  WorkloadEntry,
  RefreshWorkloadIndexResponse,
  WorkloadDetailResponse,
} from './types';

export class EtnaApiClient {
  private client: AxiosInstance;
  private baseUrl: string;

  constructor() {
    this.baseUrl = this.getServerUrl();
    this.client = axios.create({
      baseURL: this.baseUrl,
      timeout: 30000,
      headers: {
        'Content-Type': 'application/json',
      },
    });

    // Listen for configuration changes
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration('etna.serverUrl')) {
        this.updateBaseUrl();
      }
    });
  }

  private getServerUrl(): string {
    const config = vscode.workspace.getConfiguration('etna');
    return config.get<string>('serverUrl', 'http://localhost:3000');
  }

  private updateBaseUrl(): void {
    this.baseUrl = this.getServerUrl();
    this.client.defaults.baseURL = this.baseUrl;
  }

  private handleError(error: unknown): never {
    if (axios.isAxiosError(error)) {
      const axiosError = error as AxiosError<ApiError>;
      if (axiosError.response?.data?.error) {
        throw new Error(axiosError.response.data.error);
      }
      if (axiosError.code === 'ECONNREFUSED') {
        throw new Error(`Cannot connect to Etna server at ${this.baseUrl}. Is it running?`);
      }
      throw new Error(axiosError.message);
    }
    throw error;
  }

  // Health check
  async healthCheck(): Promise<HealthResponse> {
    try {
      const response = await this.client.get<HealthResponse>('/api/v1/health');
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  // Experiments
  async listExperiments(): Promise<ExperimentInfo[]> {
    try {
      const response = await this.client.get<ExperimentInfo[]>('/api/v1/experiments');
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async getExperiment(name: string): Promise<ExperimentInfo> {
    try {
      const response = await this.client.get<ExperimentInfo>(`/api/v1/experiments/${encodeURIComponent(name)}`);
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async listTests(experimentName: string): Promise<TestInfo[]> {
    try {
      const response = await this.client.get<TestInfo[]>(
        `/api/v1/experiments/${encodeURIComponent(experimentName)}/tests`
      );
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async createExperiment(request: CreateExperimentRequest): Promise<{ experiment: ExperimentInfo }> {
    try {
      const response = await this.client.post<{ experiment: ExperimentInfo }>('/api/v1/experiments', request);
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async cloneExperiment(request: CloneExperimentRequest): Promise<{ experiment: ExperimentInfo }> {
    try {
      const response = await this.client.post<{ experiment: ExperimentInfo }>(
        '/api/v1/experiments/clone',
        request,
      );
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async deleteExperiment(name: string): Promise<void> {
    try {
      await this.client.delete(`/api/v1/experiments/${encodeURIComponent(name)}`);
    } catch (error) {
      this.handleError(error);
    }
  }

  async runExperiment(name: string, request: RunExperimentRequest): Promise<RunExperimentResponse> {
    try {
      const response = await this.client.post<RunExperimentResponse>(
        `/api/v1/experiments/${encodeURIComponent(name)}/run`,
        request
      );
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  // Workloads
  async listWorkloads(experimentName: string): Promise<WorkloadMetadata[]> {
    try {
      const response = await this.client.get<WorkloadMetadata[]>(
        `/api/v1/experiments/${encodeURIComponent(experimentName)}/workloads`
      );
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async listAvailableWorkloads(): Promise<WorkloadEntry[]> {
    try {
      const response = await this.client.get<WorkloadEntry[]>('/api/v1/workloads/available');
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async refreshWorkloadIndex(): Promise<RefreshWorkloadIndexResponse> {
    try {
      const response = await this.client.post<RefreshWorkloadIndexResponse>(
        '/api/v1/workloads/index/refresh'
      );
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async addWorkload(experimentName: string, request: AddWorkloadRequest): Promise<void> {
    try {
      await this.client.post(
        `/api/v1/experiments/${encodeURIComponent(experimentName)}/workloads`,
        request
      );
    } catch (error) {
      this.handleError(error);
    }
  }

  async getWorkloadDetail(
    experimentName: string,
    workloadName: string,
  ): Promise<WorkloadDetailResponse> {
    try {
      const response = await this.client.get<WorkloadDetailResponse>(
        `/api/v1/experiments/${encodeURIComponent(experimentName)}/workloads/${encodeURIComponent(workloadName)}`,
      );
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async removeWorkload(experimentName: string, workloadName: string): Promise<void> {
    try {
      await this.client.delete(
        `/api/v1/experiments/${encodeURIComponent(experimentName)}/workloads/${encodeURIComponent(workloadName)}`
      );
    } catch (error) {
      this.handleError(error);
    }
  }

  // Jobs
  async listJobs(): Promise<JobInfo[]> {
    try {
      const response = await this.client.get<JobInfo[]>('/api/v1/jobs');
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async getJob(id: string): Promise<JobInfo> {
    try {
      const response = await this.client.get<JobInfo>(`/api/v1/jobs/${encodeURIComponent(id)}`);
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async cancelJob(id: string): Promise<void> {
    try {
      await this.client.post(`/api/v1/jobs/${encodeURIComponent(id)}/cancel`);
    } catch (error) {
      this.handleError(error);
    }
  }

  async getJobLogs(id: string): Promise<string[]> {
    try {
      const response = await this.client.get<string[]>(`/api/v1/jobs/${encodeURIComponent(id)}/logs`);
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async getJobMetrics(id: string): Promise<QueryResult> {
    try {
      const response = await this.client.get<QueryResult>(`/api/v1/jobs/${encodeURIComponent(id)}/metrics`);
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  // Test CRUD
  async getTest(experimentName: string, testName: string): Promise<TestDefinition[]> {
    try {
      const response = await this.client.get<TestDefinition[]>(
        `/api/v1/experiments/${encodeURIComponent(experimentName)}/tests/${encodeURIComponent(testName)}`
      );
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async saveTest(experimentName: string, testName: string, tests: TestDefinition[]): Promise<void> {
    try {
      await this.client.put(
        `/api/v1/experiments/${encodeURIComponent(experimentName)}/tests/${encodeURIComponent(testName)}`,
        tests
      );
    } catch (error) {
      this.handleError(error);
    }
  }

  async deleteTest(experimentName: string, testName: string): Promise<void> {
    try {
      await this.client.delete(
        `/api/v1/experiments/${encodeURIComponent(experimentName)}/tests/${encodeURIComponent(testName)}`
      );
    } catch (error) {
      this.handleError(error);
    }
  }

  // Store
  async queryMetrics(filter?: string, experimentName?: string): Promise<QueryResult> {
    try {
      const params: Record<string, string> = {};
      if (filter) params.filter = filter;
      if (experimentName) params.experiment = experimentName;
      const response = await this.client.get<QueryResult>('/api/v1/store/query', { params });
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  // Config
  async getConfig(): Promise<ConfigInfo> {
    try {
      const response = await this.client.get<ConfigInfo>('/api/v1/config');
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  // Mutations
  async listWorkloadMutations(experimentName: string, workload: string): Promise<string[]> {
    try {
      const response = await this.client.get<string[]>(
        `/api/v1/experiments/${encodeURIComponent(experimentName)}/workloads/${encodeURIComponent(workload)}/mutations`
      );
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async listMutations(path: string): Promise<FileMutationsInfo[]> {
    try {
      const response = await this.client.get<FileMutationsInfo[]>('/api/v1/mutations', {
        params: { path },
      });
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async getFileMutations(filePath: string): Promise<FileMutationsInfo> {
    try {
      const response = await this.client.get<FileMutationsInfo>('/api/v1/mutations/file', {
        params: { path: filePath },
      });
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async setMutation(request: SetMutationRequest): Promise<MutationOperationResponse> {
    try {
      const response = await this.client.post<MutationOperationResponse>('/api/v1/mutations/set', request);
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }

  async resetMutations(request: ResetMutationsRequest): Promise<MutationOperationResponse> {
    try {
      const response = await this.client.post<MutationOperationResponse>('/api/v1/mutations/reset', request);
      return response.data;
    } catch (error) {
      this.handleError(error);
    }
  }
}

// Singleton instance
let apiClient: EtnaApiClient | undefined;

export function getApiClient(): EtnaApiClient {
  if (!apiClient) {
    apiClient = new EtnaApiClient();
  }
  return apiClient;
}
