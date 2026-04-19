import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import { handleWebviewMessage } from './messageHandler';
import { ensureServerRunning } from '../server/serverManager';

let currentPanel: vscode.WebviewPanel | undefined;

export async function openDashboard(context: vscode.ExtensionContext): Promise<void> {
  const columnToShowIn = vscode.window.activeTextEditor
    ? vscode.window.activeTextEditor.viewColumn
    : undefined;

  if (currentPanel) {
    // If panel exists, reveal it
    currentPanel.reveal(columnToShowIn);
    return;
  }

  // Make sure a server is reachable (spawn one locally if needed).
  await ensureServerRunning();

  // Create a new panel
  currentPanel = vscode.window.createWebviewPanel(
    'etnaDashboard',
    'Etna Dashboard',
    columnToShowIn || vscode.ViewColumn.One,
    {
      enableScripts: true,
      retainContextWhenHidden: true,
      localResourceRoots: [
        vscode.Uri.joinPath(context.extensionUri, 'webview-ui', 'dist'),
        vscode.Uri.joinPath(context.extensionUri, 'webview-ui', 'build'),
      ],
    }
  );

  currentPanel.iconPath = {
    light: vscode.Uri.joinPath(context.extensionUri, 'resources', 'icon-light.svg'),
    dark: vscode.Uri.joinPath(context.extensionUri, 'resources', 'icon-dark.svg'),
  };

  // Set HTML content
  currentPanel.webview.html = getWebviewContent(currentPanel.webview, context.extensionUri);

  // Handle messages from the webview
  currentPanel.webview.onDidReceiveMessage(
    async (message) => {
      const response = await handleWebviewMessage(message);
      if (response && currentPanel) {
        currentPanel.webview.postMessage(response);
      }
    },
    undefined,
    context.subscriptions
  );

  // Handle panel disposal
  currentPanel.onDidDispose(
    () => {
      currentPanel = undefined;
    },
    undefined,
    context.subscriptions
  );
}

function getWebviewContent(webview: vscode.Webview, extensionUri: vscode.Uri): string {
  // Try to load the built React app
  const webviewDistPath = vscode.Uri.joinPath(extensionUri, 'webview-ui', 'dist');
  const webviewBuildPath = vscode.Uri.joinPath(extensionUri, 'webview-ui', 'build');

  // Check for Vite build output
  let indexPath = path.join(webviewDistPath.fsPath, 'index.html');
  let basePath = webviewDistPath;

  if (!fs.existsSync(indexPath)) {
    // Fall back to CRA build path
    indexPath = path.join(webviewBuildPath.fsPath, 'index.html');
    basePath = webviewBuildPath;
  }

  if (fs.existsSync(indexPath)) {
    let html = fs.readFileSync(indexPath, 'utf-8');

    // Replace asset paths with webview URIs
    const baseUri = webview.asWebviewUri(basePath);
    html = html.replace(/(href|src)="\/([^"]*)"/g, `$1="${baseUri}/$2"`);
    html = html.replace(/(href|src)="\.\/([^"]*)"/g, `$1="${baseUri}/$2"`);

    return html;
  }

  // If no build exists, return a placeholder
  return getPlaceholderHtml(webview);
}

function getPlaceholderHtml(webview: vscode.Webview): string {
  const nonce = getNonce();

  return `<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src ${webview.cspSource} 'unsafe-inline'; script-src 'nonce-${nonce}'; connect-src http://localhost:3000;">
    <title>Etna Dashboard</title>
    <style>
        body {
            font-family: var(--vscode-font-family);
            background-color: var(--vscode-editor-background);
            color: var(--vscode-editor-foreground);
            padding: 20px;
            margin: 0;
        }
        .container {
            max-width: 1200px;
            margin: 0 auto;
        }
        h1 {
            color: var(--vscode-titleBar-activeForeground);
            border-bottom: 1px solid var(--vscode-panel-border);
            padding-bottom: 10px;
        }
        .tabs {
            display: flex;
            gap: 10px;
            margin-bottom: 20px;
            border-bottom: 1px solid var(--vscode-panel-border);
        }
        .tab {
            padding: 10px 20px;
            cursor: pointer;
            background: none;
            border: none;
            color: var(--vscode-foreground);
            border-bottom: 2px solid transparent;
        }
        .tab:hover {
            background-color: var(--vscode-list-hoverBackground);
        }
        .tab.active {
            border-bottom-color: var(--vscode-focusBorder);
            color: var(--vscode-textLink-foreground);
        }
        .content {
            padding: 20px 0;
        }
        .card {
            background-color: var(--vscode-editor-background);
            border: 1px solid var(--vscode-panel-border);
            border-radius: 4px;
            padding: 15px;
            margin-bottom: 15px;
        }
        .card-title {
            font-weight: bold;
            margin-bottom: 10px;
        }
        button {
            background-color: var(--vscode-button-background);
            color: var(--vscode-button-foreground);
            border: none;
            padding: 8px 16px;
            cursor: pointer;
            border-radius: 2px;
        }
        button:hover {
            background-color: var(--vscode-button-hoverBackground);
        }
        .status-badge {
            display: inline-block;
            padding: 2px 8px;
            border-radius: 10px;
            font-size: 12px;
        }
        .status-running { background-color: var(--vscode-testing-runAction); color: white; }
        .status-completed { background-color: var(--vscode-testing-iconPassed); color: white; }
        .status-failed { background-color: var(--vscode-testing-iconFailed); color: white; }
        .status-pending { background-color: var(--vscode-testing-iconQueued); color: white; }
        table {
            width: 100%;
            border-collapse: collapse;
        }
        th, td {
            text-align: left;
            padding: 10px;
            border-bottom: 1px solid var(--vscode-panel-border);
        }
        th {
            background-color: var(--vscode-editor-inactiveSelectionBackground);
        }
        input, textarea {
            background-color: var(--vscode-input-background);
            color: var(--vscode-input-foreground);
            border: 1px solid var(--vscode-input-border);
            padding: 8px;
            width: 100%;
            box-sizing: border-box;
        }
        .loading {
            text-align: center;
            padding: 40px;
            color: var(--vscode-descriptionForeground);
        }
        .error {
            background-color: var(--vscode-inputValidation-errorBackground);
            border: 1px solid var(--vscode-inputValidation-errorBorder);
            color: var(--vscode-errorForeground);
            padding: 10px;
            border-radius: 4px;
            margin-bottom: 15px;
        }
        .empty-state {
            text-align: center;
            padding: 40px;
            color: var(--vscode-descriptionForeground);
        }
    </style>
</head>
<body>
    <div class="container">
        <h1>Etna Dashboard</h1>
        <div class="tabs">
            <button class="tab active" data-tab="experiments">Experiments</button>
            <button class="tab" data-tab="jobs">Jobs</button>
            <button class="tab" data-tab="metrics">Metrics</button>
        </div>
        <div id="content" class="content">
            <div class="loading">Loading...</div>
        </div>
    </div>
    <script nonce="${nonce}">
        const vscode = acquireVsCodeApi();
        let currentTab = 'experiments';
        let experiments = [];
        let jobs = [];
        let serverStatus = 'unknown';

        // Tab switching
        document.querySelectorAll('.tab').forEach(tab => {
            tab.addEventListener('click', () => {
                document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
                tab.classList.add('active');
                currentTab = tab.dataset.tab;
                renderContent();
            });
        });

        // Request initial data
        vscode.postMessage({ type: 'getExperiments' });
        vscode.postMessage({ type: 'getJobs' });
        vscode.postMessage({ type: 'healthCheck' });

        // Handle messages from extension
        window.addEventListener('message', event => {
            const message = event.data;
            switch (message.type) {
                case 'experiments':
                    experiments = message.data || [];
                    if (currentTab === 'experiments') renderContent();
                    break;
                case 'jobs':
                    jobs = message.data || [];
                    if (currentTab === 'jobs') renderContent();
                    break;
                case 'healthCheck':
                    serverStatus = message.data?.status || 'error';
                    break;
                case 'error':
                    showError(message.message);
                    break;
                case 'queryResult':
                    if (currentTab === 'metrics') renderMetricsResult(message.data);
                    break;
            }
        });

        function renderContent() {
            const content = document.getElementById('content');
            switch (currentTab) {
                case 'experiments':
                    content.innerHTML = renderExperiments();
                    attachExperimentHandlers();
                    break;
                case 'jobs':
                    content.innerHTML = renderJobs();
                    attachJobHandlers();
                    break;
                case 'metrics':
                    content.innerHTML = renderMetrics();
                    attachMetricsHandlers();
                    break;
            }
        }

        function renderExperiments() {
            if (experiments.length === 0) {
                return '<div class="empty-state"><p>No experiments found.</p><button onclick="createExperiment()">Create New Experiment</button></div>';
            }
            let html = '<div style="margin-bottom: 15px;"><button onclick="createExperiment()">+ New Experiment</button></div>';
            html += '<table><thead><tr><th>Name</th><th>Workloads</th><th>Actions</th></tr></thead><tbody>';
            experiments.forEach(exp => {
                const workloadCount = exp.workloads ? exp.workloads.length : 0;
                html += '<tr>';
                html += '<td><strong>' + escapeHtml(exp.name) + '</strong></td>';
                html += '<td>' + workloadCount + ' workload(s)</td>';
                html += '<td>';
                html += '<button onclick="runExperiment(\\''+escapeHtml(exp.name)+'\\')">Run</button> ';
                html += '<button onclick="deleteExperiment(\\''+escapeHtml(exp.name)+'\\')">Delete</button>';
                html += '</td></tr>';
            });
            html += '</tbody></table>';
            return html;
        }

        function renderJobs() {
            if (jobs.length === 0) {
                return '<div class="empty-state"><p>No jobs found.</p></div>';
            }
            let html = '<div style="margin-bottom: 15px;"><button onclick="refreshJobs()">Refresh</button></div>';
            html += '<table><thead><tr><th>ID</th><th>Type</th><th>Status</th><th>Created</th><th>Actions</th></tr></thead><tbody>';
            jobs.forEach(job => {
                html += '<tr>';
                html += '<td>' + escapeHtml(job.id.substring(0, 8)) + '...</td>';
                html += '<td>' + escapeHtml(job.job_type) + '</td>';
                html += '<td><span class="status-badge status-' + job.status + '">' + escapeHtml(job.status) + '</span></td>';
                html += '<td>' + new Date(job.created_at).toLocaleString() + '</td>';
                html += '<td>';
                if (job.status === 'running' || job.status === 'pending') {
                    html += '<button onclick="cancelJob(\\''+job.id+'\\')">Cancel</button>';
                }
                html += '</td></tr>';
            });
            html += '</tbody></table>';
            return html;
        }

        function renderMetrics() {
            return '<div class="card"><div class="card-title">Query Metrics</div>' +
                '<p>Enter a JQ filter expression to query metrics:</p>' +
                '<textarea id="jqFilter" rows="3" placeholder=".[] | select(.language == \\"Rust\\")">.[]</textarea>' +
                '<br><br><button onclick="queryMetrics()">Execute Query</button>' +
                '</div><div id="metricsResult"></div>';
        }

        function renderMetricsResult(data) {
            const resultDiv = document.getElementById('metricsResult');
            if (!resultDiv) return;
            if (!data || !data.metrics || data.metrics.length === 0) {
                resultDiv.innerHTML = '<div class="empty-state">No results found.</div>';
                return;
            }
            resultDiv.innerHTML = '<div class="card"><div class="card-title">Results (' + data.metrics.length + ')</div>' +
                '<pre style="overflow-x: auto; max-height: 400px;">' +
                escapeHtml(JSON.stringify(data.metrics, null, 2)) + '</pre></div>';
        }

        function attachExperimentHandlers() {}
        function attachJobHandlers() {}
        function attachMetricsHandlers() {}

        function createExperiment() {
            const name = prompt('Enter experiment name:');
            if (name) {
                vscode.postMessage({ type: 'createExperiment', name });
            }
        }

        function runExperiment(name) {
            vscode.postMessage({ type: 'runExperiment', name, tests: [] });
        }

        function deleteExperiment(name) {
            if (confirm('Delete experiment "' + name + '"?')) {
                vscode.postMessage({ type: 'deleteExperiment', name });
            }
        }

        function refreshJobs() {
            vscode.postMessage({ type: 'getJobs' });
        }

        function cancelJob(id) {
            if (confirm('Cancel job?')) {
                vscode.postMessage({ type: 'cancelJob', id });
            }
        }

        function queryMetrics() {
            const filter = document.getElementById('jqFilter').value;
            vscode.postMessage({ type: 'queryMetrics', filter });
        }

        function showError(message) {
            const content = document.getElementById('content');
            content.innerHTML = '<div class="error">' + escapeHtml(message) + '</div>' + content.innerHTML;
        }

        function escapeHtml(text) {
            if (!text) return '';
            return text.toString()
                .replace(/&/g, '&amp;')
                .replace(/</g, '&lt;')
                .replace(/>/g, '&gt;')
                .replace(/"/g, '&quot;')
                .replace(/'/g, '&#039;');
        }

        // Initial render
        renderContent();

        // Auto-refresh jobs
        setInterval(() => {
            if (currentTab === 'jobs') {
                vscode.postMessage({ type: 'getJobs' });
            }
        }, 5000);
    </script>
</body>
</html>`;
}

function getNonce(): string {
  let text = '';
  const possible = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
  for (let i = 0; i < 32; i++) {
    text += possible.charAt(Math.floor(Math.random() * possible.length));
  }
  return text;
}

export function getCurrentPanel(): vscode.WebviewPanel | undefined {
  return currentPanel;
}
