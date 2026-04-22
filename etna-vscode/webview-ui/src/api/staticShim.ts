// Static-site shim: emulates the same `postMessage` protocol the extension
// webview and browser-dev mode use, but sources data from JSON files emitted
// by `etna workload site` rather than a live server.
//
// Only implements the subset of message types the catalog view needs —
// everything else is answered with an `error` message so unexpected calls
// surface clearly instead of silently hanging.

type Msg = { type: string; [k: string]: unknown };

/** Present when this bundle was loaded from `site.html`. */
export function isStaticMode(): boolean {
  const meta = document.querySelector('meta[name="etna-mode"]');
  return meta?.getAttribute('content') === 'static';
}

function dispatch(message: unknown): void {
  window.postMessage(message, window.location.origin);
}

/** Fetch JSON relative to the HTML entry point so the site is portable across
 *  hosting paths (GH Pages project subpath, Netlify root, `file://`, etc.). */
async function fetchData<T>(rel: string): Promise<T> {
  const res = await fetch(`./data/${rel}`);
  if (!res.ok) {
    throw new Error(`GET data/${rel} → ${res.status} ${res.statusText}`);
  }
  return (await res.json()) as T;
}

/** Shape of `data/catalog.json` as emitted by `src/commands/workload/site.rs`. */
interface CatalogFile {
  schema_version: number;
  generated_at: string;
  entries: Array<{
    name: string;
    url: string;
    language: string;
    description?: string | null;
    default_ref?: string | null;
    status: string;
    tags: string[];
    has_manifest: boolean;
  }>;
}

export async function handleStaticMessage(message: Msg): Promise<void> {
  try {
    switch (message.type) {
      case 'getAvailableWorkloads': {
        const catalog = await fetchData<CatalogFile>('catalog.json');
        dispatch({ type: 'availableWorkloads', data: { workloads: catalog.entries } });
        dispatch({ type: 'catalog', data: catalog });
        return;
      }
      case 'getWorkloadDetail': {
        // The webview's workload-detail flow is keyed by
        // (experimentName, workload). In static mode there's no experiment —
        // we key purely on the workload name.
        const workload = message.workload as string;
        try {
          const detail = await fetchData(`workloads/${encodeURIComponent(workload)}.json`);
          dispatch({
            type: 'workloadDetail',
            data: {
              experimentName: message.experimentName ?? '',
              workload,
              detail,
            },
          });
        } catch (err) {
          const emsg = err instanceof Error ? err.message : String(err);
          dispatch({
            type: 'workloadDetail',
            data: {
              experimentName: message.experimentName ?? '',
              workload,
              detail: null,
              error: emsg,
            },
          });
        }
        return;
      }
      default:
        dispatch({
          type: 'error',
          message: `Static mode does not support message '${message.type}'`,
        });
    }
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    dispatch({ type: 'error', message: msg });
  }
}
