import { useState, useMemo, useRef, useEffect, KeyboardEvent as ReactKeyboardEvent } from 'react';
import { vscode, QueryResult } from '../api/vscodeApi';

interface Props {
  queryResult: QueryResult | null;
  experimentName?: string;
}

type SortDirection = 'asc' | 'desc';

// Leaf segment names that default to hidden (opaque identifiers, usually noise).
// Matches by the final path segment, so both `hash` and `foo.hash` hide by default.
// Users can restore them from the "Hidden columns" strip above the table.
const DEFAULT_HIDDEN_LEAF_NAMES = new Set(['hash']);

type ColumnPath = {
  segments: string[];
  display: string;
  leaf: string;
  // true when the column represents a plain object the user could expand into
  // per-key sub-columns. Tracked at collection time so the header can render
  // an expand affordance without re-inspecting the data.
  expandable: boolean;
};

const pathFromSegments = (segments: string[], expandable: boolean): ColumnPath => ({
  segments,
  display: segments.join('.'),
  leaf: segments[segments.length - 1] ?? '',
  expandable,
});

// Walk a row into leaf paths. Arrays and primitives are always leaves. Plain
// objects are leaves too *unless* the user has expanded their path — in which
// case we descend, so their keys become sibling columns.
function collectLeafPaths(
  value: unknown,
  prefix: string[],
  expandedPaths: Set<string>,
  out: { segments: string[]; expandable: boolean }[],
): void {
  const isPlainObject =
    value !== null && typeof value === 'object' && !Array.isArray(value);
  if (isPlainObject) {
    const entries = Object.entries(value as Record<string, unknown>);
    const here = prefix.join('.');
    if (entries.length > 0 && expandedPaths.has(here)) {
      for (const [k, sub] of entries) {
        collectLeafPaths(sub, [...prefix, k], expandedPaths, out);
      }
      return;
    }
    out.push({ segments: prefix, expandable: entries.length > 0 });
    return;
  }
  out.push({ segments: prefix, expandable: false });
}

function getValueAtPath(obj: unknown, segments: string[]): unknown {
  let cur: unknown = obj;
  for (const s of segments) {
    if (cur === null || cur === undefined || typeof cur !== 'object' || Array.isArray(cur)) {
      return undefined;
    }
    cur = (cur as Record<string, unknown>)[s];
  }
  return cur;
}

const EXAMPLES: { label: string; query: string }[] = [
  { label: 'all metrics', query: '.[]' },
  { label: 'rust only', query: '.[] | select(.language == "Rust")' },
  { label: 'failed', query: '.[] | select(.success == false)' },
  { label: 'group by workload', query: 'group_by(.workload) | map({workload: .[0].workload, count: length})' },
  { label: 'avg time by language', query: 'group_by(.language) | map({language: .[0].language, avg_time: (map(.time) | add / length)})' },
];

function MetricsPage({ queryResult, experimentName }: Props) {
  const [filter, setFilter] = useState('.[]');
  const [loading, setLoading] = useState(false);
  const [sortColumn, setSortColumn] = useState<string | null>(null);
  const [sortDirection, setSortDirection] = useState<SortDirection>('asc');
  const [columnFilters, setColumnFilters] = useState<Record<string, string>>({});
  const [viewMode, setViewMode] = useState<'table' | 'json'>('table');
  const [hiddenColumns, setHiddenColumns] = useState<Set<string>>(() => new Set());
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(() => new Set());
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  // Tracks columns we've already auto-hidden once, so users can un-hide them
  // and the default-hide rule won't keep fighting them.
  const seenColumnsRef = useRef<Set<string>>(new Set());

  const expandColumn = (display: string) => {
    setExpandedPaths(prev => {
      const next = new Set(prev);
      next.add(display);
      return next;
    });
  };
  const collapseColumn = (display: string) => {
    setExpandedPaths(prev => {
      if (!prev.has(display)) return prev;
      const next = new Set(prev);
      next.delete(display);
      return next;
    });
  };

  useEffect(() => {
    if (!loading) return;
    const t = setTimeout(() => setLoading(false), 5000);
    return () => clearTimeout(t);
  }, [loading]);

  const handleQuery = () => {
    setLoading(true);
    vscode.postMessage({ type: 'queryMetrics', filter, experimentName });
  };

  const runQuery = (f: string) => {
    setFilter(f);
    setLoading(true);
    vscode.postMessage({ type: 'queryMetrics', filter: f, experimentName });
  };

  // Render a value as a jq literal. JSON is a syntactic subset of jq, so any
  // JSON-serializable value can be dropped into a `select(.col == …)` expression
  // and jq's structural `==` will compare it correctly. Returns null for values
  // JSON can't represent (undefined, non-finite numbers, cycles).
  const toJqLiteral = (v: unknown): string | null => {
    if (v === undefined) return null;
    if (typeof v === 'number' && !Number.isFinite(v)) return null;
    try {
      const s = JSON.stringify(v);
      return s === undefined ? null : s;
    } catch {
      return null;
    }
  };

  const isJqIdent = (k: string) => /^[A-Za-z_][A-Za-z0-9_]*$/.test(k);
  const jqKey = (k: string) => isJqIdent(k) ? k : JSON.stringify(k);

  // Expand a value into a list of `path == literal` conjuncts. Plain objects
  // are flattened so each leaf gets its own clause (easy to edit/remove).
  // Arrays and primitives stay structural: `.muts == ["base"]`, `.n == 3`.
  const flattenToPredicates = (path: string, v: unknown): string[] | null => {
    if (v !== null && typeof v === 'object' && !Array.isArray(v)) {
      const entries = Object.entries(v as Record<string, unknown>);
      if (entries.length === 0) {
        const lit = toJqLiteral(v);
        return lit === null ? null : [`${path} == ${lit}`];
      }
      const out: string[] = [];
      for (const [k, sub] of entries) {
        const rec = flattenToPredicates(`${path}.${jqKey(k)}`, sub);
        if (rec === null) return null;
        out.push(...rec);
      }
      return out;
    }
    const lit = toJqLiteral(v);
    return lit === null ? null : [`${path} == ${lit}`];
  };

  const formatFilter = (preds: string[]): string => {
    if (preds.length === 1) return `.[] | select(${preds[0]})`;
    const indented = preds
      .map((p, i) => (i === 0 ? `  ${p}` : `  and ${p}`))
      .join('\n');
    return `.[]\n| select(\n${indented}\n  )`;
  };

  const filterByCell = (col: ColumnPath, value: unknown) => {
    const jqPath = '.' + col.segments.map(jqKey).join('.');
    const preds = flattenToPredicates(jqPath, value);
    if (!preds || preds.length === 0) return;
    runQuery(formatFilter(preds));
  };

  const onFilterKey = (e: ReactKeyboardEvent<HTMLTextAreaElement>) => {
    if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
      e.preventDefault();
      handleQuery();
    }
  };

  const allColumns = useMemo<ColumnPath[]>(() => {
    if (!queryResult?.metrics?.length) return [];
    // Union column paths across rows. `expandable: true` wins over false so a
    // column that's an object anywhere in the dataset exposes its chevron.
    const seen = new Map<string, { segments: string[]; expandable: boolean }>();
    for (const m of queryResult.metrics) {
      if (typeof m !== 'object' || m === null) continue;
      const leaves: { segments: string[]; expandable: boolean }[] = [];
      collectLeafPaths(m, [], expandedPaths, leaves);
      for (const { segments, expandable } of leaves) {
        const key = segments.join('\x00');
        const prior = seen.get(key);
        if (!prior) seen.set(key, { segments, expandable });
        else if (expandable && !prior.expandable) prior.expandable = true;
      }
    }
    const all = Array.from(seen.values()).map(v => pathFromSegments(v.segments, v.expandable));

    const priority = ['language', 'workload', 'status', 'time', 'trial', 'mutations', 'strategy', 'property'];
    const priIdx = (c: ColumnPath) => {
      if (c.segments.length !== 1) return priority.length + 1;
      const i = priority.indexOf(c.segments[0]);
      return i < 0 ? priority.length : i;
    };
    all.sort((a, b) => {
      const pa = priIdx(a);
      const pb = priIdx(b);
      if (pa !== pb) return pa - pb;
      // Keep sibling leaves adjacent: sort by first segment, then by full display.
      const fa = a.segments[0] ?? '';
      const fb = b.segments[0] ?? '';
      if (fa !== fb) return fa.localeCompare(fb);
      return a.display.localeCompare(b.display);
    });
    return all;
  }, [queryResult, expandedPaths]);

  // The first time a column appears, auto-hide it if its leaf is in the
  // default-hide list. After that, user toggles win — we don't re-hide.
  useEffect(() => {
    const newlySeen: string[] = [];
    for (const c of allColumns) {
      if (seenColumnsRef.current.has(c.display)) continue;
      seenColumnsRef.current.add(c.display);
      if (DEFAULT_HIDDEN_LEAF_NAMES.has(c.leaf)) newlySeen.push(c.display);
    }
    if (newlySeen.length === 0) return;
    setHiddenColumns(prev => {
      const next = new Set(prev);
      for (const d of newlySeen) next.add(d);
      return next;
    });
  }, [allColumns]);

  const columns = useMemo(
    () => allColumns.filter(c => !hiddenColumns.has(c.display)),
    [allColumns, hiddenColumns]
  );

  const hiddenPresent = useMemo(
    () => allColumns.filter(c => hiddenColumns.has(c.display)),
    [allColumns, hiddenColumns]
  );

  const hideColumn = (display: string) => setHiddenColumns(prev => new Set(prev).add(display));
  const showColumn = (display: string) => setHiddenColumns(prev => {
    const next = new Set(prev);
    next.delete(display);
    return next;
  });
  const showAllColumns = () => setHiddenColumns(new Set());

  const columnByDisplay = useMemo(() => {
    const m = new Map<string, ColumnPath>();
    for (const c of allColumns) m.set(c.display, c);
    return m;
  }, [allColumns]);

  const stringifyForSearch = (v: unknown): string => {
    if (v === null || v === undefined) return '';
    if (typeof v === 'object') {
      try { return JSON.stringify(v); } catch { return String(v); }
    }
    return String(v);
  };

  const processedMetrics = useMemo(() => {
    if (!queryResult?.metrics?.length) return [];
    let result = [...queryResult.metrics];

    Object.entries(columnFilters).forEach(([display, filterValue]) => {
      if (!filterValue.trim()) return;
      const col = columnByDisplay.get(display);
      if (!col) return;
      const lowerFilter = filterValue.toLowerCase();
      result = result.filter(m => {
        const value = getValueAtPath(m, col.segments);
        if (value === null || value === undefined) return false;
        return stringifyForSearch(value).toLowerCase().includes(lowerFilter);
      });
    });

    if (sortColumn) {
      const col = columnByDisplay.get(sortColumn);
      if (col) {
        result.sort((a, b) => {
          const aVal = getValueAtPath(a, col.segments);
          const bVal = getValueAtPath(b, col.segments);
          if (aVal === null || aVal === undefined) return 1;
          if (bVal === null || bVal === undefined) return -1;
          let comparison = 0;
          if (typeof aVal === 'number' && typeof bVal === 'number') comparison = aVal - bVal;
          else comparison = stringifyForSearch(aVal).localeCompare(stringifyForSearch(bVal));
          return sortDirection === 'asc' ? comparison : -comparison;
        });
      }
    }

    return result;
  }, [queryResult, columnFilters, sortColumn, sortDirection, columnByDisplay]);

  const handleSort = (column: string) => {
    if (sortColumn === column) setSortDirection(prev => prev === 'asc' ? 'desc' : 'asc');
    else { setSortColumn(column); setSortDirection('asc'); }
  };

  const handleFilterChange = (column: string, value: string) => {
    setColumnFilters(prev => ({ ...prev, [column]: value }));
  };

  const formatCellValue = (col: ColumnPath, value: unknown): string => {
    if (value === null || value === undefined) return '—';
    const leaf = col.leaf;
    if (leaf === 'time' && typeof value === 'string') {
      const match = value.match(/^(\d+)ns$/);
      if (match) {
        const ns = parseInt(match[1], 10);
        if (ns >= 1e9) return `${(ns / 1e9).toFixed(2)}s`;
        if (ns >= 1e6) return `${(ns / 1e6).toFixed(2)}ms`;
        if (ns >= 1e3) return `${(ns / 1e3).toFixed(2)}µs`;
        return `${ns}ns`;
      }
      return value;
    }
    // Hash-like opaque identifiers: short-form in the cell, full in the tooltip.
    // Match on the leaf segment so nested hashes (e.g. `blob.hash`) shorten too.
    if ((leaf === 'hash' || leaf.endsWith('_hash')) && typeof value === 'string' && value.length > 10) {
      return value.slice(0, 8) + '…';
    }
    if (Array.isArray(value)) return value.join(', ');
    if (typeof value === 'object') return JSON.stringify(value);
    return String(value);
  };

  const statusSlot = (status: string) => {
    const s = status?.toLowerCase();
    if (s === 'foundbug') return 'is-failed';
    if (s === 'timedout') return 'is-pending';
    if (s === 'aborted') return 'is-cancelled';
    if (s === 'passed') return 'is-passed';
    return '';
  };

  const activeFilterCount = Object.values(columnFilters).filter(v => v.trim()).length;

  return (
    <div className="mp-page">
      <header className="mp-pagehead">
        <div className="mp-pagehead-left">
          <span className="mp-kicker">Metrics</span>
          <p className="mp-lede">
            Query {experimentName ? <>the <code>{experimentName}</code> store</> : <>the metrics store</>} with <code>jq</code>.
          </p>
        </div>
      </header>

      <section className="mp-querycard">
        <div className="mp-querycard-head">
          <span className="mp-querycard-tag">JQ</span>
          <span className="mp-querycard-hint">
            <kbd>⌘</kbd>+<kbd>⏎</kbd> to execute
          </span>
        </div>
        <textarea
          ref={textareaRef}
          className="mp-querybox ex-mono"
          rows={Math.min(12, Math.max(3, filter.split('\n').length + 1))}
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
          onKeyDown={onFilterKey}
          spellCheck={false}
          placeholder='.[] | select(.language == "Rust")'
        />
        <div className="mp-querycard-foot">
          <div className="mp-examples" role="toolbar" aria-label="Example queries">
            <span className="mp-examples-label">examples</span>
            {EXAMPLES.map((ex) => (
              <button
                key={ex.label}
                type="button"
                className={`mp-example ${filter === ex.query ? 'is-active' : ''}`}
                onClick={() => setFilter(ex.query)}
                title={ex.query}
              >
                {ex.label}
              </button>
            ))}
          </div>
          <button
            className="ex-btn ex-btn-primary"
            onClick={handleQuery}
            disabled={loading}
          >
            {loading ? (
              <>
                <span className="mp-spinner" aria-hidden /> Executing…
              </>
            ) : (
              <>Execute <span className="ex-btn-glyph" aria-hidden>▶</span></>
            )}
          </button>
        </div>
      </section>

      {queryResult && (
        <section className="mp-results">
          <header className="mp-results-head">
            <div className="mp-results-title">
              <span className="mp-results-label">Results</span>
              <span className="mp-results-count">
                {processedMetrics.length.toString().padStart(2, '0')}
                <span className="mp-results-total"> / {(queryResult.metrics?.length || 0).toString().padStart(2, '0')}</span>
              </span>
              {activeFilterCount > 0 && (
                <span className="mp-results-filterbadge" title={`${activeFilterCount} column filter${activeFilterCount === 1 ? '' : 's'} active`}>
                  {activeFilterCount} filter{activeFilterCount === 1 ? '' : 's'}
                </span>
              )}
            </div>
            <div className="mp-viewtoggle" role="tablist" aria-label="View mode">
              <button
                type="button"
                role="tab"
                className={`mp-viewtoggle-opt ${viewMode === 'table' ? 'is-active' : ''}`}
                aria-selected={viewMode === 'table'}
                onClick={() => setViewMode('table')}
              >Table</button>
              <button
                type="button"
                role="tab"
                className={`mp-viewtoggle-opt ${viewMode === 'json' ? 'is-active' : ''}`}
                aria-selected={viewMode === 'json'}
                onClick={() => setViewMode('json')}
              >JSON</button>
            </div>
          </header>

          {queryResult.metrics && queryResult.metrics.length > 0 ? (
            viewMode === 'json' ? (
              <pre className="mp-json">
                {JSON.stringify(processedMetrics, null, 2)}
              </pre>
            ) : (
              <>
                {hiddenPresent.length > 0 && (
                  <div className="mp-hidden">
                    <span className="mp-hidden-label">Hidden</span>
                    {hiddenPresent.map(col => (
                      <button
                        key={col.display}
                        type="button"
                        className="mp-hidden-chip"
                        onClick={() => showColumn(col.display)}
                        title={`Show column "${col.display}"`}
                      >
                        <span className="mp-hidden-chip-plus" aria-hidden>+</span>
                        {col.display}
                      </button>
                    ))}
                    {hiddenPresent.length > 1 && (
                      <button
                        type="button"
                        className="mp-hidden-showall"
                        onClick={showAllColumns}
                      >show all</button>
                    )}
                  </div>
                )}
              <div className="mp-tablewrap">
                <table className="mp-table">
                  <thead>
                    <tr>
                      {columns.map(col => {
                        const isSorted = sortColumn === col.display;
                        const prefixSegs = col.segments.slice(0, -1);
                        return (
                          <th
                            key={col.display}
                            onClick={() => handleSort(col.display)}
                            className={`mp-th ${isSorted ? 'is-sorted' : ''} ${col.segments.length > 1 ? 'is-nested' : ''} ${col.expandable ? 'is-expandable' : ''}`}
                            title={`Click to sort by ${col.display}`}
                          >
                            <span className="mp-th-inner">
                              <span className="mp-th-label">
                                {prefixSegs.map((seg, i) => {
                                  const ancestor = prefixSegs.slice(0, i + 1).join('.');
                                  return (
                                    <button
                                      key={i}
                                      type="button"
                                      className="mp-th-prefix-seg"
                                      onClick={(e) => { e.stopPropagation(); collapseColumn(ancestor); }}
                                      title={`Collapse "${ancestor}" back into one column`}
                                    >
                                      {seg}
                                      <span className="mp-th-prefix-dot" aria-hidden>.</span>
                                    </button>
                                  );
                                })}
                                <span className="mp-th-leaf">{col.leaf}</span>
                                {col.expandable && (
                                  <button
                                    type="button"
                                    className="mp-th-expand"
                                    onClick={(e) => { e.stopPropagation(); expandColumn(col.display); }}
                                    aria-label={`Expand ${col.display} into per-key columns`}
                                    title={`Expand "${col.display}" into per-key columns`}
                                  >▸</button>
                                )}
                              </span>
                              <button
                                type="button"
                                className="mp-th-hide"
                                onClick={(e) => { e.stopPropagation(); hideColumn(col.display); }}
                                aria-label={`Hide column ${col.display}`}
                                title={`Hide "${col.display}"`}
                              >×</button>
                              <span className="mp-th-ind" aria-hidden>
                                {isSorted ? (sortDirection === 'asc' ? '↑' : '↓') : '↕'}
                              </span>
                            </span>
                          </th>
                        );
                      })}
                    </tr>
                    <tr className="mp-filterrow">
                      {columns.map(col => (
                        <th key={`filter-${col.display}`}>
                          <input
                            type="text"
                            placeholder="filter"
                            value={columnFilters[col.display] || ''}
                            onChange={(e) => handleFilterChange(col.display, e.target.value)}
                            className="mp-colfilter"
                          />
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {processedMetrics.map((metric, idx) => (
                      <tr key={idx}>
                        {columns.map(col => {
                          const value = getValueAtPath(metric, col.segments);
                          const displayValue = formatCellValue(col, value);
                          const canFilter = value !== undefined && toJqLiteral(value) !== null;
                          const cellTitle = canFilter
                            ? `${displayValue}  ·  click to filter by ${col.display} = ${displayValue}`
                            : displayValue;
                          const onCellClick = canFilter ? () => filterByCell(col, value) : undefined;
                          if (col.leaf === 'status' && col.segments.length === 1) {
                            return (
                              <td
                                key={col.display}
                                title={cellTitle}
                                className={canFilter ? 'mp-td is-filterable' : 'mp-td'}
                                onClick={onCellClick}
                              >
                                <span className={`mp-badge ${statusSlot(String(value))}`}>{displayValue}</span>
                              </td>
                            );
                          }
                          return (
                            <td
                              key={col.display}
                              title={cellTitle}
                              className={canFilter ? 'mp-td is-filterable' : 'mp-td'}
                              onClick={onCellClick}
                            >
                              <span className="mp-cell">{displayValue}</span>
                            </td>
                          );
                        })}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              </>
            )
          ) : (
            <div className="mp-empty">
              <span className="mp-empty-rune" aria-hidden>∅</span>
              <span>No results for this query.</span>
            </div>
          )}
        </section>
      )}
    </div>
  );
}

export default MetricsPage;
