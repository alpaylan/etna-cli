import { getApiClient } from '../api/client';

interface WebviewMessage {
  type: string;
  [key: string]: unknown;
}

interface WebviewResponse {
  type: string;
  data?: unknown;
  message?: string;
}

export async function handleWebviewMessage(message: WebviewMessage): Promise<WebviewResponse | null> {
  const client = getApiClient();

  try {
    switch (message.type) {
      case 'healthCheck': {
        const result = await client.healthCheck();
        return { type: 'healthCheck', data: result };
      }

      case 'getExperiments': {
        const experiments = await client.listExperiments();
        return { type: 'experiments', data: experiments };
      }

      case 'createExperiment': {
        const name = message.name as string;
        await client.createExperiment({ name });
        // Return updated list
        const experiments = await client.listExperiments();
        return { type: 'experiments', data: experiments };
      }

      case 'cloneExperiment': {
        const url = message.url as string;
        const ref = message.ref as string | undefined;
        // No `path` — server defaults to the managed `<etna-dir>/experiments/` dir.
        await client.cloneExperiment({ url, ref });
        const experiments = await client.listExperiments();
        return { type: 'experiments', data: experiments };
      }

      case 'deleteExperiment': {
        const name = message.name as string;
        await client.deleteExperiment(name);
        // Return updated list
        const experiments = await client.listExperiments();
        return { type: 'experiments', data: experiments };
      }

      case 'runExperiment': {
        const name = message.name as string;
        const tests = (message.tests as string[]) || [];
        const result = await client.runExperiment(name, { tests });
        // Return job info and updated jobs list
        const jobs = await client.listJobs();
        return { type: 'jobs', data: jobs };
      }

      case 'getJobs': {
        const jobs = await client.listJobs();
        return { type: 'jobs', data: jobs };
      }

      case 'getTests': {
        const experimentName = message.experimentName as string;
        const tests = await client.listTests(experimentName);
        return { type: 'tests', data: { experimentName, tests } };
      }

      case 'getJob': {
        const id = message.id as string;
        const job = await client.getJob(id);
        return { type: 'job', data: job };
      }

      case 'cancelJob': {
        const id = message.id as string;
        await client.cancelJob(id);
        // Return updated jobs list
        const jobs = await client.listJobs();
        return { type: 'jobs', data: jobs };
      }

      case 'getJobLogs': {
        const id = message.id as string;
        const logs = await client.getJobLogs(id);
        return { type: 'jobLogs', data: { id, logs } };
      }

      case 'getJobMetrics': {
        const id = message.id as string;
        const result = await client.getJobMetrics(id);
        return { type: 'jobMetrics', data: { id, metrics: result } };
      }

      case 'getTest': {
        const experimentName = message.experimentName as string;
        const testName = message.testName as string;
        const tests = await client.getTest(experimentName, testName);
        return { type: 'testContent', data: { experimentName, testName, tests } };
      }

      case 'saveTest': {
        const experimentName = message.experimentName as string;
        const testName = message.testName as string;
        const tests = message.tests as unknown[];
        await client.saveTest(experimentName, testName, tests as never[]);
        // Return updated tests list
        const allTests = await client.listTests(experimentName);
        return { type: 'tests', data: { experimentName, tests: allTests } };
      }

      case 'deleteTest': {
        const experimentName = message.experimentName as string;
        const testName = message.testName as string;
        await client.deleteTest(experimentName, testName);
        // Return updated tests list
        const allTests = await client.listTests(experimentName);
        return { type: 'tests', data: { experimentName, tests: allTests } };
      }

      case 'queryMetrics': {
        const filter = message.filter as string | undefined;
        const experimentName = message.experimentName as string | undefined;
        const result = await client.queryMetrics(filter, experimentName);
        return { type: 'queryResult', data: result };
      }

      case 'getConfig': {
        const config = await client.getConfig();
        return { type: 'config', data: config };
      }

      case 'getWorkloadMutations': {
        const experimentName = message.experimentName as string;
        const workload = message.workload as string;
        const mutations = await client.listWorkloadMutations(experimentName, workload);
        return { type: 'workloadMutations', data: { experimentName, workload, mutations } };
      }

      case 'getWorkloads': {
        const experimentName = message.experimentName as string;
        const workloads = await client.listWorkloads(experimentName);
        return { type: 'workloads', data: { experimentName, workloads } };
      }

      case 'getAvailableWorkloads': {
        const workloads = await client.listAvailableWorkloads();
        return { type: 'availableWorkloads', data: { workloads } };
      }

      case 'addWorkload': {
        const experimentName = message.experimentName as string;
        // Accept `spec` (catalog name or URL) with `url` as a legacy alias
        // from older webview builds.
        const spec = (message.spec ?? message.url) as string;
        const ref = message.ref as string | undefined;
        await client.addWorkload(experimentName, { spec, ref });
        const workloads = await client.listWorkloads(experimentName);
        return { type: 'workloads', data: { experimentName, workloads } };
      }

      case 'refreshWorkloadIndex': {
        const result = await client.refreshWorkloadIndex();
        return { type: 'workloadIndexRefreshed', data: result };
      }

      case 'removeWorkload': {
        const experimentName = message.experimentName as string;
        const workload = message.workload as string;
        await client.removeWorkload(experimentName, workload);
        const workloads = await client.listWorkloads(experimentName);
        return { type: 'workloads', data: { experimentName, workloads } };
      }

      default:
        return { type: 'error', message: `Unknown message type: ${message.type}` };
    }
  } catch (error) {
    const errorMessage = error instanceof Error ? error.message : 'Unknown error';
    return { type: 'error', message: errorMessage };
  }
}
