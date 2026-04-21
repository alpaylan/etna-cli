import { useEffect, useRef, useState } from 'react';
import { vscode, WorkloadMetadata, onMessage } from '../../api/vscodeApi';

interface Props {
  experimentName: string;
  workloads: WorkloadMetadata[];
}

type Pending = { kind: 'add' | 'remove'; workload: string };

function WorkloadsPanel({ experimentName, workloads: initial }: Props) {
  const [workloads, setWorkloads] = useState<WorkloadMetadata[]>(initial);
  const [url, setUrl] = useState<string>('');
  const [ref, setRef] = useState<string>('');
  const [pending, setPending] = useState<Pending | null>(null);
  const [confirmingKey, setConfirmingKey] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const pendingUrlRef = useRef<string | null>(null);

  useEffect(() => setWorkloads(initial), [initial]);

  useEffect(() => {
    const unsubscribe = onMessage((message) => {
      if (message.type === 'workloads') {
        const payload = message.data as { experimentName: string; workloads: WorkloadMetadata[] };
        if (payload.experimentName === experimentName) {
          setWorkloads(payload.workloads);
          if (pending?.kind === 'add') {
            setUrl('');
            setRef('');
          }
          setPending(null);
          pendingUrlRef.current = null;
          vscode.postMessage({ type: 'getExperiments' });
        }
      } else if (message.type === 'error') {
        setError(message.message || 'Unknown error');
        setPending(null);
        pendingUrlRef.current = null;
      }
    });
    return unsubscribe;
  }, [experimentName, pending]);

  useEffect(() => {
    if (!error) return;
    const t = setTimeout(() => setError(null), 4500);
    return () => clearTimeout(t);
  }, [error]);

  const handleAdd = () => {
    const trimmedUrl = url.trim();
    if (!trimmedUrl) return;
    const trimmedRef = ref.trim() || undefined;
    pendingUrlRef.current = trimmedUrl;
    setPending({ kind: 'add', workload: trimmedUrl });
    setError(null);
    vscode.postMessage({
      type: 'addWorkload',
      experimentName,
      url: trimmedUrl,
      ref: trimmedRef,
    });
  };

  const handleRemove = (w: WorkloadMetadata) => {
    if (confirmingKey !== w.name) {
      setConfirmingKey(w.name);
      return;
    }
    setConfirmingKey(null);
    setPending({ kind: 'remove', workload: w.name });
    setError(null);
    vscode.postMessage({
      type: 'removeWorkload',
      experimentName,
      workload: w.name,
    });
  };

  const addBusy = pending?.kind === 'add';
  const addDisabled = !url.trim() || addBusy;

  return (
    <div className="ex-drawer ex-drawer-standalone">
      <header className="ex-drawer-head">
        <div className="ex-drawer-title">
          <span className="ex-drawer-label">Workloads</span>
          <span className="ex-drawer-count">{workloads.length.toString().padStart(2, '0')}</span>
        </div>
      </header>

      {error && <div className="error" role="alert">{error}</div>}

      <section className="ex-newtest" aria-label="Add workload">
        <span className="ex-newtest-tag">ADD</span>
        <input
          className="ex-input ex-mono ex-newtest-input"
          type="url"
          placeholder="https://github.com/owner/repo"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter' && !addDisabled) handleAdd(); }}
          disabled={addBusy}
          style={{ flex: '1 1 auto' }}
        />
        <input
          className="ex-input ex-mono"
          type="text"
          placeholder="ref (optional)"
          value={ref}
          onChange={(e) => setRef(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter' && !addDisabled) handleAdd(); }}
          disabled={addBusy}
          style={{ flex: '0 1 160px', minWidth: 120 }}
        />
        <button
          className="ex-btn ex-btn-primary ex-btn-sm"
          onClick={handleAdd}
          disabled={addDisabled}
          type="button"
        >
          {addBusy ? 'Cloning…' : 'Add'}
        </button>
      </section>

      {workloads.length === 0 ? (
        <div className="ex-drawer-empty">
          <span aria-hidden>∅</span> No workloads yet — paste a repo URL above.
        </div>
      ) : (
        <ul className="ex-testlist">
          {workloads.map((w) => {
            const isRemoving = pending?.kind === 'remove' && pending.workload === w.name;
            const isConfirming = confirmingKey === w.name;
            return (
              <li key={w.name} className="ex-testitem">
                <div className="ex-testitem-main">
                  <span className="ex-testitem-name">
                    <span className="ex-mono">{w.name}</span>
                  </span>
                  <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                    {isConfirming && !isRemoving && (
                      <button
                        className="ex-linkbtn"
                        onClick={() => setConfirmingKey(null)}
                        type="button"
                      >cancel</button>
                    )}
                    <button
                      className="ex-linkbtn"
                      onClick={() => handleRemove(w)}
                      disabled={isRemoving}
                      type="button"
                      style={isConfirming ? { color: 'var(--tx-error-fg)' } : undefined}
                    >
                      {isRemoving ? 'removing…' : isConfirming ? 'click to confirm' : 'remove'}
                    </button>
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

export default WorkloadsPanel;
