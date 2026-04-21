import { useState, useEffect, useCallback } from 'react';
import { vscode, onMessage, ExperimentInfo, JobInfo, QueryResult, WebviewMessage, TestInfo } from './api/vscodeApi';
import ExperimentsPage from './pages/ExperimentsPage';
import ExperimentWorkspace from './pages/ExperimentWorkspace';

type Sub = 'tests' | 'workloads' | 'dashboard' | 'jobs' | 'metrics';

function App() {
  const [selectedExperiment, setSelectedExperiment] = useState<string | null>(null);
  const [initialSub, setInitialSub] = useState<Sub | undefined>(undefined);
  const [experiments, setExperiments] = useState<ExperimentInfo[]>([]);
  const [jobs, setJobs] = useState<JobInfo[]>([]);
  const [queryResult, setQueryResult] = useState<QueryResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [experimentTests, setExperimentTests] = useState<Record<string, TestInfo[]>>({});

  const handleMessage = useCallback((message: WebviewMessage) => {
    switch (message.type) {
      case 'experiments':
        setExperiments(message.data as ExperimentInfo[] || []);
        setLoading(false);
        break;
      case 'jobs':
        setJobs(message.data as JobInfo[] || []);
        break;
      case 'queryResult':
        setQueryResult(message.data as QueryResult);
        break;
      case 'jobMetrics': {
        const metricsData = message.data as { id: string; metrics: QueryResult };
        setQueryResult(metricsData.metrics);
        const job = jobs.find(j => j.id === metricsData.id);
        const expName = (job?.metadata as { experiment_name?: string } | undefined)?.experiment_name;
        if (expName) {
          setSelectedExperiment(expName);
          setInitialSub('metrics');
        }
        break;
      }
      case 'tests': {
        const testsData = message.data as { experimentName: string; tests: TestInfo[] };
        setExperimentTests(prev => ({
          ...prev,
          [testsData.experimentName]: testsData.tests,
        }));
        break;
      }
      case 'error':
        setError(message.message || 'Unknown error');
        setTimeout(() => setError(null), 5000);
        break;
      case 'healthCheck':
        break;
    }
  }, [jobs]);

  useEffect(() => {
    const unsubscribe = onMessage(handleMessage);
    vscode.postMessage({ type: 'getExperiments' });
    vscode.postMessage({ type: 'getJobs' });
    vscode.postMessage({ type: 'healthCheck' });
    return unsubscribe;
  }, [handleMessage]);

  // Always poll jobs — the experiment list shows per-experiment active counts,
  // and the workspace shows an unread indicator, both of which need fresh data.
  useEffect(() => {
    const interval = setInterval(() => {
      vscode.postMessage({ type: 'getJobs' });
    }, 5000);
    return () => clearInterval(interval);
  }, []);

  const refreshExperiments = () => {
    vscode.postMessage({ type: 'getExperiments' });
  };

  const refreshJobs = () => {
    vscode.postMessage({ type: 'getJobs' });
  };

  const fetchTests = useCallback((experimentName: string) => {
    vscode.postMessage({ type: 'getTests', experimentName });
  }, []);

  const activeExperiment = selectedExperiment
    ? experiments.find(e => e.name === selectedExperiment) ?? null
    : null;

  // If the selected experiment disappears (e.g. deleted from another client),
  // fall back to the list rather than rendering an empty workspace.
  useEffect(() => {
    if (selectedExperiment && !activeExperiment && experiments.length > 0) {
      setSelectedExperiment(null);
    }
  }, [selectedExperiment, activeExperiment, experiments.length]);

  return (
    <div className="app">
      {!activeExperiment && <h1>Etna Dashboard</h1>}

      {error && <div className="error">{error}</div>}

      <div className="content">
        {activeExperiment ? (
          <ExperimentWorkspace
            experiment={activeExperiment}
            jobs={jobs}
            queryResult={queryResult}
            tests={experimentTests[activeExperiment.name] || []}
            onFetchTests={fetchTests}
            onRefreshJobs={refreshJobs}
            onBack={() => { setSelectedExperiment(null); setInitialSub(undefined); }}
            initialSub={initialSub}
          />
        ) : (
          <ExperimentsPage
            experiments={experiments}
            jobs={jobs}
            loading={loading}
            onRefresh={refreshExperiments}
            onOpenExperiment={(name) => { setSelectedExperiment(name); setInitialSub(undefined); }}
          />
        )}
      </div>
    </div>
  );
}

export default App;
