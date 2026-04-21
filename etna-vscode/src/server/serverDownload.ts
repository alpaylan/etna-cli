import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import * as fsp from 'fs/promises';
import * as os from 'os';
import * as cp from 'child_process';
import * as crypto from 'crypto';
import axios from 'axios';

const REPO = 'alpaylan/etna-cli';
const EXE = process.platform === 'win32' ? 'etna-server.exe' : 'etna-server';

interface DistArtifact {
  kind: string;
  target_triples?: string[];
  checksum?: string;
  assets?: { name: string; kind: string }[];
}

interface DistManifest {
  announcement_tag?: string;
  artifacts: Record<string, DistArtifact>;
}

function currentTargetTriple(): string {
  const p = process.platform;
  const a = process.arch;
  if (p === 'darwin' && a === 'arm64') return 'aarch64-apple-darwin';
  if (p === 'darwin' && a === 'x64') return 'x86_64-apple-darwin';
  if (p === 'linux' && a === 'x64') return 'x86_64-unknown-linux-gnu';
  if (p === 'win32' && a === 'x64') return 'x86_64-pc-windows-msvc';
  throw new Error(`Unsupported platform/arch for etna-server download: ${p}/${a}`);
}

async function fileExists(p: string): Promise<boolean> {
  try {
    await fsp.access(p);
    return true;
  } catch {
    return false;
  }
}

async function fetchLatestTag(): Promise<string> {
  const res = await axios.get<{ tag_name?: string }>(
    `https://api.github.com/repos/${REPO}/releases/latest`,
    { headers: { Accept: 'application/vnd.github+json' }, timeout: 15000 }
  );
  const tag = res.data?.tag_name;
  if (typeof tag !== 'string' || tag.length === 0) {
    throw new Error(`GitHub release response missing tag_name for ${REPO}`);
  }
  return tag;
}

async function fetchManifest(tag: string): Promise<DistManifest> {
  const url = `https://github.com/${REPO}/releases/download/${tag}/dist-manifest.json`;
  const res = await axios.get<DistManifest>(url, { timeout: 15000 });
  if (!res.data?.artifacts) {
    throw new Error(`dist-manifest.json at ${url} is malformed`);
  }
  return res.data;
}

function pickArtifact(manifest: DistManifest, triple: string): { archive: string; checksum: string } {
  for (const [name, art] of Object.entries(manifest.artifacts)) {
    if (art.kind !== 'executable-zip') continue;
    if (!art.target_triples?.includes(triple)) continue;
    if (!art.checksum) {
      throw new Error(`Artifact ${name} has no checksum sidecar in dist-manifest.json`);
    }
    return { archive: name, checksum: art.checksum };
  }
  throw new Error(`No release artifact for target ${triple} in dist-manifest.json`);
}

async function downloadTo(url: string, dest: string): Promise<void> {
  const res = await axios.get(url, { responseType: 'stream', timeout: 120000 });
  await new Promise<void>((resolve, reject) => {
    const out = fs.createWriteStream(dest);
    res.data.on('error', reject);
    out.on('error', reject);
    out.on('finish', () => resolve());
    res.data.pipe(out);
  });
}

async function sha256OfFile(p: string): Promise<string> {
  const h = crypto.createHash('sha256');
  await new Promise<void>((resolve, reject) => {
    const s = fs.createReadStream(p);
    s.on('data', (d) => h.update(d as Buffer));
    s.on('end', () => resolve());
    s.on('error', reject);
  });
  return h.digest('hex');
}

function extractArchive(archive: string, dest: string): void {
  // Rely on `tar` — present on macOS, Linux, and Windows 10+ (System32\tar.exe).
  // BSD tar auto-detects xz/gz/zip, so a single `tar -xf` handles all release formats.
  const r = cp.spawnSync('tar', ['-xf', archive, '-C', dest], { stdio: 'pipe' });
  if (r.error) {
    throw new Error(`Failed to invoke tar: ${r.error.message}`);
  }
  if (r.status !== 0) {
    const stderr = r.stderr?.toString() ?? '';
    throw new Error(`tar -xf ${path.basename(archive)} failed (exit ${r.status}): ${stderr}`);
  }
}

async function findServerBinary(root: string): Promise<string> {
  const stack = [root];
  while (stack.length) {
    const dir = stack.pop()!;
    const entries = await fsp.readdir(dir, { withFileTypes: true });
    for (const e of entries) {
      const full = path.join(dir, e.name);
      if (e.isDirectory()) stack.push(full);
      else if (e.isFile() && e.name === EXE) return full;
    }
  }
  throw new Error(`${EXE} not found in extracted archive`);
}

/**
 * Resolve an executable etna-server binary, downloading the latest release from
 * GitHub into the extension's globalStorage cache on first use. Throws on any
 * failure; callers are expected to surface the error to the user.
 */
export async function ensureDownloadedBinary(context: vscode.ExtensionContext): Promise<string> {
  const triple = currentTargetTriple();
  const tag = await fetchLatestTag();
  const versionDir = path.join(context.globalStorageUri.fsPath, 'server', tag);
  const cachedBinary = path.join(versionDir, EXE);

  if (await fileExists(cachedBinary)) return cachedBinary;

  await fsp.mkdir(versionDir, { recursive: true });

  await vscode.window.withProgress(
    {
      location: vscode.ProgressLocation.Notification,
      title: `Downloading etna-server ${tag}`,
      cancellable: false,
    },
    async (progress) => {
      progress.report({ message: 'fetching manifest' });
      const manifest = await fetchManifest(tag);
      const { archive, checksum } = pickArtifact(manifest, triple);
      const base = `https://github.com/${REPO}/releases/download/${tag}`;

      const tmpDir = await fsp.mkdtemp(path.join(os.tmpdir(), 'etna-dl-'));
      try {
        const archivePath = path.join(tmpDir, archive);
        const checksumPath = path.join(tmpDir, checksum);

        progress.report({ message: `downloading ${archive}` });
        await downloadTo(`${base}/${archive}`, archivePath);
        await downloadTo(`${base}/${checksum}`, checksumPath);

        progress.report({ message: 'verifying checksum' });
        const expected = (await fsp.readFile(checksumPath, 'utf8')).trim().split(/\s+/)[0].toLowerCase();
        const actual = (await sha256OfFile(archivePath)).toLowerCase();
        if (!expected || expected !== actual) {
          throw new Error(`Checksum mismatch for ${archive} (expected ${expected || '<empty>'}, got ${actual})`);
        }

        progress.report({ message: 'extracting' });
        const extractDir = path.join(tmpDir, 'ex');
        await fsp.mkdir(extractDir, { recursive: true });
        extractArchive(archivePath, extractDir);
        const extractedBin = await findServerBinary(extractDir);
        await fsp.rename(extractedBin, cachedBinary);
        if (process.platform !== 'win32') {
          await fsp.chmod(cachedBinary, 0o755);
        }
      } finally {
        await fsp.rm(tmpDir, { recursive: true, force: true }).catch(() => undefined);
      }
    }
  );

  return cachedBinary;
}
