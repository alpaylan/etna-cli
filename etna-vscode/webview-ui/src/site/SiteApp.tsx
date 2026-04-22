import { useEffect, useState } from 'react';
import CatalogPage from './CatalogPage';
import WorkloadDetailView from '../pages/workspace/WorkloadDetail';

/**
 * URL convention: `?w=<name>` shows the detail view for that workload,
 * otherwise the catalog gallery. Simple and bookmarkable — no router needed.
 */
function readWorkloadFromUrl(): string | null {
  const params = new URLSearchParams(window.location.search);
  const w = params.get('w');
  return w && w.length > 0 ? w : null;
}

function setWorkloadInUrl(name: string | null) {
  const url = new URL(window.location.href);
  if (name) {
    url.searchParams.set('w', name);
  } else {
    url.searchParams.delete('w');
  }
  window.history.pushState({ w: name }, '', url.toString());
}

function SiteApp() {
  const [workload, setWorkload] = useState<string | null>(() => readWorkloadFromUrl());

  // Respond to browser back/forward.
  useEffect(() => {
    const onPop = () => setWorkload(readWorkloadFromUrl());
    window.addEventListener('popstate', onPop);
    return () => window.removeEventListener('popstate', onPop);
  }, []);

  const select = (name: string | null) => {
    setWorkloadInUrl(name);
    setWorkload(name);
    window.scrollTo(0, 0);
  };

  if (workload) {
    return (
      // experimentName is empty — the static shim ignores it.
      <WorkloadDetailView
        experimentName=""
        workloadName={workload}
        onBack={() => select(null)}
      />
    );
  }
  return <CatalogPage onSelect={(name) => select(name)} />;
}

export default SiteApp;
