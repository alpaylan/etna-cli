// Cloudflare Worker: serves GitHub Gist files with proper Content-Type
// URL format: https://<worker>.workers.dev/<gist_id>/<filename>
// If filename is omitted, defaults to the first .html file or first file in the gist.

export default {
  async fetch(request) {
    const url = new URL(request.url);
    const parts = url.pathname.slice(1).split('/').filter(Boolean);

    if (parts.length === 0) {
      return new Response('Usage: /<gist_id>/<filename>\n', {
        headers: { 'content-type': 'text/plain' },
      });
    }

    const gistId = parts[0];
    const requestedFile = parts.slice(1).join('/') || '';

    // Fetch gist metadata from GitHub API
    const apiRes = await fetch(`https://api.github.com/gists/${gistId}`, {
      headers: { 'User-Agent': 'etna-report-worker' },
    });

    if (!apiRes.ok) {
      return new Response(`Gist not found: ${gistId}\n`, { status: 404 });
    }

    const gist = await apiRes.json();
    const files = gist.files || {};
    const fileNames = Object.keys(files);

    if (fileNames.length === 0) {
      return new Response('Gist has no files\n', { status: 404 });
    }

    // Resolve filename
    let fileName = requestedFile;
    if (!fileName) {
      fileName = fileNames.find(f => f.endsWith('.html')) || fileNames[0];
    }

    const fileInfo = files[fileName];
    if (!fileInfo) {
      return new Response(`File not found: ${fileName}\nAvailable: ${fileNames.join(', ')}\n`, { status: 404 });
    }

    // Fetch full content (handles truncated files via raw_url)
    let content;
    if (fileInfo.truncated && fileInfo.raw_url) {
      const rawRes = await fetch(fileInfo.raw_url, {
        headers: { 'User-Agent': 'etna-report-worker' },
      });
      content = await rawRes.text();
    } else {
      content = fileInfo.content;
    }

    // Guess content type
    const ext = fileName.split('.').pop().toLowerCase();
    const types = {
      html: 'text/html', css: 'text/css', js: 'application/javascript',
      json: 'application/json', svg: 'image/svg+xml', txt: 'text/plain',
    };
    const contentType = types[ext] || 'text/plain';

    return new Response(content, {
      headers: {
        'content-type': `${contentType}; charset=utf-8`,
        'access-control-allow-origin': '*',
        'cache-control': 'public, max-age=300',
      },
    });
  },
};
