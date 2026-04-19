import { useState, useEffect, useMemo, useRef, KeyboardEvent as ReactKeyboardEvent } from 'react';
import { vscode, ExperimentInfo, JobInfo } from '../api/vscodeApi';

interface Props {
  experiments: ExperimentInfo[];
  jobs: JobInfo[];
  loading: boolean;
  onRefresh: () => void;
  onOpenExperiment: (name: string) => void;
}

const ord = (n: number) => String(n + 1).padStart(2, '0');

function ExperimentsPage({ experiments, jobs, loading, onRefresh, onOpenExperiment }: Props) {
  const [showCreateForm, setShowCreateForm] = useState(false);
  const [newExperimentName, setNewExperimentName] = useState('');
  const createInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (showCreateForm) requestAnimationFrame(() => createInputRef.current?.focus());
  }, [showCreateForm]);

  const handleCreate = () => {
    if (!newExperimentName.trim()) return;
    vscode.postMessage({ type: 'createExperiment', name: newExperimentName.trim() });
    setNewExperimentName('');
    setShowCreateForm(false);
  };

  const handleDelete = (e: React.MouseEvent, name: string) => {
    e.stopPropagation();
    if (confirm(`Delete experiment "${name}"?`)) {
      vscode.postMessage({ type: 'deleteExperiment', name });
    }
  };

  // Index of running/pending job counts per experiment (from job metadata).
  const activeCounts = useMemo(() => {
    const m: Record<string, number> = {};
    for (const j of jobs) {
      if (j.status !== 'running' && j.status !== 'pending') continue;
      const name = (j.metadata as { experiment_name?: string })?.experiment_name;
      if (!name) continue;
      m[name] = (m[name] || 0) + 1;
    }
    return m;
  }, [jobs]);

  const sortedExperiments = useMemo(() => {
    return [...experiments].sort((a, b) => {
      const aT = a.last_activity ?? 0;
      const bT = b.last_activity ?? 0;
      return bT - aT;
    });
  }, [experiments]);

  if (loading) {
    return (
      <div className="ex-loading">
        <span className="ex-loading-dot" aria-hidden />
        <span>Fetching experiments…</span>
      </div>
    );
  }

  const onCreateKey = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') handleCreate();
    if (e.key === 'Escape') { setShowCreateForm(false); setNewExperimentName(''); }
  };

  return (
    <div className="ex-page">
      <header className="ex-pagehead">
        <div className="ex-pagehead-left">
          <span className="ex-kicker">Experiments</span>
          <p className="ex-pagelede">
            {experiments.length === 0
              ? <>Nothing yet — start by creating an experiment.</>
              : <>{experiments.length} registered · most recent first</>}
          </p>
        </div>
        <div className="ex-pagehead-right">
          <button className="ex-btn ex-btn-ghost" onClick={onRefresh} title="Refresh">
            <span className="ex-btn-glyph" aria-hidden>↻</span> Refresh
          </button>
          <button
            className="ex-btn ex-btn-primary"
            onClick={() => setShowCreateForm(s => !s)}
          >
            <span className="ex-btn-glyph" aria-hidden>+</span> New experiment
          </button>
        </div>
      </header>

      {showCreateForm && (
        <section className="ex-createcard">
          <div className="ex-createcard-head">
            <span className="ex-createcard-tag">NEW</span>
            <span className="ex-createcard-hint">Name your experiment, press <kbd>⏎</kbd> to create.</span>
          </div>
          <div className="ex-createcard-body">
            <input
              ref={createInputRef}
              className="ex-input ex-mono"
              type="text"
              value={newExperimentName}
              onChange={(e) => setNewExperimentName(e.target.value)}
              onKeyDown={onCreateKey}
              placeholder="my-experiment"
            />
            <div className="ex-createcard-actions">
              <button
                className="ex-btn ex-btn-ghost"
                onClick={() => { setShowCreateForm(false); setNewExperimentName(''); }}
              >Cancel</button>
              <button
                className="ex-btn ex-btn-primary"
                onClick={handleCreate}
                disabled={!newExperimentName.trim()}
              >Create</button>
            </div>
          </div>
        </section>
      )}

      {experiments.length === 0 ? (
        <div className="ex-empty">
          <div className="ex-empty-rune" aria-hidden>◇</div>
          <h3 className="ex-empty-title">No experiments yet</h3>
          <p className="ex-empty-sub">
            An experiment is a collection of tests you can run against the Etna benchmark workloads.
          </p>
          <button className="ex-btn ex-btn-primary" onClick={() => setShowCreateForm(true)}>
            <span className="ex-btn-glyph" aria-hidden>+</span> Create your first experiment
          </button>
        </div>
      ) : (
        <ul className="ex-list">
          {sortedExperiments.map((exp, i) => {
            const workloadCount = exp.workloads?.length || 0;
            const activity = formatRelativeTime(exp.last_activity);
            const pulseState = pulseFor(exp.last_activity);
            const active = activeCounts[exp.name] || 0;

            return (
              <li
                key={exp.name}
                className="ex-card is-openable"
                onClick={() => onOpenExperiment(exp.name)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    onOpenExperiment(exp.name);
                  }
                }}
                role="button"
                tabIndex={0}
              >
                <div className="ex-card-rail" aria-hidden />
                <div className="ex-card-main">
                  <span className="ex-card-index">{ord(i)}</span>
                  <div className="ex-card-body">
                    <div className="ex-card-topline">
                      <h3 className="ex-card-name">{exp.name}</h3>
                      <span
                        className={`ex-pulse ${pulseState}`}
                        title={formatAbsoluteTime(exp.last_activity)}
                      >
                        <span className="ex-pulse-dot" aria-hidden />
                        <span>{activity}</span>
                      </span>
                      {active > 0 && (
                        <span className="ex-active-pill" title={`${active} running or pending job${active === 1 ? '' : 's'}`}>
                          <span className="ex-active-pill-dot" aria-hidden />
                          {active} active
                        </span>
                      )}
                    </div>

                    <dl className="ex-statstrip">
                      <div className="ex-stat">
                        <dt>Workloads</dt>
                        <dd>{workloadCount.toString().padStart(2, '0')}</dd>
                      </div>
                      <div className="ex-stat ex-stat-path" title={exp.path}>
                        <dt>Path</dt>
                        <dd><code>{truncatePath(exp.path)}</code></dd>
                      </div>
                    </dl>
                  </div>

                  <div className="ex-card-actions">
                    <span className="ex-card-open" aria-hidden>Open →</span>
                    <button
                      className="ex-btn ex-btn-icondanger"
                      onClick={(e) => handleDelete(e, exp.name)}
                      aria-label={`Delete ${exp.name}`}
                      title={`Delete ${exp.name}`}
                    >×</button>
                  </div>
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

function formatRelativeTime(unixSeconds: number | null | undefined): string {
  if (unixSeconds == null) return 'no activity';
  const diff = Math.floor(Date.now() / 1000) - unixSeconds;
  if (diff < 60) return 'just now';
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  if (diff < 2592000) return `${Math.floor(diff / 86400)}d ago`;
  if (diff < 31536000) return `${Math.floor(diff / 2592000)}mo ago`;
  return `${Math.floor(diff / 31536000)}y ago`;
}

function formatAbsoluteTime(unixSeconds: number | null | undefined): string {
  if (unixSeconds == null) return 'No git history';
  return new Date(unixSeconds * 1000).toLocaleString();
}

function pulseFor(unixSeconds: number | null | undefined): string {
  if (unixSeconds == null) return 'is-cold';
  const diff = Math.floor(Date.now() / 1000) - unixSeconds;
  if (diff < 3600) return 'is-hot';
  if (diff < 86400) return 'is-warm';
  if (diff < 2592000) return 'is-mild';
  return 'is-cold';
}

function truncatePath(path: string, maxLength = 48): string {
  if (!path) return '—';
  if (path.length <= maxLength) return path;
  const parts = path.split('/');
  if (parts.length <= 2) return path;
  return `…/${parts.slice(-2).join('/')}`;
}

export default ExperimentsPage;
