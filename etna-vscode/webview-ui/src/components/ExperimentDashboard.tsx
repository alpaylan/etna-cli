import { useMemo, useState, useEffect, useRef } from 'react';
import { SERVER_URL } from '../api/browserShim';

interface Props {
  experimentName: string;
  onBack: () => void;
}

export default function ExperimentDashboard({ experimentName, onBack }: Props) {
  const [reloadKey, setReloadKey] = useState(0);
  const [loading, setLoading] = useState(true);
  const iframeRef = useRef<HTMLIFrameElement>(null);

  const src = useMemo(
    () =>
      `${SERVER_URL}/api/v1/experiments/${encodeURIComponent(experimentName)}/report?_=${reloadKey}`,
    [experimentName, reloadKey]
  );

  useEffect(() => {
    setLoading(true);
  }, [reloadKey]);

  return (
    <div className="ed-page">
      <header className="ed-head">
        <button className="ex-btn ex-btn-ghost" onClick={onBack}>
          <span className="ex-btn-glyph" aria-hidden>←</span> Back
        </button>
        <div className="ed-crumb">
          <span className="ed-crumb-tag">Report</span>
          <span className="ed-crumb-value">{experimentName}</span>
        </div>
        <button
          className="ex-btn ex-btn-ghost"
          onClick={() => setReloadKey((k) => k + 1)}
          title="Re-run the report generator"
        >
          <span className="ex-btn-glyph" aria-hidden>↻</span> Regenerate
        </button>
      </header>

      <div className={`ed-frame ${loading ? 'is-loading' : ''}`}>
        {loading && (
          <div className="ed-loading" aria-hidden>
            <span className="ed-loading-dot" />
            <span className="ed-loading-text">Rendering report…</span>
          </div>
        )}
        <iframe
          ref={iframeRef}
          key={reloadKey}
          src={src}
          title={`Dashboard: ${experimentName}`}
          className="ed-iframe"
          onLoad={() => setLoading(false)}
        />
      </div>
    </div>
  );
}
