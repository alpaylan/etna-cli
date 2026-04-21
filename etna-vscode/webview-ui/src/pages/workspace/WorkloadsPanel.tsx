import { useEffect, useRef, useState } from 'react';
import { vscode, WorkloadMetadata, WorkloadEntry, onMessage } from '../../api/vscodeApi';

interface Props {
  experimentName: string;
  workloads: WorkloadMetadata[];
}

type Pending = { kind: 'add' | 'remove'; workload: string };

// Sentinel for the "type a URL" option in the catalog dropdown.
const URL_MODE = '__url__';

function WorkloadsPanel({ experimentName, workloads: initial }: Props) {
  const [workloads, setWorkloads] = useState<WorkloadMetadata[]>(initial);
  const [catalog, setCatalog] = useState<WorkloadEntry[]>([]);
  const [catalogLoaded, setCatalogLoaded] = useState(false);
  const [selection, setSelection] = useState<string>(URL_MODE);
  const [url, setUrl] = useState<string>('');
  const [ref, setRef] = useState<string>('');
  const [pending, setPending] = useState<Pending | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [confirmingKey, setConfirmingKey] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const pendingSpecRef = useRef<string | null>(null);

  useEffect(() => setWorkloads(initial), [initial]);

  // One-shot catalog load on mount.
  useEffect(() => {
    vscode.postMessage({ type: 'getAvailableWorkloads' });
  }, []);

  useEffect(() => {
    const unsubscribe = onMessage((message) => {
      if (message.type === 'workloads') {
        const payload = message.data as { experimentName: string; workloads: WorkloadMetadata[] };
        if (payload.experimentName === experimentName) {
          setWorkloads(payload.workloads);
          if (pending?.kind === 'add') {
            setUrl('');
            setRef('');
            setSelection(URL_MODE);
          }
          setPending(null);
          pendingSpecRef.current = null;
          vscode.postMessage({ type: 'getExperiments' });
        }
      } else if (message.type === 'availableWorkloads') {
        const payload = message.data as { workloads: WorkloadEntry[] };
        setCatalog(payload.workloads ?? []);
        setCatalogLoaded(true);
      } else if (message.type === 'workloadIndexRefreshed') {
        setRefreshing(false);
        vscode.postMessage({ type: 'getAvailableWorkloads' });
      } else if (message.type === 'error') {
        setError(message.message || 'Unknown error');
        setPending(null);
        setRefreshing(false);
        pendingSpecRef.current = null;
      }
    });
    return unsubscribe;
  }, [experimentName, pending]);

  useEffect(() => {
    if (!error) return;
    const t = setTimeout(() => setError(null), 4500);
    return () => clearTimeout(t);
  }, [error]);

  const handleSelect = (value: string) => {
    setSelection(value);
    if (value === URL_MODE) {
      setUrl('');
    } else {
      // Prefill URL so power-users can tweak before submitting.
      const entry = catalog.find((e) => e.name === value);
      setUrl(entry?.url ?? '');
    }
  };

  const handleAdd = () => {
    // `spec` is what the server gets: either the catalog name (resolved
    // server-side) or, when the user picked "URL", the URL they typed.
    const trimmedUrl = url.trim();
    const spec = selection === URL_MODE ? trimmedUrl : selection;
    if (!spec) return;
    const trimmedRef = ref.trim() || undefined;
    pendingSpecRef.current = spec;
    setPending({ kind: 'add', workload: spec });
    setError(null);
    vscode.postMessage({
      type: 'addWorkload',
      experimentName,
      spec,
      ref: trimmedRef,
    });
  };

  const handleRefreshCatalog = () => {
    setRefreshing(true);
    setError(null);
    vscode.postMessage({ type: 'refreshWorkloadIndex' });
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
  const urlMode = selection === URL_MODE;
  const addDisabled =
    (urlMode ? !url.trim() : !selection || selection === URL_MODE) || addBusy;

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
        <select
          className="ex-input ex-mono"
          value={selection}
          onChange={(e) => handleSelect(e.target.value)}
          disabled={addBusy}
          style={{ flex: '0 1 220px', minWidth: 180 }}
          aria-label="Workload catalog"
        >
          <option value={URL_MODE}>— paste a URL —</option>
          {catalog.map((entry) => (
            <option key={entry.name} value={entry.name}>
              {entry.name} · {entry.language}
              {entry.status !== 'stable' ? ` · ${entry.status}` : ''}
            </option>
          ))}
        </select>
        <input
          className="ex-input ex-mono ex-newtest-input"
          type="url"
          placeholder={urlMode ? 'https://github.com/owner/repo' : 'resolved from catalog'}
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter' && !addDisabled) handleAdd(); }}
          disabled={addBusy || !urlMode}
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
        <button
          className="ex-linkbtn"
          onClick={handleRefreshCatalog}
          disabled={refreshing}
          type="button"
          title="Refresh the cached workload catalog from its canonical URL"
        >
          {refreshing ? 'refreshing…' : 'refresh catalog'}
        </button>
      </section>

      {catalogLoaded && catalog.length === 0 && (
        <div className="ex-drawer-empty">
          <span aria-hidden>⚠</span> Catalog is empty — try `refresh catalog`.
        </div>
      )}

      {workloads.length === 0 ? (
        <div className="ex-drawer-empty">
          <span aria-hidden>∅</span> No workloads yet — pick one from the catalog or paste a repo URL.
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
