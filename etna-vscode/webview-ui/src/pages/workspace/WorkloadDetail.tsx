import { useEffect, useMemo, useState } from 'react';
import { marked } from 'marked';
import {
  vscode,
  onMessage,
  WorkloadDetail as Detail,
  ManifestTaskGroup,
  SourceContext,
  InjectionSpec,
  BugDetails,
  Witness,
} from '../../api/vscodeApi';

interface Props {
  experimentName: string;
  workloadName: string;
  onBack: () => void;
}

type DocTab = 'bugs' | 'tasks' | 'readme';

const TAB_ORDER: DocTab[] = ['bugs', 'tasks', 'readme'];
const TAB_LABEL: Record<DocTab, string> = {
  bugs: 'BUGS.md',
  tasks: 'TASKS.md',
  readme: 'README.md',
};

function pickDoc(detail: Detail, tab: DocTab): string | null {
  switch (tab) {
    case 'bugs': return detail.bugs_md ?? null;
    case 'tasks': return detail.tasks_md ?? null;
    case 'readme': return detail.readme_md ?? null;
  }
}

function shortSha(sha: string): string {
  return sha.length >= 7 ? sha.slice(0, 7) : sha;
}

function witnessLabel(w: Witness): string {
  return 'input' in w ? w.input : w.test_fn;
}

function witnessKind(w: Witness): 'input' | 'test_fn' {
  return 'input' in w ? 'input' : 'test_fn';
}

/** Split a patch body into visually-classified lines for coloring. */
type PatchLineKind = 'add' | 'del' | 'hunk' | 'file' | 'ctx';
interface PatchLine { kind: PatchLineKind; text: string }

function classifyPatchLine(line: string): PatchLineKind {
  if (line.startsWith('+++') || line.startsWith('---') || line.startsWith('diff ') || line.startsWith('index ')) {
    return 'file';
  }
  if (line.startsWith('@@')) return 'hunk';
  if (line.startsWith('+')) return 'add';
  if (line.startsWith('-')) return 'del';
  return 'ctx';
}

function parsePatch(body: string): PatchLine[] {
  // Strip a trailing empty line so we don't render a ghost row.
  const lines = body.replace(/\n$/, '').split('\n');
  return lines.map((text) => ({ kind: classifyPatchLine(text), text }));
}

function SourceBlock({ source }: { source: SourceContext }) {
  const repo = source.repo.replace(/\/+$/, '');
  const commitLinks = source.commits.map((sha, i) => {
    const subject = source.commit_subjects?.[i];
    return (
      <li key={sha}>
        <a
          href={`${repo}/commit/${sha}`}
          target="_blank"
          rel="noreferrer"
          className="ex-mono"
          title={sha}
        >
          {shortSha(sha)}
        </a>
        {subject && <span style={{ marginLeft: 8 }}>{subject}</span>}
      </li>
    );
  });
  const prLinks = (source.prs ?? []).map((n) => (
    <a key={`pr-${n}`} href={`${repo}/pull/${n}`} target="_blank" rel="noreferrer" className="wl-ref-link">
      PR #{n}
    </a>
  ));
  const issueLinks = (source.issues ?? []).map((n) => (
    <a key={`iss-${n}`} href={`${repo}/issues/${n}`} target="_blank" rel="noreferrer" className="wl-ref-link">
      Issue #{n}
    </a>
  ));
  return (
    <div className="wl-subsection">
      <div className="wl-subsection-label">Source</div>
      <div className="wl-subsection-body">
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', alignItems: 'center' }}>
          <a href={repo} target="_blank" rel="noreferrer" className="wl-ref-link ex-mono">
            {repo.replace(/^https?:\/\//, '')}
          </a>
          {prLinks}
          {issueLinks}
          {source.discussion && (
            <a href={source.discussion} target="_blank" rel="noreferrer" className="wl-ref-link">
              discussion
            </a>
          )}
          {source.origin && <span className="ex-chip">{source.origin}</span>}
        </div>
        {source.commits.length > 0 && (
          <ul className="wl-commits">{commitLinks}</ul>
        )}
        <blockquote className="wl-summary">{source.summary}</blockquote>
      </div>
    </div>
  );
}

function BugBlock({ bug }: { bug: BugDetails }) {
  return (
    <div className="wl-subsection">
      <div className="wl-subsection-label">Bug</div>
      <div className="wl-subsection-body">
        <div className="ex-mono" style={{ fontWeight: 600 }}>{bug.short_name}</div>
        <div><span className="wl-kicker">Invariant.</span> {bug.invariant}</div>
        <div><span className="wl-kicker">Trigger.</span> {bug.how_triggered}</div>
      </div>
    </div>
  );
}

function InjectionBlock({
  injection,
  patchBody,
}: {
  injection: InjectionSpec;
  patchBody: string | null;
}) {
  const kindClass = injection.kind === 'patch' ? 'wl-kind-patch' : 'wl-kind-marauders';
  const locations = injection.locations ?? [];
  const lines = patchBody ? parsePatch(patchBody) : null;
  return (
    <div className="wl-subsection">
      <div className="wl-subsection-label">Injection</div>
      <div className="wl-subsection-body">
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap', alignItems: 'center' }}>
          <span className={`wl-kind-badge ${kindClass}`}>{injection.kind}</span>
          {injection.files.map((f) => (
            <span key={f} className="ex-chip ex-mono">{f}</span>
          ))}
        </div>
        {locations.length > 0 && (
          <ul className="wl-locations">
            {locations.map((loc, i) => (
              <li key={i} className="ex-mono">
                {loc.file}
                {loc.line != null && <>:{loc.line}</>}
                {loc.symbol && <span style={{ opacity: 0.7 }}> — {loc.symbol}</span>}
              </li>
            ))}
          </ul>
        )}
        {injection.patch && (
          <div className="wl-patch">
            <div className="wl-patch-head">
              <span className="ex-mono">{injection.patch}</span>
              {patchBody == null && (
                <span className="wl-patch-missing">not available</span>
              )}
            </div>
            {lines && (
              <pre className="wl-patch-body">
                {lines.map((l, i) => (
                  <div key={i} className={`wl-diff-line wl-diff-${l.kind}`}>
                    {l.text || ' '}
                  </div>
                ))}
              </pre>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function TasksTable({ group }: { group: ManifestTaskGroup }) {
  return (
    <div className="wl-subsection">
      <div className="wl-subsection-label">
        Properties · {group.tasks.length}
      </div>
      <table className="ex-table">
        <thead>
          <tr>
            <th style={{ width: '40%' }}>Property</th>
            <th>Witness(es)</th>
          </tr>
        </thead>
        <tbody>
          {group.tasks.map((t, i) => {
            const witnesses = t.witnesses ?? [];
            return (
              <tr key={`${t.property}-${i}`}>
                <td className="ex-mono">{t.property}</td>
                <td>
                  {witnesses.length === 0 ? (
                    <span style={{ opacity: 0.6 }}>—</span>
                  ) : (
                    <ul className="wl-witness-list">
                      {witnesses.map((w, j) => (
                        <li key={j}>
                          <span className={`wl-witness-kind wl-witness-${witnessKind(w)}`}>
                            {witnessKind(w) === 'input' ? 'input' : 'fn'}
                          </span>
                          <code className="wl-witness-val">{witnessLabel(w)}</code>
                          {w.note && <span className="wl-witness-note"> — {w.note}</span>}
                        </li>
                      ))}
                    </ul>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

function TaskGroupCard({
  group,
  index,
  patches,
}: {
  group: ManifestTaskGroup;
  index: number;
  patches: Record<string, string>;
}) {
  const [open, setOpen] = useState(index === 0);
  const mutationLabel = group.mutations.length === 0
    ? '(no mutations)'
    : group.mutations.join(', ');
  const patchBody = group.injection?.patch ? patches[group.injection.patch] ?? null : null;

  return (
    <div className={`wl-group ${open ? 'is-open' : ''}`}>
      <button
        type="button"
        className="wl-group-head"
        onClick={() => setOpen((v) => !v)}
        aria-expanded={open}
      >
        <span className="wl-group-caret" aria-hidden>{open ? '▾' : '▸'}</span>
        <span className="wl-group-index">#{index + 1}</span>
        <span className="wl-group-mutations ex-mono">{mutationLabel}</span>
        <span className="wl-group-meta">
          {group.bug && <span className="ex-chip">{group.bug.short_name}</span>}
          {group.injection && (
            <span className={`wl-kind-badge ${group.injection.kind === 'patch' ? 'wl-kind-patch' : 'wl-kind-marauders'}`}>
              {group.injection.kind}
            </span>
          )}
          <span className="wl-group-count">
            {group.tasks.length} prop{group.tasks.length === 1 ? '' : 's'}
          </span>
        </span>
      </button>
      {open && (
        <div className="wl-group-body">
          {group.source && <SourceBlock source={group.source} />}
          {group.bug && <BugBlock bug={group.bug} />}
          {group.injection && (
            <InjectionBlock injection={group.injection} patchBody={patchBody} />
          )}
          <TasksTable group={group} />
        </div>
      )}
    </div>
  );
}

function WorkloadDetailView({ experimentName, workloadName, onBack }: Props) {
  const [detail, setDetail] = useState<Detail | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<DocTab>('bugs');

  useEffect(() => {
    setDetail(null);
    setError(null);
    vscode.postMessage({
      type: 'getWorkloadDetail',
      experimentName,
      workload: workloadName,
    });
  }, [experimentName, workloadName]);

  useEffect(() => {
    const unsub = onMessage((m) => {
      if (m.type === 'workloadDetail') {
        const payload = m.data as {
          experimentName: string;
          workload: string;
          detail: Detail | null;
          error?: string;
        };
        if (payload.experimentName !== experimentName || payload.workload !== workloadName) return;
        if (payload.detail) {
          setDetail(payload.detail);
        } else {
          setError(payload.error || 'Failed to load workload detail');
        }
      } else if (m.type === 'error') {
        setError(m.message || 'Unknown error');
      }
    });
    return unsub;
  }, [experimentName, workloadName]);

  // Default the active tab to whichever doc actually exists, in priority
  // order BUGS → TASKS → README. Only runs once per detail load.
  useEffect(() => {
    if (!detail) return;
    const firstPresent = TAB_ORDER.find((t) => pickDoc(detail, t) !== null);
    if (firstPresent && pickDoc(detail, tab) === null) {
      setTab(firstPresent);
    }
  }, [detail]);

  const renderedBody = useMemo(() => {
    if (!detail) return '';
    const raw = pickDoc(detail, tab);
    if (raw == null) return '';
    return marked.parse(raw, { async: false }) as string;
  }, [detail, tab]);

  const allMutations = useMemo(() => {
    if (!detail) return [] as string[];
    const seen = new Set<string>();
    const out: string[] = [];
    for (const g of detail.manifest.tasks) {
      for (const m of g.mutations) {
        if (!seen.has(m)) { seen.add(m); out.push(m); }
      }
    }
    return out;
  }, [detail]);

  return (
    <div className="ex-drawer ex-drawer-standalone">
      <header className="ex-drawer-head" style={{ gap: 10, alignItems: 'center' }}>
        <button className="ex-btn ex-btn-ghost ex-btn-sm" onClick={onBack} type="button">
          <span className="ex-btn-glyph" aria-hidden>←</span> Workloads
        </button>
        <div className="ex-drawer-title">
          <span className="ex-drawer-label">Workload</span>
          <span className="ex-mono">{workloadName}</span>
        </div>
      </header>

      {error && <div className="error" role="alert">{error}</div>}

      {!detail && !error && <div className="ex-drawer-empty">Loading…</div>}

      {detail && (
        <>
          <section className="ex-drawer-section" aria-label="Manifest">
            <div style={{ display: 'flex', flexDirection: 'column', gap: 8 }}>
              <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexWrap: 'wrap' }}>
                <span className="ex-mono" style={{ fontWeight: 600 }}>{detail.manifest.name}</span>
                <span className="ex-chip">{detail.manifest.language}</span>
                {detail.manifest.crate && (
                  <span className="ex-chip ex-mono">crate: {detail.manifest.crate}</span>
                )}
                {detail.manifest.base_commit && (
                  <span className="ex-chip ex-mono" title={detail.manifest.base_commit}>
                    base {shortSha(detail.manifest.base_commit)}
                  </span>
                )}
              </div>
              {detail.manifest.description && (
                <div style={{ opacity: 0.85 }}>{detail.manifest.description}</div>
              )}
              {allMutations.length > 0 && (
                <div style={{ display: 'flex', gap: 4, flexWrap: 'wrap', marginTop: 4 }}>
                  {allMutations.map((m) => (
                    <span key={m} className="ex-chip ex-mono">{m}</span>
                  ))}
                </div>
              )}
            </div>
          </section>

          {detail.manifest.tasks.length > 0 && (
            <section className="ex-drawer-section" aria-label="Tasks">
              <div className="wl-section-header">
                Tasks
                <span className="wl-section-count">
                  {detail.manifest.tasks.length} group{detail.manifest.tasks.length === 1 ? '' : 's'}
                </span>
              </div>
              <div className="wl-group-list">
                {detail.manifest.tasks.map((g, i) => (
                  <TaskGroupCard
                    key={`${g.mutations.join(',')}-${i}`}
                    group={g}
                    index={i}
                    patches={detail.patches ?? {}}
                  />
                ))}
              </div>
            </section>
          )}

          {detail.manifest.dropped && detail.manifest.dropped.length > 0 && (
            <section className="ex-drawer-section" aria-label="Dropped candidates">
              <div className="wl-section-header">
                Dropped candidates
                <span className="wl-section-count">{detail.manifest.dropped.length}</span>
              </div>
              <ul className="wl-dropped-list">
                {detail.manifest.dropped.map((d) => (
                  <li key={d.commit}>
                    <code className="ex-mono">{shortSha(d.commit)}</code>
                    {d.subject && <span style={{ marginLeft: 8 }}>{d.subject}</span>}
                    <div style={{ opacity: 0.75, fontSize: 11.5, marginTop: 2 }}>{d.reason}</div>
                  </li>
                ))}
              </ul>
            </section>
          )}

          <nav className="ws-subtabs" role="tablist" aria-label="Workload docs" style={{ marginTop: 12 }}>
            {TAB_ORDER.map((t) => {
              const present = pickDoc(detail, t) !== null;
              return (
                <button
                  key={t}
                  role="tab"
                  aria-selected={tab === t}
                  className={`ws-subtab ${tab === t ? 'is-active' : ''}`}
                  onClick={() => setTab(t)}
                  disabled={!present}
                  title={present ? undefined : `${TAB_LABEL[t]} not found in this workload`}
                >
                  <span className="ws-subtab-label">{TAB_LABEL[t]}</span>
                  {!present && <span className="ws-subtab-count">—</span>}
                </button>
              );
            })}
          </nav>

          <section className="ex-drawer-section" aria-label={TAB_LABEL[tab]}>
            {pickDoc(detail, tab) == null ? (
              <div className="ex-drawer-empty">
                <span aria-hidden>∅</span> {TAB_LABEL[tab]} not present in this workload.
              </div>
            ) : (
              <div className="md-body" dangerouslySetInnerHTML={{ __html: renderedBody }} />
            )}
          </section>
        </>
      )}
    </div>
  );
}

export default WorkloadDetailView;
