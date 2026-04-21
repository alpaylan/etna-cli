import { useState, useEffect, useRef, KeyboardEvent as ReactKeyboardEvent } from 'react';
import { vscode, TestInfo, TestDefinition, onMessage } from '../../api/vscodeApi';
import TestEditor from '../../components/TestEditor';

interface Props {
  experimentName: string;
  tests: TestInfo[];
  onFetchTests: (experimentName: string) => void;
  onQueuedRun: (count: number) => void;
}

interface EditingTest {
  testName: string;
  tests: TestDefinition[];
  isNew: boolean;
}

interface VariantSummary {
  title: string;
  trials: number;
  timeout: number;
  cross: boolean;
  mutations: string[];
  tasks: number;
}

interface TestSummary {
  short: string;
  rich: VariantSummary[];
}

const ord = (n: number) => String(n + 1).padStart(2, '0');

function summarize(defs: TestDefinition[] | undefined): TestSummary | null {
  if (!defs) return null;
  if (defs.length === 0) return { short: 'empty — no variants', rich: [] };
  const variants: VariantSummary[] = defs.map(d => ({
    title: d.workload?.trim() || '?',
    trials: d.trials,
    timeout: d.timeout,
    cross: !!d.cross,
    mutations: d.mutations || [],
    tasks: (d.tasks || []).length,
  }));
  const unique = [...new Set(variants.map(v => v.title))];
  let short: string;
  if (unique.length === 1) {
    short = defs.length === 1 ? unique[0] : `${defs.length} × ${unique[0]}`;
  } else if (unique.length <= 2) {
    short = `${defs.length} variants · ${unique.join(' · ')}`;
  } else {
    short = `${defs.length} variants · ${unique.slice(0, 2).join(' · ')} +${unique.length - 2}`;
  }
  return { short, rich: variants };
}

function TestsPanel({ experimentName, tests, onFetchTests, onQueuedRun }: Props) {
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [editingTest, setEditingTest] = useState<EditingTest | null>(null);
  const [loadingTest, setLoadingTest] = useState<string | null>(null);
  const [newTestOpen, setNewTestOpen] = useState(false);
  const [newTestName, setNewTestName] = useState('');
  const [testCache, setTestCache] = useState<Record<string, TestDefinition[]>>({});

  const newTestInputRef = useRef<HTMLInputElement>(null);
  const editIntentRef = useRef<string | null>(null);
  const requestedSummaryRef = useRef<Set<string>>(new Set());
  const initialFetchRef = useRef(false);

  useEffect(() => {
    const unsubscribe = onMessage((message) => {
      if (message.type !== 'testContent') return;
      const { experimentName: expName, testName, tests: defs } = message.data as {
        experimentName: string;
        testName: string;
        tests: TestDefinition[];
      };
      if (expName !== experimentName) return;
      setTestCache(prev => ({ ...prev, [testName]: defs }));
      if (editIntentRef.current === testName) {
        setEditingTest({ testName, tests: defs, isNew: false });
        setLoadingTest(null);
        editIntentRef.current = null;
      }
    });
    return unsubscribe;
  }, [experimentName]);

  useEffect(() => {
    if (initialFetchRef.current) return;
    initialFetchRef.current = true;
    onFetchTests(experimentName);
  }, [experimentName, onFetchTests]);

  useEffect(() => {
    if (newTestOpen) requestAnimationFrame(() => newTestInputRef.current?.focus());
  }, [newTestOpen]);

  useEffect(() => {
    for (const t of tests) {
      if (requestedSummaryRef.current.has(t.name)) continue;
      if (testCache[t.name]) { requestedSummaryRef.current.add(t.name); continue; }
      requestedSummaryRef.current.add(t.name);
      vscode.postMessage({ type: 'getTest', experimentName, testName: t.name });
    }
  }, [tests, testCache, experimentName]);

  const toggleTestSelection = (testName: string) => {
    setSelected(prev => {
      const next = new Set(prev);
      if (next.has(testName)) next.delete(testName); else next.add(testName);
      return next;
    });
  };

  const toggleAllTests = () => {
    const allSelected = tests.length > 0 && selected.size === tests.length;
    setSelected(allSelected ? new Set() : new Set(tests.map(t => t.name)));
  };

  const handleRun = () => {
    const testsToRun = selected.size === 0 ? tests.map(t => t.name) : Array.from(selected);
    vscode.postMessage({ type: 'runExperiment', name: experimentName, tests: testsToRun });
    onQueuedRun(testsToRun.length);
  };

  const handleEditTest = (testName: string) => {
    const cached = testCache[testName];
    if (cached) {
      setEditingTest({ testName, tests: cached, isNew: false });
      return;
    }
    editIntentRef.current = testName;
    setLoadingTest(testName);
    vscode.postMessage({ type: 'getTest', experimentName, testName });
  };

  const commitNewTest = () => {
    const name = newTestName.trim();
    if (!name) return;
    setEditingTest({
      testName: name,
      tests: [{
        workload: '',
        trials: 10,
        timeout: 60,
        mutations: ['base'],
        cross: false,
        params: {},
        tasks: [{ strategy: '', property: '' }],
      }],
      isNew: true,
    });
    setNewTestOpen(false);
    setNewTestName('');
  };

  const handleSaveTest = (defs: TestDefinition[]) => {
    if (!editingTest) return;
    vscode.postMessage({
      type: 'saveTest',
      experimentName,
      testName: editingTest.testName,
      tests: defs,
    });
    setTestCache(prev => ({ ...prev, [editingTest.testName]: defs }));
    requestedSummaryRef.current.add(editingTest.testName);
    setEditingTest(null);
  };

  const handleDeleteTest = () => {
    if (!editingTest || editingTest.isNew) return;
    if (!confirm(`Delete test "${editingTest.testName}"?`)) return;
    vscode.postMessage({
      type: 'deleteTest',
      experimentName,
      testName: editingTest.testName,
    });
    setTestCache(prev => {
      const { [editingTest.testName]: _dropped, ...rest } = prev;
      return rest;
    });
    requestedSummaryRef.current.delete(editingTest.testName);
    setEditingTest(null);
  };

  if (editingTest) {
    return (
      <TestEditor
        experimentName={experimentName}
        testName={editingTest.testName}
        initialTests={editingTest.tests}
        isNew={editingTest.isNew}
        onSave={handleSaveTest}
        onCancel={() => setEditingTest(null)}
        onDelete={editingTest.isNew ? undefined : handleDeleteTest}
      />
    );
  }

  const onNewTestKey = (e: ReactKeyboardEvent<HTMLInputElement>) => {
    if (e.key === 'Enter') commitNewTest();
    if (e.key === 'Escape') { setNewTestOpen(false); setNewTestName(''); }
  };

  return (
    <div className="ex-drawer ex-drawer-standalone">
      <header className="ex-drawer-head">
        <div className="ex-drawer-title">
          <span className="ex-drawer-label">Tests</span>
          <span className="ex-drawer-count">{tests.length.toString().padStart(2, '0')}</span>
        </div>
        <div className="ex-drawer-actions">
          {tests.length > 0 && (
            <button className="ex-linkbtn" onClick={toggleAllTests} type="button">
              {selected.size === tests.length ? 'deselect all' : 'select all'}
            </button>
          )}
          <button
            className="ex-btn ex-btn-ghost ex-btn-sm"
            onClick={() => setNewTestOpen(true)}
            type="button"
          >
            <span className="ex-btn-glyph" aria-hidden>+</span> New test
          </button>
          <button
            className="ex-btn ex-btn-primary ex-btn-sm"
            onClick={handleRun}
            disabled={tests.length === 0}
            title={selected.size > 0 ? `Run ${selected.size} selected` : 'Run all tests'}
          >
            {selected.size > 0 ? `Run ${selected.size}` : 'Run all'}
            <span className="ex-btn-glyph" aria-hidden>▶</span>
          </button>
        </div>
      </header>

      {newTestOpen && (
        <div className="ex-newtest">
          <span className="ex-newtest-tag">NEW TEST</span>
          <input
            ref={newTestInputRef}
            className="ex-input ex-mono ex-newtest-input"
            type="text"
            value={newTestName}
            onChange={(e) => setNewTestName(e.target.value)}
            onKeyDown={onNewTestKey}
            placeholder="test-name (no .json)"
          />
          <button
            className="ex-btn ex-btn-ghost ex-btn-sm"
            onClick={() => { setNewTestOpen(false); setNewTestName(''); }}
            type="button"
          >Cancel</button>
          <button
            className="ex-btn ex-btn-primary ex-btn-sm"
            onClick={commitNewTest}
            disabled={!newTestName.trim()}
            type="button"
          >Open editor</button>
        </div>
      )}

      {tests.length === 0 ? (
        <div className="ex-drawer-empty">
          <span aria-hidden>∅</span> No tests yet — create one with <strong>+ New test</strong>.
        </div>
      ) : (
        <ul className="ex-testlist">
          {tests.map((test) => {
            const checked = selected.has(test.name);
            const isLoading = loadingTest === test.name;
            const summary = summarize(testCache[test.name]);
            return (
              <li key={test.name} className={`ex-testitem ${checked ? 'is-selected' : ''}`}>
                <div className="ex-testitem-main">
                  <label className="ex-testitem-label">
                    <input
                      type="checkbox"
                      className="ex-checkbox"
                      checked={checked}
                      onChange={() => toggleTestSelection(test.name)}
                    />
                    <span className="ex-testitem-name">{test.name}</span>
                  </label>
                  <button
                    className="ex-linkbtn"
                    onClick={() => handleEditTest(test.name)}
                    disabled={isLoading}
                    type="button"
                  >
                    {isLoading ? 'loading…' : 'edit'}
                  </button>
                </div>
                <div className="ex-testitem-summary">
                  {summary ? summary.short : <span className="ex-skel">loading…</span>}
                </div>

                <div className="ex-testpop" role="tooltip" aria-hidden>
                  {!summary ? (
                    <div className="ex-testpop-empty">Loading test content…</div>
                  ) : summary.rich.length === 0 ? (
                    <div className="ex-testpop-empty">No variants configured.</div>
                  ) : (
                    <>
                      <div className="ex-testpop-head">
                        <span className="ex-testpop-name">{test.name}</span>
                        <span className="ex-testpop-count">
                          {summary.rich.length} variant{summary.rich.length === 1 ? '' : 's'}
                        </span>
                      </div>
                      <ul className="ex-testpop-list">
                        {summary.rich.map((v, idx) => (
                          <li key={idx} className="ex-testpop-item">
                            <span className="ex-testpop-index">{ord(idx)}</span>
                            <span className="ex-testpop-target">{v.title}</span>
                            <span className="ex-testpop-meta">
                              <span>{v.trials}× {v.timeout}s</span>
                              {v.tasks > 0 && <span>· {v.tasks} task{v.tasks === 1 ? '' : 's'}</span>}
                              {v.cross && <span className="ex-testpop-tag">cross</span>}
                            </span>
                            {v.mutations.length > 0 && (
                              <span className="ex-testpop-muts">
                                <span className="ex-testpop-muts-label">mut</span>
                                {v.mutations.map(m => (
                                  <span
                                    key={m}
                                    className={`ex-testpop-mut ${m === 'base' ? 'is-base' : ''}`}
                                  >{m}</span>
                                ))}
                              </span>
                            )}
                          </li>
                        ))}
                      </ul>
                    </>
                  )}
                </div>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}

export default TestsPanel;
