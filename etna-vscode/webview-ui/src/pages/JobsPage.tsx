import { useState, useEffect, useRef } from 'react';
import { vscode, JobInfo, onMessage } from '../api/vscodeApi';

interface Props {
  jobs: JobInfo[];
  onRefresh: () => void;
  experimentName?: string;
}

const ord = (n: number) => String(n + 1).padStart(2, '0');

const ERROR_PREVIEW_LIMIT = 240;
const ERROR_PREVIEW_LINES = 3;

function truncateError(text: string): { preview: string; truncated: boolean } {
  const lines = text.split('\n');
  if (text.length <= ERROR_PREVIEW_LIMIT && lines.length <= ERROR_PREVIEW_LINES) {
    return { preview: text, truncated: false };
  }
  const byLine = lines.slice(0, ERROR_PREVIEW_LINES).join('\n');
  const capped = byLine.length > ERROR_PREVIEW_LIMIT ? byLine.slice(0, ERROR_PREVIEW_LIMIT).trimEnd() + '…' : byLine;
  return { preview: capped, truncated: true };
}

function JobsPage({ jobs: allJobs, onRefresh, experimentName }: Props) {
  const jobs = experimentName
    ? allJobs.filter(j => (j.metadata as { experiment_name?: string })?.experiment_name === experimentName)
    : allJobs;
  const [expandedJob, setExpandedJob] = useState<string | null>(null);
  const [jobLogs, setJobLogs] = useState<Record<string, string[]>>({});
  const [loadingLogs, setLoadingLogs] = useState<string | null>(null);
  const [expandedErrors, setExpandedErrors] = useState<Set<string>>(new Set());
  const logsEndRef = useRef<HTMLDivElement>(null);

  const toggleError = (id: string) => {
    setExpandedErrors(prev => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id); else next.add(id);
      return next;
    });
  };

  useEffect(() => {
    const unsubscribe = onMessage((message) => {
      if (message.type === 'jobLogs') {
        const { id, logs } = message.data as { id: string; logs: string[] };
        setJobLogs(prev => ({ ...prev, [id]: logs }));
        setLoadingLogs(null);
      }
    });
    return unsubscribe;
  }, []);

  // Auto-scroll logs to bottom on update
  useEffect(() => {
    if (expandedJob && logsEndRef.current) {
      logsEndRef.current.scrollIntoView({ behavior: 'smooth' });
    }
  }, [jobLogs, expandedJob]);

  const handleCancel = (id: string) => {
    if (confirm('Cancel this job?')) {
      vscode.postMessage({ type: 'cancelJob', id });
    }
  };

  const handleViewLogs = (id: string) => {
    if (expandedJob === id) {
      setExpandedJob(null);
    } else {
      setExpandedJob(id);
      setLoadingLogs(id);
      vscode.postMessage({ type: 'getJobLogs', id });
    }
  };

  const refreshLogs = (id: string) => {
    setLoadingLogs(id);
    vscode.postMessage({ type: 'getJobLogs', id });
  };

  const handleViewMetrics = (id: string) => {
    vscode.postMessage({ type: 'getJobMetrics', id });
  };

  const sortedJobs = [...jobs].sort((a, b) =>
    new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
  );

  const runningCount = jobs.filter(j => j.status === 'running' || j.status === 'pending').length;

  return (
    <div className="jp-page">
      <header className="jp-pagehead">
        <div className="jp-pagehead-left">
          <span className="jp-kicker">Jobs</span>
          <p className="jp-lede">
            {jobs.length === 0
              ? <>No jobs yet — run an experiment to queue one.</>
              : <>
                  {jobs.length} registered
                  {runningCount > 0 && <> · <strong className="jp-active">{runningCount} active</strong></>}
                  <span className="jp-tick" title="Auto-refreshes every 5s"> · auto-refresh</span>
                </>}
          </p>
        </div>
        <button className="ex-btn ex-btn-ghost" onClick={onRefresh} title="Force refresh">
          <span className="ex-btn-glyph" aria-hidden>↻</span> Refresh
        </button>
      </header>

      {sortedJobs.length === 0 ? (
        <div className="ex-empty">
          <div className="ex-empty-rune" aria-hidden>⌘</div>
          <h3 className="ex-empty-title">No jobs queued</h3>
          <p className="ex-empty-sub">Jobs appear here when you run an experiment. They keep their logs and metrics for inspection.</p>
        </div>
      ) : (
        <ul className="jp-list">
          {sortedJobs.map((job, i) => {
            const isExpanded = expandedJob === job.id;
            const logs = jobLogs[job.id] || [];
            const isActive = job.status === 'running' || job.status === 'pending';

            return (
              <li key={job.id} className={`jp-card ${isExpanded ? 'is-open' : ''}`} data-status={job.status}>
                <div className="jp-card-rail" aria-hidden />

                <div className="jp-card-main">
                  <span className="jp-card-index">{ord(i)}</span>

                  <div className="jp-card-body">
                    <div className="jp-topline">
                      <span className="jp-status" data-status={job.status}>
                        <span className="jp-status-dot" aria-hidden />
                        <span>{job.status}</span>
                      </span>
                      <span className="jp-type">{job.job_type}</span>
                      <code className="jp-id" title={job.id}>{job.id.substring(0, 8)}</code>
                    </div>

                    <dl className="jp-statstrip">
                      <div className="jp-stat">
                        <dt>Duration</dt>
                        <dd>{formatDuration(job)}</dd>
                      </div>
                      <div className="jp-stat">
                        <dt>Created</dt>
                        <dd title={formatAbsoluteTime(job.created_at)}>{formatRelativeTime(job.created_at)}</dd>
                      </div>
                      {job.started_at && (
                        <div className="jp-stat">
                          <dt>Started</dt>
                          <dd title={formatAbsoluteTime(job.started_at)}>{formatRelativeTime(job.started_at)}</dd>
                        </div>
                      )}
                    </dl>

                    {job.error && (() => {
                      const isOpen = expandedErrors.has(job.id);
                      const { preview, truncated } = truncateError(job.error);
                      return (
                        <div className={`jp-error ${truncated ? 'is-truncatable' : ''} ${isOpen ? 'is-open' : ''}`}>
                          <span className="jp-error-rune" aria-hidden>!</span>
                          <div className="jp-error-body">
                            <span className="jp-error-text">{isOpen ? job.error : preview}</span>
                            {truncated && (
                              <button
                                type="button"
                                className="jp-error-toggle"
                                onClick={() => toggleError(job.id)}
                                aria-expanded={isOpen}
                              >
                                {isOpen ? 'show less' : 'show more'}
                              </button>
                            )}
                          </div>
                        </div>
                      );
                    })()}
                  </div>

                  <div className="jp-actions">
                    <button
                      className="ex-btn ex-btn-ghost"
                      onClick={() => handleViewLogs(job.id)}
                      aria-expanded={isExpanded}
                    >
                      Logs
                      <span className="ex-btn-glyph" aria-hidden>{isExpanded ? '▴' : '▾'}</span>
                    </button>
                    {(job.status === 'completed' || job.status === 'failed') && (
                      <button className="ex-btn ex-btn-ghost" onClick={() => handleViewMetrics(job.id)}>
                        Metrics <span className="ex-btn-glyph" aria-hidden>→</span>
                      </button>
                    )}
                    {isActive && (
                      <button className="ex-btn ex-btn-icondanger" onClick={() => handleCancel(job.id)} aria-label="Cancel job" title="Cancel">
                        ×
                      </button>
                    )}
                  </div>
                </div>

                {isExpanded && (
                  <div className="jp-drawer">
                    <header className="jp-drawer-head">
                      <div className="jp-drawer-title">
                        <span className="jp-drawer-label">Logs</span>
                        <span className="jp-drawer-count">{logs.length.toString().padStart(3, '0')} line{logs.length === 1 ? '' : 's'}</span>
                      </div>
                      <button
                        className="ex-linkbtn"
                        onClick={() => refreshLogs(job.id)}
                        disabled={loadingLogs === job.id}
                        type="button"
                      >
                        {loadingLogs === job.id ? 'loading…' : 'refresh'}
                      </button>
                    </header>

                    {logs.length === 0 ? (
                      <div className="jp-drawer-empty">
                        {loadingLogs === job.id ? 'Loading logs…' : 'No logs available.'}
                      </div>
                    ) : (
                      <pre className="jp-logs">
                        {logs.join('\n')}
                        <div ref={logsEndRef} />
                      </pre>
                    )}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

function formatDuration(job: JobInfo): string {
  if (!job.started_at) return '—';
  const start = new Date(job.started_at).getTime();
  const end = job.completed_at ? new Date(job.completed_at).getTime() : Date.now();
  const duration = Math.floor((end - start) / 1000);

  if (duration < 60) return `${duration}s`;
  if (duration < 3600) return `${Math.floor(duration / 60)}m ${duration % 60}s`;
  return `${Math.floor(duration / 3600)}h ${Math.floor((duration % 3600) / 60)}m`;
}

function formatRelativeTime(dateStr: string): string {
  const diff = Math.floor((Date.now() - new Date(dateStr).getTime()) / 1000);
  if (diff < 10) return 'just now';
  if (diff < 60) return `${diff}s ago`;
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  return `${Math.floor(diff / 86400)}d ago`;
}

function formatAbsoluteTime(dateStr: string): string {
  return new Date(dateStr).toLocaleString();
}

export default JobsPage;
