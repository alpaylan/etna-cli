// Browser dev-mode shim: emulates the VS Code ↔ webview `postMessage` protocol
// by translating messages to direct HTTP calls against a running etna server.
// Activated automatically when `acquireVsCodeApi` is unavailable (i.e. `npm run dev`).

type Msg = { type: string; [k: string]: unknown };

export const SERVER_URL = (import.meta.env?.VITE_ETNA_SERVER_URL ?? 'http://localhost:3000').replace(/\/$/, '');

export function isBrowserMode(): boolean {
  return typeof (globalThis as { acquireVsCodeApi?: unknown }).acquireVsCodeApi !== 'function';
}

function dispatch(message: unknown): void {
  // Queue the event so listeners registered via window.addEventListener('message') pick it up.
  window.postMessage(message, window.location.origin);
}

async function req<T>(method: string, path: string, body?: unknown): Promise<T> {
  const opts: RequestInit = { method, headers: { 'Content-Type': 'application/json' } };
  if (body !== undefined) opts.body = JSON.stringify(body);
  const res = await fetch(`${SERVER_URL}${path}`, opts);
  if (!res.ok) {
    const text = await res.text().catch(() => '');
    let msg = text || res.statusText;
    try {
      const parsed = JSON.parse(text);
      if (parsed?.error) msg = parsed.error;
    } catch {
      // keep msg as raw text
    }
    throw new Error(msg);
  }
  if (res.status === 204) return undefined as unknown as T;
  const ctype = res.headers.get('content-type') ?? '';
  if (ctype.includes('application/json')) return (await res.json()) as T;
  return (await res.text()) as unknown as T;
}

export async function handleBrowserMessage(message: Msg): Promise<void> {
  try {
    switch (message.type) {
      case 'healthCheck': {
        const data = await req('GET', '/api/v1/health');
        dispatch({ type: 'healthCheck', data });
        break;
      }
      case 'getExperiments': {
        const data = await req('GET', '/api/v1/experiments');
        dispatch({ type: 'experiments', data });
        break;
      }
      case 'createExperiment': {
        await req('POST', '/api/v1/experiments', { name: message.name });
        const data = await req('GET', '/api/v1/experiments');
        dispatch({ type: 'experiments', data });
        break;
      }
      case 'cloneExperiment': {
        const url = message.url as string;
        const ref = message.ref as string | undefined;
        const body: Record<string, string> = { url };
        if (ref) body.ref = ref;
        await req('POST', '/api/v1/experiments/clone', body);
        const data = await req('GET', '/api/v1/experiments');
        dispatch({ type: 'experiments', data });
        break;
      }
      case 'deleteExperiment': {
        await req('DELETE', `/api/v1/experiments/${encodeURIComponent(message.name as string)}`);
        const data = await req('GET', '/api/v1/experiments');
        dispatch({ type: 'experiments', data });
        break;
      }
      case 'runExperiment': {
        const name = message.name as string;
        const tests = (message.tests as string[]) ?? [];
        await req('POST', `/api/v1/experiments/${encodeURIComponent(name)}/run`, { tests });
        const data = await req('GET', '/api/v1/jobs');
        dispatch({ type: 'jobs', data });
        break;
      }
      case 'getJobs': {
        const data = await req('GET', '/api/v1/jobs');
        dispatch({ type: 'jobs', data });
        break;
      }
      case 'getJob': {
        const data = await req('GET', `/api/v1/jobs/${encodeURIComponent(message.id as string)}`);
        dispatch({ type: 'job', data });
        break;
      }
      case 'cancelJob': {
        await req('POST', `/api/v1/jobs/${encodeURIComponent(message.id as string)}/cancel`);
        const data = await req('GET', '/api/v1/jobs');
        dispatch({ type: 'jobs', data });
        break;
      }
      case 'getJobLogs': {
        const id = message.id as string;
        const logs = await req('GET', `/api/v1/jobs/${encodeURIComponent(id)}/logs`);
        dispatch({ type: 'jobLogs', data: { id, logs } });
        break;
      }
      case 'getJobMetrics': {
        const id = message.id as string;
        const metrics = await req('GET', `/api/v1/jobs/${encodeURIComponent(id)}/metrics`);
        dispatch({ type: 'jobMetrics', data: { id, metrics } });
        break;
      }
      case 'getTests': {
        const experimentName = message.experimentName as string;
        const tests = await req('GET', `/api/v1/experiments/${encodeURIComponent(experimentName)}/tests`);
        dispatch({ type: 'tests', data: { experimentName, tests } });
        break;
      }
      case 'getTest': {
        const experimentName = message.experimentName as string;
        const testName = message.testName as string;
        const tests = await req(
          'GET',
          `/api/v1/experiments/${encodeURIComponent(experimentName)}/tests/${encodeURIComponent(testName)}`
        );
        dispatch({ type: 'testContent', data: { experimentName, testName, tests } });
        break;
      }
      case 'saveTest': {
        const experimentName = message.experimentName as string;
        const testName = message.testName as string;
        await req(
          'PUT',
          `/api/v1/experiments/${encodeURIComponent(experimentName)}/tests/${encodeURIComponent(testName)}`,
          message.tests
        );
        const allTests = await req('GET', `/api/v1/experiments/${encodeURIComponent(experimentName)}/tests`);
        dispatch({ type: 'tests', data: { experimentName, tests: allTests } });
        break;
      }
      case 'deleteTest': {
        const experimentName = message.experimentName as string;
        const testName = message.testName as string;
        await req(
          'DELETE',
          `/api/v1/experiments/${encodeURIComponent(experimentName)}/tests/${encodeURIComponent(testName)}`
        );
        const allTests = await req('GET', `/api/v1/experiments/${encodeURIComponent(experimentName)}/tests`);
        dispatch({ type: 'tests', data: { experimentName, tests: allTests } });
        break;
      }
      case 'queryMetrics': {
        const filter = message.filter as string | undefined;
        const experimentName = message.experimentName as string | undefined;
        const qs = new URLSearchParams();
        if (filter) qs.set('filter', filter);
        if (experimentName) qs.set('experiment', experimentName);
        const q = qs.toString();
        const data = await req('GET', `/api/v1/store/query${q ? `?${q}` : ''}`);
        dispatch({ type: 'queryResult', data });
        break;
      }
      case 'getConfig': {
        const data = await req('GET', '/api/v1/config');
        dispatch({ type: 'config', data });
        break;
      }
      case 'getWorkloads': {
        const experimentName = message.experimentName as string;
        const workloads = await req(
          'GET',
          `/api/v1/experiments/${encodeURIComponent(experimentName)}/workloads`
        );
        dispatch({ type: 'workloads', data: { experimentName, workloads } });
        break;
      }
      case 'getAvailableWorkloads': {
        const workloads = await req('GET', '/api/v1/workloads/available');
        dispatch({ type: 'availableWorkloads', data: { workloads } });
        break;
      }
      case 'addWorkload': {
        const experimentName = message.experimentName as string;
        const spec = (message.spec ?? message.url) as string;
        const ref = message.ref as string | undefined;
        const body: Record<string, string> = { spec };
        if (ref) body.ref = ref;
        await req(
          'POST',
          `/api/v1/experiments/${encodeURIComponent(experimentName)}/workloads`,
          body
        );
        const workloads = await req(
          'GET',
          `/api/v1/experiments/${encodeURIComponent(experimentName)}/workloads`
        );
        dispatch({ type: 'workloads', data: { experimentName, workloads } });
        break;
      }
      case 'refreshWorkloadIndex': {
        const data = await req('POST', '/api/v1/workloads/index/refresh');
        dispatch({ type: 'workloadIndexRefreshed', data });
        break;
      }
      case 'getWorkloadDetail': {
        const experimentName = message.experimentName as string;
        const workload = message.workload as string;
        try {
          const detail = await req(
            'GET',
            `/api/v1/experiments/${encodeURIComponent(experimentName)}/workloads/${encodeURIComponent(workload)}`,
          );
          dispatch({
            type: 'workloadDetail',
            data: { experimentName, workload, detail },
          });
        } catch (err) {
          const emsg = err instanceof Error ? err.message : String(err);
          dispatch({
            type: 'workloadDetail',
            data: { experimentName, workload, detail: null, error: emsg },
          });
        }
        break;
      }

      case 'removeWorkload': {
        const experimentName = message.experimentName as string;
        const workload = message.workload as string;
        await req(
          'DELETE',
          `/api/v1/experiments/${encodeURIComponent(experimentName)}/workloads/${encodeURIComponent(workload)}`
        );
        const workloads = await req(
          'GET',
          `/api/v1/experiments/${encodeURIComponent(experimentName)}/workloads`
        );
        dispatch({ type: 'workloads', data: { experimentName, workloads } });
        break;
      }
      case 'getWorkloadMutations': {
        const experimentName = message.experimentName as string;
        const workload = message.workload as string;
        try {
          const mutations = await req<string[]>(
            'GET',
            `/api/v1/experiments/${encodeURIComponent(experimentName)}/workloads/${encodeURIComponent(workload)}/mutations`
          );
          dispatch({ type: 'workloadMutations', data: { experimentName, workload, mutations } });
        } catch (err) {
          // Scope the failure to this workload so the editor can mark
          // suggestions as unavailable without tearing down the whole page.
          const emsg = err instanceof Error ? err.message : String(err);
          dispatch({ type: 'workloadMutations', data: { experimentName, workload, mutations: null, error: emsg } });
        }
        break;
      }
      default:
        dispatch({ type: 'error', message: `Unknown message type: ${message.type}` });
    }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    dispatch({ type: 'error', message: msg });
  }
}
