import { useEffect, useMemo, useState } from 'react';
import { vscode, onMessage } from '../api/vscodeApi';

interface CatalogEntry {
  name: string;
  url: string;
  language: string;
  description?: string | null;
  default_ref?: string | null;
  status: string;
  tags: string[];
  has_manifest: boolean;
}

interface CatalogFile {
  schema_version: number;
  generated_at: string;
  entries: CatalogEntry[];
}

interface Props {
  onSelect: (name: string) => void;
}

function CatalogPage({ onSelect }: Props) {
  const [catalog, setCatalog] = useState<CatalogFile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [languageFilter, setLanguageFilter] = useState<string>('all');

  useEffect(() => {
    vscode.postMessage({ type: 'getAvailableWorkloads' });
  }, []);

  useEffect(() => {
    return onMessage((m) => {
      if (m.type === 'catalog') {
        setCatalog(m.data as CatalogFile);
      } else if (m.type === 'error') {
        setError(m.message ?? 'Unknown error');
      }
    });
  }, []);

  const languages = useMemo(() => {
    if (!catalog) return [] as string[];
    const s = new Set<string>();
    for (const e of catalog.entries) s.add(e.language);
    return Array.from(s).sort();
  }, [catalog]);

  const visible = useMemo(() => {
    if (!catalog) return [] as CatalogEntry[];
    const q = query.trim().toLowerCase();
    return catalog.entries.filter((e) => {
      if (languageFilter !== 'all' && e.language !== languageFilter) return false;
      if (!q) return true;
      return (
        e.name.toLowerCase().includes(q) ||
        (e.description ?? '').toLowerCase().includes(q) ||
        e.tags.some((t) => t.toLowerCase().includes(q))
      );
    });
  }, [catalog, query, languageFilter]);

  return (
    <div className="site-shell">
      <header className="site-head">
        <div>
          <h1 className="site-title">Etna Workload Catalog</h1>
          <div className="site-sub">
            Browse every property-based testing workload in the Etna index — rendered
            straight from each repo's <code>etna.toml</code>.
          </div>
        </div>
        {catalog && (
          <div className="site-stats">
            <span>{catalog.entries.length} workloads</span>
            <span>·</span>
            <span>{catalog.entries.filter((e) => e.has_manifest).length} with manifest</span>
            <span>·</span>
            <span>{languages.length} languages</span>
          </div>
        )}
      </header>

      <div className="site-filters">
        <input
          type="text"
          className="ex-input ex-mono"
          placeholder="search name, description, tag…"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          style={{ flex: '1 1 260px', maxWidth: 420 }}
          spellCheck={false}
          autoComplete="off"
        />
        <div className="site-chips" role="tablist">
          <button
            className={`site-chip ${languageFilter === 'all' ? 'is-active' : ''}`}
            onClick={() => setLanguageFilter('all')}
            type="button"
          >
            all
          </button>
          {languages.map((lang) => (
            <button
              key={lang}
              className={`site-chip ${languageFilter === lang ? 'is-active' : ''}`}
              onClick={() => setLanguageFilter(lang)}
              type="button"
            >
              {lang}
            </button>
          ))}
        </div>
      </div>

      {error && <div className="error" role="alert">{error}</div>}

      {!catalog && !error && <div className="ex-drawer-empty">Loading catalog…</div>}

      {catalog && visible.length === 0 && (
        <div className="ex-drawer-empty">
          <span aria-hidden>∅</span> No workloads match the current filters.
        </div>
      )}

      <div className="site-grid">
        {visible.map((entry) => (
          <button
            key={entry.name}
            className={`site-card ${entry.has_manifest ? '' : 'is-stub'}`}
            type="button"
            onClick={() => {
              if (entry.has_manifest) onSelect(entry.name);
            }}
            disabled={!entry.has_manifest}
            title={
              entry.has_manifest
                ? `View ${entry.name}`
                : `${entry.name}: no etna.toml published yet`
            }
          >
            <div className="site-card-head">
              <span className="site-card-name ex-mono">{entry.name}</span>
              <span className="ex-chip">{entry.language}</span>
            </div>
            {entry.description && (
              <div className="site-card-desc">{entry.description}</div>
            )}
            <div className="site-card-foot">
              {entry.status !== 'stable' && (
                <span className="ex-chip">{entry.status}</span>
              )}
              {entry.tags.map((t) => (
                <span key={t} className="ex-chip">{t}</span>
              ))}
              {!entry.has_manifest && (
                <span className="site-missing">no manifest</span>
              )}
            </div>
          </button>
        ))}
      </div>

      {catalog && (
        <footer className="site-foot">
          Generated {new Date(catalog.generated_at).toLocaleString()}.
        </footer>
      )}
    </div>
  );
}

export default CatalogPage;
