import { useState, useEffect, useMemo, useCallback } from 'react';
import { vscode, ExperimentInfo, JobInfo, QueryResult, TestInfo } from '../api/vscodeApi';
import TestsPanel from './workspace/TestsPanel';
import JobsPage from './JobsPage';
import MetricsPage from './MetricsPage';
import ExperimentDashboard from '../components/ExperimentDashboard';

type Sub = 'tests' | 'dashboard' | 'jobs' | 'metrics';

interface Props {
  experiment: ExperimentInfo;
  jobs: JobInfo[];
  queryResult: QueryResult | null;
  tests: TestInfo[];
  onFetchTests: (experimentName: string) => void;
  onRefreshJobs: () => void;
  onBack: () => void;
  initialSub?: Sub;
}

const SEEN_KEY_PREFIX = 'etna.jobsLastSeenAt::';

function readSeen(name: string): number {
  try {
    const raw = localStorage.getItem(SEEN_KEY_PREFIX + name);
    const n = raw ? parseInt(raw, 10) : NaN;
    return Number.isFinite(n) ? n : Date.now();
  } catch {
    return Date.now();
  }
}

function writeSeen(name: string, ts: number) {
  try { localStorage.setItem(SEEN_KEY_PREFIX + name, String(ts)); } catch { /* ignore */ }
}

function latestJobTimestamp(jobs: JobInfo[]): number {
  let max = 0;
  for (const j of jobs) {
    for (const s of [j.created_at, j.started_at, j.completed_at]) {
      if (!s) continue;
      const t = Date.parse(s);
      if (Number.isFinite(t) && t > max) max = t;
    }
  }
  return max;
}

function ExperimentWorkspace({
  experiment,
  jobs: allJobs,
  queryResult,
  tests,
  onFetchTests,
  onRefreshJobs,
  onBack,
  initialSub,
}: Props) {
  const [sub, setSub] = useState<Sub>(initialSub ?? 'tests');

  useEffect(() => {
    if (initialSub) setSub(initialSub);
  }, [initialSub]);
  const [seenAt, setSeenAt] = useState<number>(() => readSeen(experiment.name));
  const [runToast, setRunToast] = useState<{ count: number; ts: number } | null>(null);

  // Jobs for this experiment, via metadata.experiment_name.
  const jobs = useMemo(
    () => allJobs.filter(j => (j.metadata as { experiment_name?: string })?.experiment_name === experiment.name),
    [allJobs, experiment.name]
  );

  const activeCount = useMemo(
    () => jobs.filter(j => j.status === 'running' || j.status === 'pending').length,
    [jobs]
  );

  const latestAt = useMemo(() => latestJobTimestamp(jobs), [jobs]);
  const hasUnseen = sub !== 'jobs' && latestAt > seenAt;

  useEffect(() => {
    if (sub !== 'jobs') return;
    const now = Date.now();
    setSeenAt(now);
    writeSeen(experiment.name, now);
  }, [sub, jobs, experiment.name]);

  useEffect(() => {
    if (!runToast) return;
    const t = setTimeout(() => setRunToast(null), 3500);
    return () => clearTimeout(t);
  }, [runToast]);

  const handleQueuedRun = useCallback((count: number) => {
    setRunToast({ count, ts: Date.now() });
    // Fetch jobs promptly so the Jobs tab reflects the new queued work.
    vscode.postMessage({ type: 'getJobs' });
  }, []);

  return (
    <div className="ws-page">
      {runToast && (
        <div className="ex-runtoast" role="status" aria-live="polite" key={runToast.ts}>
          <span className="ex-runtoast-check" aria-hidden>✓</span>
          <div className="ex-runtoast-body">
            <span className="ex-runtoast-title">
              Queued {runToast.count} test{runToast.count === 1 ? '' : 's'}
            </span>
            <span className="ex-runtoast-sub">
              <code>{experiment.name}</code> — track progress in the <strong>Jobs</strong> sub-tab
            </span>
          </div>
          <button
            type="button"
            className="ex-runtoast-close"
            onClick={() => setRunToast(null)}
            aria-label="Dismiss"
          >×</button>
        </div>
      )}

      <header className="ws-head">
        <button className="ex-btn ex-btn-ghost ws-back" onClick={onBack}>
          <span className="ex-btn-glyph" aria-hidden>←</span> Experiments
        </button>
        <div className="ws-crumb">
          <span className="ws-crumb-tag">Experiment</span>
          <span className="ws-crumb-value">{experiment.name}</span>
        </div>
      </header>

      <nav className="ws-subtabs" role="tablist" aria-label="Experiment views">
        <button
          role="tab"
          aria-selected={sub === 'tests'}
          className={`ws-subtab ${sub === 'tests' ? 'is-active' : ''}`}
          onClick={() => setSub('tests')}
        >
          <span className="ws-subtab-ord">01</span>
          <span className="ws-subtab-label">Tests</span>
          {tests.length > 0 && <span className="ws-subtab-count">{tests.length}</span>}
        </button>
        <button
          role="tab"
          aria-selected={sub === 'dashboard'}
          className={`ws-subtab ${sub === 'dashboard' ? 'is-active' : ''}`}
          onClick={() => setSub('dashboard')}
        >
          <span className="ws-subtab-ord">02</span>
          <span className="ws-subtab-label">Dashboard</span>
        </button>
        <button
          role="tab"
          aria-selected={sub === 'jobs'}
          className={`ws-subtab ${sub === 'jobs' ? 'is-active' : ''} ${hasUnseen ? 'has-unseen' : ''}`}
          onClick={() => setSub('jobs')}
        >
          <span className="ws-subtab-ord">03</span>
          <span className="ws-subtab-label">Jobs</span>
          {activeCount > 0 && <span className="ws-subtab-count is-active">{activeCount}</span>}
          {hasUnseen && <span className="ws-subtab-unseen" aria-hidden />}
        </button>
        <button
          role="tab"
          aria-selected={sub === 'metrics'}
          className={`ws-subtab ${sub === 'metrics' ? 'is-active' : ''}`}
          onClick={() => setSub('metrics')}
        >
          <span className="ws-subtab-ord">04</span>
          <span className="ws-subtab-label">Metrics</span>
        </button>
      </nav>

      <div className="ws-body">
        {sub === 'tests' && (
          <TestsPanel
            experimentName={experiment.name}
            tests={tests}
            onFetchTests={onFetchTests}
            onQueuedRun={handleQueuedRun}
          />
        )}
        {sub === 'dashboard' && (
          <ExperimentDashboard
            experimentName={experiment.name}
            onBack={() => setSub('tests')}
          />
        )}
        {sub === 'jobs' && (
          <JobsPage
            jobs={allJobs}
            onRefresh={onRefreshJobs}
            experimentName={experiment.name}
          />
        )}
        {sub === 'metrics' && (
          <MetricsPage
            queryResult={queryResult}
            experimentName={experiment.name}
          />
        )}
      </div>
    </div>
  );
}

export default ExperimentWorkspace;
