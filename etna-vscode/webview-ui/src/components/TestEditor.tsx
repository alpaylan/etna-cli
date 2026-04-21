import {
  useState,
  useEffect,
  useRef,
  useCallback,
  useMemo,
  SetStateAction,
  KeyboardEvent as ReactKeyboardEvent,
} from 'react';
import { TestDefinition, vscode, onMessage } from '../api/vscodeApi';

type MutationCatalogEntry =
  | { status: 'loading' }
  | { status: 'ready'; values: string[] }
  | { status: 'error'; error: string };

const catalogKey = (wl: string) => wl.trim();

const MAX_HISTORY = 50;

const WORKLOAD_SUGGESTIONS = [
  'BST', 'RBT', 'STLC', 'SystemF', 'IFC', 'Sorting',
  'BSTProplang', 'RBTProplang', 'STLCProplang', 'IFCProplang', 'SortingProplang',
];

const ord = (n: number) => String(n + 1).padStart(2, '0');

interface Props {
  experimentName: string;
  testName: string;
  initialTests: TestDefinition[];
  onSave: (tests: TestDefinition[]) => void;
  onCancel: () => void;
  onDelete?: () => void;
  isNew?: boolean;
}

interface ParamsStatus {
  valid: boolean;
  message: string;
  count?: number;
}

function targetLabel(t: TestDefinition): string {
  const w = t.workload?.trim();
  return w || 'Untitled target';
}

function targetSummary(t: TestDefinition): string {
  const bits: string[] = [];
  bits.push(`${t.trials} trial${t.trials === 1 ? '' : 's'}`);
  bits.push(`${t.timeout}s`);
  const muts = (t.mutations || []).length;
  if (muts > 0) bits.push(`${muts} mut`);
  const tsks = (t.tasks || []).length;
  if (tsks > 0) bits.push(`${tsks} task${tsks === 1 ? '' : 's'}`);
  if (t.cross) bits.push('cross');
  return bits.join(' · ');
}

// ---------- Params editor helpers ----------

type ParamRow = { id: number; key: string; rawValue: string };

let paramRowIdCounter = 0;
const nextParamRowId = () => ++paramRowIdCounter;

function formatRawValue(v: unknown): string {
  if (typeof v === 'string') return v;
  try { return JSON.stringify(v); } catch { return String(v); }
}

function parseRawValue(raw: string): { ok: true; value: unknown } | { ok: false } {
  const trimmed = raw.trim();
  if (trimmed === '') return { ok: true, value: '' };
  // Try JSON first — handles numbers, booleans, null, arrays, objects
  try { return { ok: true, value: JSON.parse(trimmed) }; } catch { /* fallthrough */ }
  // Treat as plain string otherwise
  return { ok: true, value: raw };
}

function rowsFromParams(params: Record<string, unknown> | undefined): ParamRow[] {
  const out: ParamRow[] = [];
  if (!params) return out;
  for (const [k, v] of Object.entries(params)) {
    out.push({ id: nextParamRowId(), key: k, rawValue: formatRawValue(v) });
  }
  return out;
}

function rowsToParams(rows: ParamRow[]): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const r of rows) {
    const k = r.key.trim();
    if (!k) continue;
    const parsed = parseRawValue(r.rawValue);
    out[k] = parsed.ok ? parsed.value : r.rawValue;
  }
  return out;
}

function classifyValue(raw: string): { kind: string; cls: string } {
  const trimmed = raw.trim();
  if (trimmed === '') return { kind: 'str', cls: 'is-str' };
  try {
    const v = JSON.parse(trimmed);
    if (v === null) return { kind: 'null', cls: 'is-null' };
    if (typeof v === 'boolean') return { kind: 'bool', cls: 'is-bool' };
    if (typeof v === 'number') return { kind: 'num', cls: 'is-num' };
    if (Array.isArray(v)) return { kind: 'arr', cls: 'is-arr' };
    if (typeof v === 'object') return { kind: 'obj', cls: 'is-obj' };
  } catch { /* string */ }
  return { kind: 'str', cls: 'is-str' };
}

function TestEditor({ experimentName, testName, initialTests, onSave, onCancel, onDelete, isNew }: Props) {
  const [tests, setTests] = useState<TestDefinition[]>(initialTests);
  const [activeTestIndex, setActiveTestIndex] = useState(0);
  const [paramRows, setParamRows] = useState<ParamRow[]>(() => rowsFromParams(initialTests[0]?.params));
  const [paramsJsonMode, setParamsJsonMode] = useState(false);
  const [paramsText, setParamsText] = useState(
    initialTests[0]?.params ? JSON.stringify(initialTests[0].params, null, 2) : '{}'
  );
  const [mutationInput, setMutationInput] = useState('');
  const [showValidation, setShowValidation] = useState(false);
  const [mutationCatalog, setMutationCatalog] = useState<Record<string, MutationCatalogEntry>>({});
  const historyRef = useRef<TestDefinition[][]>([]);
  const futureRef = useRef<TestDefinition[][]>([]);
  const requestedMutationsRef = useRef<Set<string>>(new Set());

  const setTestsWithHistory = useCallback((update: SetStateAction<TestDefinition[]>) => {
    setTests(prev => {
      historyRef.current = [...historyRef.current.slice(-(MAX_HISTORY - 1)), prev];
      futureRef.current = [];
      return typeof update === 'function' ? update(prev) : update;
    });
  }, []);

  const undo = useCallback(() => {
    if (historyRef.current.length === 0) return;
    setTests(prev => {
      const previous = historyRef.current[historyRef.current.length - 1];
      historyRef.current = historyRef.current.slice(0, -1);
      futureRef.current = [...futureRef.current, prev];
      setActiveTestIndex(idx => Math.min(idx, previous.length - 1));
      return previous;
    });
  }, []);

  const redo = useCallback(() => {
    if (futureRef.current.length === 0) return;
    setTests(prev => {
      const next = futureRef.current[futureRef.current.length - 1];
      futureRef.current = futureRef.current.slice(0, -1);
      historyRef.current = [...historyRef.current, prev];
      setActiveTestIndex(idx => Math.min(idx, next.length - 1));
      return next;
    });
  }, []);

  // Listen for mutation catalog responses from the server.
  useEffect(() => {
    const unsub = onMessage((m) => {
      if (m.type !== 'workloadMutations') return;
      const { workload, mutations, error } = (m.data ?? {}) as {
        experimentName: string;
        workload: string;
        mutations: string[] | null;
        error?: string;
      };
      if (!workload) return;
      const key = catalogKey(workload);
      setMutationCatalog(prev => ({
        ...prev,
        [key]: mutations
          ? { status: 'ready', values: mutations }
          : { status: 'error', error: error ?? 'Failed to load mutations' },
      }));
    });
    return unsub;
  }, []);

  useEffect(() => {
    setTests(initialTests);
    setActiveTestIndex(0);
    setParamRows(rowsFromParams(initialTests[0]?.params));
    setParamsText(initialTests[0]?.params ? JSON.stringify(initialTests[0].params, null, 2) : '{}');
    historyRef.current = [];
    futureRef.current = [];
  }, [initialTests]);

  useEffect(() => {
    const current = tests[activeTestIndex];
    setParamRows(rowsFromParams(current?.params));
    setParamsText(current?.params ? JSON.stringify(current.params, null, 2) : '{}');
    setMutationInput('');
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeTestIndex]);

  const activeTest = tests[activeTestIndex] || {
    workload: '',
    trials: 10,
    timeout: 60,
    mutations: [],
    cross: false,
    params: {},
    tasks: [],
  };

  // Debounced request for the active target's mutation catalog.
  useEffect(() => {
    const wl = activeTest.workload?.trim();
    if (!wl) return;
    const key = catalogKey(wl);
    if (requestedMutationsRef.current.has(key)) return;
    const timer = setTimeout(() => {
      requestedMutationsRef.current.add(key);
      setMutationCatalog(prev => prev[key] ? prev : { ...prev, [key]: { status: 'loading' } });
      vscode.postMessage({ type: 'getWorkloadMutations', experimentName, workload: wl });
    }, 250);
    return () => clearTimeout(timer);
  }, [activeTest.workload, experimentName]);

  const activeCatalog: MutationCatalogEntry | undefined = useMemo(() => {
    const wl = activeTest.workload?.trim();
    if (!wl) return undefined;
    return mutationCatalog[catalogKey(wl)];
  }, [mutationCatalog, activeTest.workload]);

  const availableMutations = activeCatalog?.status === 'ready' ? activeCatalog.values : [];
  const mutationSuggestions = availableMutations.filter(m => !(activeTest.mutations || []).includes(m));

  const updateActiveTest = useCallback((updates: Partial<TestDefinition>) => {
    setTestsWithHistory(prev => {
      const updated = [...prev];
      updated[activeTestIndex] = { ...prev[activeTestIndex], ...updates };
      return updated;
    });
  }, [activeTestIndex, setTestsWithHistory]);

  const addTest = () => {
    setTestsWithHistory(prev => [...prev, {
      workload: '', trials: 10, timeout: 60,
      mutations: ['base'], cross: false, params: {}, tasks: [{ strategy: '', property: '' }],
    }]);
    setActiveTestIndex(tests.length);
  };

  const removeTest = (index: number) => {
    if (tests.length <= 1) return;
    setTestsWithHistory(prev => prev.filter((_, i) => i !== index));
    setActiveTestIndex(idx => {
      if (idx === index) return Math.max(0, index - 1);
      if (idx > index) return idx - 1;
      return idx;
    });
  };

  // Mutations as chips ----------------------------------------------
  const addMutationFromInput = () => {
    const raw = mutationInput.trim();
    if (!raw) return;
    const candidates = raw.split(',').map(s => s.trim()).filter(Boolean);
    const existing = new Set(activeTest.mutations || []);
    const merged = [...(activeTest.mutations || [])];
    for (const c of candidates) {
      if (!existing.has(c)) { merged.push(c); existing.add(c); }
    }
    updateActiveTest({ mutations: merged });
    setMutationInput('');
  };

  const removeMutation = (name: string) => {
    updateActiveTest({ mutations: (activeTest.mutations || []).filter(m => m !== name) });
  };

  const onMutationKeyDown = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter' || e.key === ',') {
      e.preventDefault();
      addMutationFromInput();
    } else if (e.key === 'Backspace' && mutationInput === '' && (activeTest.mutations || []).length > 0) {
      const list = activeTest.mutations || [];
      updateActiveTest({ mutations: list.slice(0, -1) });
    }
  };

  // Params — KV mode -----------------------------------------------
  const commitParamRows = (rows: ParamRow[]) => {
    setParamRows(rows);
    const parsed = rowsToParams(rows);
    updateActiveTest({ params: parsed });
    setParamsText(JSON.stringify(parsed, null, 2));
  };

  const addParamRow = () => {
    commitParamRows([...paramRows, { id: nextParamRowId(), key: '', rawValue: '' }]);
  };

  const updateParamRow = (id: number, patch: Partial<Omit<ParamRow, 'id'>>) => {
    commitParamRows(paramRows.map(r => (r.id === id ? { ...r, ...patch } : r)));
  };

  const removeParamRow = (id: number) => {
    commitParamRows(paramRows.filter(r => r.id !== id));
  };

  const rowKeyDuplicates = useMemo(() => {
    const counts = new Map<string, number>();
    for (const r of paramRows) {
      const k = r.key.trim();
      if (!k) continue;
      counts.set(k, (counts.get(k) || 0) + 1);
    }
    const dup = new Set<string>();
    for (const [k, n] of counts) if (n > 1) dup.add(k);
    return dup;
  }, [paramRows]);

  // Params — JSON mode ---------------------------------------------
  const paramsStatus = useMemo<ParamsStatus>(() => {
    if (!paramsJsonMode) {
      // KV mode — derive validity from rows
      const empty = paramRows.every(r => !r.key.trim());
      if (empty) return { valid: true, message: 'no parameters', count: 0 };
      if (rowKeyDuplicates.size > 0) {
        return {
          valid: false,
          message: `duplicate key${rowKeyDuplicates.size === 1 ? '' : 's'}: ${Array.from(rowKeyDuplicates).join(', ')}`,
        };
      }
      const missing = paramRows.filter(r => !r.key.trim() && r.rawValue.trim()).length;
      if (missing > 0) return { valid: false, message: `${missing} row${missing === 1 ? ' is' : 's are'} missing a key` };
      const keyed = paramRows.filter(r => r.key.trim()).length;
      return { valid: true, message: `${keyed} parameter${keyed === 1 ? '' : 's'}`, count: keyed };
    }
    const text = paramsText.trim();
    if (!text) return { valid: true, message: 'empty — treated as { }', count: 0 };
    try {
      const v = JSON.parse(text);
      if (typeof v !== 'object' || v === null || Array.isArray(v)) {
        return { valid: false, message: 'must be a JSON object (not array or scalar)' };
      }
      return { valid: true, message: 'valid JSON object', count: Object.keys(v).length };
    } catch (e) {
      return { valid: false, message: (e as Error).message };
    }
  }, [paramsJsonMode, paramRows, rowKeyDuplicates, paramsText]);

  const handleParamsJsonChange = (value: string) => {
    setParamsText(value);
    try {
      const params = value.trim() ? JSON.parse(value) : {};
      if (typeof params === 'object' && params !== null && !Array.isArray(params)) {
        updateActiveTest({ params });
        setParamRows(rowsFromParams(params));
      }
    } catch {
      // keep typing; don't commit invalid JSON to model
    }
  };

  const toggleParamsMode = () => {
    if (paramsJsonMode) {
      // JSON → KV: try to parse, fall back to current rows
      try {
        const v = JSON.parse(paramsText || '{}');
        if (typeof v === 'object' && v !== null && !Array.isArray(v)) {
          setParamRows(rowsFromParams(v as Record<string, unknown>));
        }
      } catch { /* ignore */ }
      setParamsJsonMode(false);
    } else {
      setParamsText(JSON.stringify(rowsToParams(paramRows), null, 2));
      setParamsJsonMode(true);
    }
  };

  // Tasks ----------------------------------------------------------
  const addTask = () => {
    updateActiveTest({
      tasks: [...(activeTest.tasks || []), { strategy: '', property: '' }],
    });
  };

  const updateTask = (taskIndex: number, field: string, value: string) => {
    const newTasks = [...(activeTest.tasks || [])];
    newTasks[taskIndex] = { ...newTasks[taskIndex], [field]: value };
    updateActiveTest({ tasks: newTasks });
  };

  const removeTask = (taskIndex: number) => {
    updateActiveTest({
      tasks: (activeTest.tasks || []).filter((_, i) => i !== taskIndex),
    });
  };

  const duplicateTask = (taskIndex: number) => {
    const src = (activeTest.tasks || [])[taskIndex];
    if (!src) return;
    const newTasks = [...(activeTest.tasks || [])];
    newTasks.splice(taskIndex + 1, 0, { ...src });
    updateActiveTest({ tasks: newTasks });
  };

  const addTaskField = (taskIndex: number) => {
    const newTasks = [...(activeTest.tasks || [])];
    let fieldName = 'field';
    let counter = 1;
    while (newTasks[taskIndex][fieldName] !== undefined) {
      fieldName = `field${counter++}`;
    }
    newTasks[taskIndex] = { ...newTasks[taskIndex], [fieldName]: '' };
    updateActiveTest({ tasks: newTasks });
  };

  const removeTaskField = (taskIndex: number, key: string) => {
    const newTasks = [...(activeTest.tasks || [])];
    const { [key]: _omit, ...rest } = newTasks[taskIndex];
    newTasks[taskIndex] = rest;
    updateActiveTest({ tasks: newTasks });
  };

  const renameTaskField = (taskIndex: number, oldKey: string, newKey: string) => {
    if (!newKey || newKey === oldKey) return;
    const newTasks = [...(activeTest.tasks || [])];
    const task = newTasks[taskIndex];
    if (newKey in task) return;
    const entries = Object.entries(task).map(([k, v]) => k === oldKey ? [newKey, v] : [k, v]);
    newTasks[taskIndex] = Object.fromEntries(entries);
    updateActiveTest({ tasks: newTasks });
  };

  // Validation -----------------------------------------------------
  const invalidIndices = useMemo(() => {
    const bad: number[] = [];
    tests.forEach((t, i) => {
      if (!t.workload?.trim()) bad.push(i);
    });
    return bad;
  }, [tests]);

  const activeInvalid = showValidation && invalidIndices.includes(activeTestIndex);
  const canSave = tests.length > 0 && invalidIndices.length === 0 && paramsStatus.valid;

  const handleSave = useCallback(() => {
    if (!canSave) {
      setShowValidation(true);
      if (invalidIndices.length > 0) setActiveTestIndex(invalidIndices[0]);
      return;
    }
    onSave(tests);
  }, [canSave, invalidIndices, onSave, tests]);

  // Keyboard: ⌘/Ctrl+Z, ⌘/Ctrl+Shift+Z, ⌘/Ctrl+S, Esc
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (mod && e.key === 'z') {
        e.preventDefault(); e.stopPropagation();
        if (e.shiftKey) redo(); else undo();
      } else if (mod && (e.key === 's' || e.key === 'S')) {
        e.preventDefault(); e.stopPropagation();
        handleSave();
      } else if (e.key === 'Escape') {
        // don't steal focus from form inputs with local use of Esc
        const tag = (e.target as HTMLElement | null)?.tagName?.toLowerCase();
        if (tag !== 'input' && tag !== 'textarea') {
          e.preventDefault();
          onCancel();
        }
      }
    };
    window.addEventListener('keydown', handleKeyDown, true);
    return () => window.removeEventListener('keydown', handleKeyDown, true);
  }, [undo, redo, handleSave, onCancel]);

  // Run plan -------------------------------------------------------
  const plan = useMemo(() => {
    const muts = Math.max(1, (activeTest.mutations || []).length);
    const tsks = Math.max(1, (activeTest.tasks || []).length);
    const trials = Math.max(0, activeTest.trials || 0);
    return {
      muts: (activeTest.mutations || []).length,
      tsks: (activeTest.tasks || []).length,
      trials,
      total: muts * tsks * trials,
      worstSeconds: muts * tsks * (activeTest.timeout || 0),
    };
  }, [activeTest.mutations, activeTest.tasks, activeTest.trials, activeTest.timeout]);

  const formatDuration = (seconds: number): string => {
    if (!Number.isFinite(seconds) || seconds <= 0) return '—';
    if (seconds < 60) return `${Math.round(seconds)}s`;
    if (seconds < 3600) return `${(seconds / 60).toFixed(1)}m`;
    return `${(seconds / 3600).toFixed(1)}h`;
  };

  // -----------------------------------------------------------------

  return (
    <div className="tx-editor">
      <header className="tx-header">
        <div className="tx-header-left">
          <div className="tx-crumb">
            <span className="tx-crumb-tag">Experiment</span>
            <span className="tx-crumb-value">{experimentName}</span>
          </div>
          <h2 className="tx-title">
            <span className="tx-title-verb">{isNew ? 'Drafting' : 'Editing'}</span>
            <span className="tx-title-name">{testName}</span>
          </h2>
        </div>
        <div className="tx-header-right">
          <button className="tx-btn tx-btn-ghost" onClick={onCancel} title="Cancel (Esc)">Cancel</button>
          {onDelete && !isNew && (
            <button className="tx-btn tx-btn-danger" onClick={onDelete}>Delete test</button>
          )}
          <button
            className="tx-btn tx-btn-primary"
            onClick={handleSave}
            disabled={showValidation && !canSave}
            title={!canSave && showValidation ? 'Fix validation errors first' : 'Save (⌘S)'}
          >
            <span>{isNew ? 'Create test' : 'Save changes'}</span>
            <span className="tx-btn-kbd" aria-hidden>⌘S</span>
          </button>
        </div>
      </header>

      {showValidation && !canSave && (
        <div className="tx-banner tx-banner-error">
          <span className="tx-banner-rune">!</span>
          <span>
            {invalidIndices.length > 0 && (
              <>Target{invalidIndices.length === 1 ? '' : 's'} {invalidIndices.map(ord).join(', ')} {invalidIndices.length === 1 ? 'is' : 'are'} missing required fields.</>
            )}
            {!paramsStatus.valid && (
              <> Parameters are invalid — {paramsStatus.message}.</>
            )}
          </span>
        </div>
      )}

      <div className="tx-body">
        <aside className="tx-sidebar">
          <div className="tx-sidebar-head">
            <span className="tx-sidebar-label">Targets</span>
            <span className="tx-sidebar-count">{tests.length.toString().padStart(2, '0')}</span>
          </div>
          <ul className="tx-target-list">
            {tests.map((t, i) => {
              const invalid = showValidation && invalidIndices.includes(i);
              const isActive = i === activeTestIndex;
              const wl = t.workload?.trim();
              return (
                <li key={i}>
                  <button
                    className={`tx-target ${isActive ? 'is-active' : ''} ${invalid ? 'is-invalid' : ''}`}
                    onClick={() => setActiveTestIndex(i)}
                  >
                    <span className="tx-target-index">{ord(i)}</span>
                    <span className="tx-target-body">
                      <span className="tx-target-title">
                        {wl ? (
                          <span className="tx-target-wl">{wl}</span>
                        ) : (
                          <span className="tx-target-placeholder">Untitled target</span>
                        )}
                      </span>
                      <span className="tx-target-meta">{targetSummary(t)}</span>
                    </span>
                    {tests.length > 1 && (
                      <span
                        className="tx-target-remove"
                        role="button"
                        aria-label="Remove target"
                        onClick={(e) => { e.stopPropagation(); removeTest(i); }}
                      >×</span>
                    )}
                  </button>
                </li>
              );
            })}
          </ul>
          <button className="tx-target-add" onClick={addTest}>
            <span aria-hidden>+</span> Add target
          </button>

          <div className="tx-sidekbd" aria-hidden>
            <div className="tx-sidekbd-row"><kbd>⌘S</kbd><span>save</span></div>
            <div className="tx-sidekbd-row"><kbd>⌘Z</kbd><span>undo</span></div>
            <div className="tx-sidekbd-row"><kbd>⌘⇧Z</kbd><span>redo</span></div>
          </div>
        </aside>

        <main className="tx-canvas">
          {/* Run plan — compact preview so users see the scale of a run before saving */}
          <section className="tx-runplan" aria-label="Run plan summary">
            <div className="tx-runplan-head">
              <span className="tx-runplan-label">Run plan</span>
              <span className="tx-runplan-target">{targetLabel(activeTest)}</span>
            </div>
            <div className="tx-runplan-body">
              <span className="tx-runplan-term">
                <span className="tx-runplan-num">{plan.muts || 0}</span>
                <span className="tx-runplan-unit">mutation{plan.muts === 1 ? '' : 's'}</span>
              </span>
              <span className="tx-runplan-op" aria-hidden>×</span>
              <span className="tx-runplan-term">
                <span className="tx-runplan-num">{plan.tsks || 0}</span>
                <span className="tx-runplan-unit">task{plan.tsks === 1 ? '' : 's'}</span>
              </span>
              <span className="tx-runplan-op" aria-hidden>×</span>
              <span className="tx-runplan-term">
                <span className="tx-runplan-num">{plan.trials}</span>
                <span className="tx-runplan-unit">trial{plan.trials === 1 ? '' : 's'}</span>
              </span>
              <span className="tx-runplan-eq" aria-hidden>=</span>
              <span className="tx-runplan-total">
                <span className="tx-runplan-num">{plan.total.toLocaleString()}</span>
                <span className="tx-runplan-unit">execution{plan.total === 1 ? '' : 's'}</span>
              </span>
            </div>
            <div className="tx-runplan-foot">
              <span className="tx-runplan-foot-item">
                <span className="tx-runplan-foot-label">worst-case</span>
                <span className="tx-runplan-foot-value">≤ {formatDuration(plan.worstSeconds)}</span>
              </span>
              {activeTest.cross && (
                <span className="tx-runplan-foot-item tx-runplan-foot-cross">
                  <span className="tx-runplan-foot-dot" aria-hidden /> cross
                </span>
              )}
              {(plan.muts === 0 || plan.tsks === 0) && (
                <span className="tx-runplan-foot-item is-warn">
                  <span className="tx-runplan-foot-rune" aria-hidden>!</span>
                  {plan.muts === 0 ? 'no mutations configured' : 'no tasks configured'}
                </span>
              )}
            </div>
          </section>

          {/* I — Identity & budget (merged for tighter hierarchy) */}
          <section className="tx-section">
            <header className="tx-section-head">
              <span className="tx-section-index">01</span>
              <div className="tx-section-headings">
                <h3 className="tx-section-title">Identity &amp; budget</h3>
                <p className="tx-section-sub">Workload picks <em>what</em> runs; trials and timeout bound the <em>cost</em>.</p>
              </div>
            </header>
            <div className="tx-setup-grid">
              <label className="tx-field tx-field-wide">
                <span className="tx-field-label">Workload <em>required</em></span>
                <input
                  type="text"
                  list="tx-workload-suggestions"
                  className={`tx-input ${activeInvalid && !activeTest.workload?.trim() ? 'is-invalid' : ''}`}
                  value={activeTest.workload}
                  onChange={(e) => updateActiveTest({ workload: e.target.value })}
                  placeholder="bst-rust, rbt-ocaml …"
                />
                {activeInvalid && !activeTest.workload?.trim() && (
                  <span className="tx-field-error">Pick a workload.</span>
                )}
              </label>
              <datalist id="tx-workload-suggestions">
                {WORKLOAD_SUGGESTIONS.map(s => <option key={s} value={s} />)}
              </datalist>

              <label className="tx-field">
                <span className="tx-field-label">Trials</span>
                <input
                  type="number"
                  className="tx-input tx-input-num tx-mono"
                  value={activeTest.trials}
                  onChange={(e) => updateActiveTest({ trials: parseInt(e.target.value) || 1 })}
                  min={1}
                />
                <span className="tx-field-hint">per task × mutation</span>
              </label>
              <label className="tx-field">
                <span className="tx-field-label">Timeout</span>
                <div className="tx-input-suffix">
                  <input
                    type="number"
                    className="tx-input tx-input-num tx-mono"
                    value={activeTest.timeout}
                    onChange={(e) => updateActiveTest({ timeout: parseFloat(e.target.value) || 60 })}
                    min={0.1}
                    step={0.1}
                  />
                  <span className="tx-suffix">s</span>
                </div>
                <span className="tx-field-hint">wall-clock seconds</span>
              </label>
              <div className="tx-field">
                <span className="tx-field-label">Cross</span>
                <label className="tx-toggle">
                  <input
                    type="checkbox"
                    checked={activeTest.cross || false}
                    onChange={(e) => updateActiveTest({ cross: e.target.checked })}
                  />
                  <span className="tx-toggle-track"><span className="tx-toggle-thumb" /></span>
                  <span className="tx-toggle-label">{activeTest.cross ? 'enabled' : 'disabled'}</span>
                </label>
                <span className="tx-field-hint">compare across workloads</span>
              </div>
            </div>
          </section>

          {/* II — Mutations */}
          <section className="tx-section">
            <header className="tx-section-head">
              <span className="tx-section-index">02</span>
              <div className="tx-section-headings">
                <h3 className="tx-section-title">Mutations</h3>
                <p className="tx-section-sub">Source variants to evaluate. <code>base</code> is the unmutated reference.</p>
              </div>
              {activeCatalog?.status === 'ready' && (
                <span className="tx-section-action tx-catalog-tag" title={`${availableMutations.length} mutations available for ${activeTest.workload}`}>
                  {availableMutations.length} known
                </span>
              )}
            </header>
            <div
              className={`tx-chipinput ${(activeTest.mutations || []).length === 0 && !mutationInput ? 'is-empty' : ''}`}
              onClick={(e) => {
                const input = (e.currentTarget as HTMLElement).querySelector<HTMLInputElement>('.tx-chipinput-input');
                input?.focus();
              }}
            >
              {(activeTest.mutations || []).map((m) => {
                const isUnknown =
                  activeCatalog?.status === 'ready' && !availableMutations.includes(m);
                const cls = [
                  'tx-chip',
                  m === 'base' ? 'is-base' : '',
                  isUnknown ? 'is-unknown' : '',
                ].filter(Boolean).join(' ');
                return (
                  <span
                    key={m}
                    className={cls}
                    title={isUnknown ? `"${m}" is not listed for ${activeTest.workload}` : undefined}
                  >
                    {isUnknown && <span className="tx-chip-warn" aria-hidden>!</span>}
                    <span className="tx-chip-text">{m}</span>
                    <button
                      className="tx-chip-x"
                      onClick={(e) => { e.stopPropagation(); removeMutation(m); }}
                      aria-label={`Remove ${m}`}
                      type="button"
                    >×</button>
                  </span>
                );
              })}
              <input
                className="tx-chipinput-input"
                value={mutationInput}
                onChange={(e) => setMutationInput(e.target.value)}
                onKeyDown={onMutationKeyDown}
                onBlur={addMutationFromInput}
                list={activeCatalog?.status === 'ready' ? 'tx-mutation-suggestions' : undefined}
                placeholder={(activeTest.mutations || []).length === 0 ? 'type a mutation name, press Enter…' : 'add another…'}
              />
              {activeCatalog?.status === 'ready' && (
                <datalist id="tx-mutation-suggestions">
                  {availableMutations.map(m => <option key={m} value={m} />)}
                </datalist>
              )}
            </div>

            {activeCatalog?.status === 'loading' && (
              <div className="tx-catalog-note is-loading">
                <span className="tx-catalog-dot" aria-hidden />
                <span>Loading mutations for <code>{activeTest.workload}</code>…</span>
              </div>
            )}
            {activeCatalog?.status === 'error' && (
              <div className="tx-catalog-note is-error">
                <span className="tx-catalog-rune" aria-hidden>!</span>
                <span>
                  Couldn't load the mutation catalog for <code>{activeTest.workload}</code>.{' '}
                  <span className="tx-catalog-detail">{activeCatalog.error}</span>
                </span>
              </div>
            )}
            {activeCatalog?.status === 'ready' && mutationSuggestions.length > 0 && (
              <div className="tx-suggest">
                <span className="tx-suggest-label">Suggestions</span>
                <div className="tx-suggest-row">
                  {mutationSuggestions.map((m) => (
                    <button
                      key={m}
                      type="button"
                      className={`tx-chip-suggest ${m === 'base' ? 'is-base' : ''}`}
                      onClick={() =>
                        updateActiveTest({
                          mutations: [...(activeTest.mutations || []), m],
                        })
                      }
                      title={`Add "${m}"`}
                    >
                      <span className="tx-chip-suggest-plus" aria-hidden>+</span>
                      <span>{m}</span>
                    </button>
                  ))}
                </div>
              </div>
            )}

            <div className="tx-chip-hint">
              <span>Press <kbd>Enter</kbd> or <kbd>,</kbd> to add · <kbd>⌫</kbd> on empty input to remove last.</span>
              {!(activeTest.mutations || []).includes('base') && (
                <button
                  className="tx-linkbtn"
                  type="button"
                  onClick={() => updateActiveTest({ mutations: ['base', ...(activeTest.mutations || [])] })}
                >
                  + add <code>base</code>
                </button>
              )}
            </div>
          </section>

          {/* III — Parameters (KV editor with JSON fallback) */}
          <section className="tx-section">
            <header className="tx-section-head">
              <span className="tx-section-index">03</span>
              <div className="tx-section-headings">
                <h3 className="tx-section-title">Parameters</h3>
                <p className="tx-section-sub">
                  {paramsJsonMode
                    ? <>Free-form JSON object. Values are whatever <code>JSON.parse</code> returns.</>
                    : <>Key → value rows. Values parse as JSON when possible (<code>42</code>, <code>true</code>, <code>"txt"</code>, <code>[1,2]</code>), else fall back to plain strings.</>
                  }
                </p>
              </div>
              <button
                className="tx-btn tx-btn-ghost tx-section-action tx-mode-toggle"
                onClick={toggleParamsMode}
                type="button"
                title={paramsJsonMode ? 'Switch to structured key-value editor' : 'Switch to raw JSON editor'}
              >
                {paramsJsonMode ? '▤ Structured' : '{ } JSON'}
              </button>
            </header>

            {!paramsJsonMode ? (
              <div className="tx-params">
                {paramRows.length === 0 ? (
                  <div className="tx-empty tx-empty-tight">
                    <span className="tx-empty-rune" aria-hidden>◇</span>
                    <p>No parameters yet — add one to pass it to every task.</p>
                    <button className="tx-btn tx-btn-ghost" onClick={addParamRow} type="button">+ Add parameter</button>
                  </div>
                ) : (
                  <>
                    <div className="tx-kv-table" role="table" aria-label="Parameters">
                      <div className="tx-kv-headrow" role="row">
                        <span className="tx-kv-colhead" role="columnheader">Key</span>
                        <span className="tx-kv-colhead" role="columnheader">Value</span>
                        <span className="tx-kv-colhead tx-kv-colhead-type" role="columnheader">Type</span>
                        <span className="tx-kv-colhead" />
                      </div>
                      {paramRows.map((row) => {
                        const dup = row.key.trim() && rowKeyDuplicates.has(row.key.trim());
                        const cls = classifyValue(row.rawValue);
                        return (
                          <div key={row.id} className={`tx-kv-datarow ${dup ? 'is-dup' : ''}`} role="row">
                            <input
                              type="text"
                              className={`tx-input tx-mono tx-kv-key ${dup ? 'is-invalid' : ''}`}
                              value={row.key}
                              onChange={(e) => updateParamRow(row.id, { key: e.target.value })}
                              placeholder="key"
                            />
                            <input
                              type="text"
                              className="tx-input tx-mono tx-kv-value"
                              value={row.rawValue}
                              onChange={(e) => updateParamRow(row.id, { rawValue: e.target.value })}
                              placeholder={'value (JSON or plain text)'}
                            />
                            <span className={`tx-kv-type ${cls.cls}`} title={`Interpreted as ${cls.kind}`}>{cls.kind}</span>
                            <button
                              className="tx-btn tx-btn-icon tx-btn-ghost"
                              onClick={() => removeParamRow(row.id)}
                              aria-label="Remove parameter"
                              type="button"
                            >×</button>
                          </div>
                        );
                      })}
                    </div>
                    <div className="tx-kv-foot">
                      <button className="tx-linkbtn" onClick={addParamRow} type="button">+ add parameter</button>
                    </div>
                  </>
                )}
                <div className={`tx-params-status ${paramsStatus.valid ? 'ok' : 'bad'}`}>
                  <span className="tx-status-dot" aria-hidden />
                  <span className="tx-status-glyph" aria-hidden>{paramsStatus.valid ? '✓' : '✗'}</span>
                  <span className="tx-status-text">{paramsStatus.message}</span>
                </div>
              </div>
            ) : (
              <div className="tx-params">
                <textarea
                  className={`tx-textarea tx-mono ${paramsStatus.valid ? '' : 'is-invalid'}`}
                  value={paramsText}
                  onChange={(e) => handleParamsJsonChange(e.target.value)}
                  spellCheck={false}
                  rows={6}
                  placeholder={'{\n  "size": 10\n}'}
                />
                <div className={`tx-params-status ${paramsStatus.valid ? 'ok' : 'bad'}`}>
                  <span className="tx-status-dot" aria-hidden />
                  <span className="tx-status-glyph" aria-hidden>{paramsStatus.valid ? '✓' : '✗'}</span>
                  <span className="tx-status-text">{paramsStatus.message}</span>
                  {paramsStatus.valid && paramsStatus.count !== undefined && paramsStatus.count > 0 && (
                    <span className="tx-status-count">· <strong>{paramsStatus.count}</strong> key{paramsStatus.count === 1 ? '' : 's'}</span>
                  )}
                </div>
              </div>
            )}
          </section>

          {/* IV — Tasks */}
          <section className="tx-section">
            <header className="tx-section-head">
              <span className="tx-section-index">04</span>
              <div className="tx-section-headings">
                <h3 className="tx-section-title">Tasks</h3>
                <p className="tx-section-sub">Each task is one <em>strategy</em> × <em>property</em> pairing. Extra keys pass through unchanged.</p>
              </div>
              <button className="tx-btn tx-btn-ghost tx-section-action" onClick={addTask} type="button">+ Task</button>
            </header>

            {(activeTest.tasks || []).length === 0 ? (
              <div className="tx-empty">
                <span className="tx-empty-rune" aria-hidden>◇</span>
                <p>No tasks configured yet.</p>
                <button className="tx-btn tx-btn-ghost" onClick={addTask} type="button">Add your first task</button>
              </div>
            ) : (
              <div className="tx-task-list">
                {(activeTest.tasks || []).map((task, taskIndex) => {
                  const extra = Object.entries(task).filter(([k]) => k !== 'strategy' && k !== 'property');
                  return (
                    <article key={taskIndex} className="tx-task">
                      <header className="tx-task-head">
                        <span className="tx-task-badge">Task {ord(taskIndex)}</span>
                        <div className="tx-task-head-actions">
                          <button
                            className="tx-linkbtn tx-task-head-link"
                            onClick={() => duplicateTask(taskIndex)}
                            type="button"
                            title="Duplicate this task"
                          >duplicate</button>
                          <button
                            className="tx-btn tx-btn-icon tx-btn-danger"
                            onClick={() => removeTask(taskIndex)}
                            aria-label="Remove task"
                            type="button"
                          >×</button>
                        </div>
                      </header>
                      <div className="tx-grid tx-grid-2 tx-task-primary">
                        <label className="tx-field">
                          <span className="tx-field-label">Strategy</span>
                          <input
                            type="text"
                            className="tx-input tx-mono"
                            value={task.strategy || ''}
                            onChange={(e) => updateTask(taskIndex, 'strategy', e.target.value)}
                            placeholder="e.g. bespoke, quickcheck"
                          />
                        </label>
                        <label className="tx-field">
                          <span className="tx-field-label">Property</span>
                          <input
                            type="text"
                            className="tx-input tx-mono"
                            value={task.property || ''}
                            onChange={(e) => updateTask(taskIndex, 'property', e.target.value)}
                            placeholder="e.g. prop_InsertValid"
                          />
                        </label>
                      </div>
                      {extra.length > 0 && (
                        <>
                          <div className="tx-task-divider"><span>Additional fields</span></div>
                          <div className="tx-task-extra">
                            {extra.map(([key, value], fieldIndex) => (
                              <div key={`${taskIndex}-${fieldIndex}`} className="tx-kv-row">
                                <input
                                  type="text"
                                  className="tx-input tx-mono tx-kv-key"
                                  value={key}
                                  onChange={(e) => renameTaskField(taskIndex, key, e.target.value)}
                                  placeholder="key"
                                />
                                <span className="tx-kv-sep" aria-hidden>=</span>
                                <input
                                  type="text"
                                  className="tx-input tx-mono tx-kv-value"
                                  value={value || ''}
                                  onChange={(e) => updateTask(taskIndex, key, e.target.value)}
                                  placeholder="value"
                                />
                                <button
                                  className="tx-btn tx-btn-icon tx-btn-ghost"
                                  onClick={() => removeTaskField(taskIndex, key)}
                                  aria-label={`Remove field ${key}`}
                                  type="button"
                                >×</button>
                              </div>
                            ))}
                          </div>
                        </>
                      )}
                      <footer className="tx-task-foot">
                        <button
                          className="tx-linkbtn"
                          onClick={() => addTaskField(taskIndex)}
                          type="button"
                        >
                          + add arbitrary field
                        </button>
                      </footer>
                    </article>
                  );
                })}
              </div>
            )}
          </section>
        </main>
      </div>
    </div>
  );
}

export default TestEditor;
