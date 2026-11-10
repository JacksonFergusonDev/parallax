/**
 * Parallax's GitHub releases, read once at build time. The release workflow
 * redeploys the site after publishing, so the download button and the version
 * list always name the newest release without anyone editing the site.
 *
 * Set PARALLAX_REQUIRE_RELEASES=1 (the deploy does) to fail the build when
 * GitHub can't be reached, rather than publishing a page with no downloads.
 * GITHUB_TOKEN, when set, avoids the anonymous API rate limit.
 * PARALLAX_RELEASES_FIXTURE names a JSON file of API responses to read
 * instead, which the site's tests build with.
 */
import { readFile } from 'node:fs/promises';
import { repository } from './site';

export interface Asset {
  name: string;
  url: string;
  size: number;
}

export interface Release {
  version: string;
  tag: string;
  name: string;
  date: Date;
  prerelease: boolean;
  notes: string;
  page: string;
  /** The universal macOS disk image. */
  dmg?: Asset;
  /** CLI archives by Rust target, such as aarch64-apple-darwin. */
  cli: Record<string, Asset>;
}

interface GitHubAsset {
  name: string;
  browser_download_url: string;
  size: number;
}

interface GitHubRelease {
  tag_name: string;
  name: string | null;
  draft: boolean;
  prerelease: boolean;
  published_at: string | null;
  body: string | null;
  html_url: string;
  assets: GitHubAsset[];
}

const CLI_ASSET = /^parallax-cli-.+-(aarch64-apple-darwin|x86_64-apple-darwin)\.tar\.gz$/;

export function toRelease(raw: GitHubRelease): Release | undefined {
  if (raw.draft || !raw.published_at) return undefined;
  const asset = (a: GitHubAsset): Asset => ({
    name: a.name,
    url: a.browser_download_url,
    size: a.size,
  });
  const dmg = raw.assets.find((a) => a.name.endsWith('_universal.dmg'));
  const cli: Record<string, Asset> = {};
  for (const a of raw.assets) {
    const target = CLI_ASSET.exec(a.name)?.[1];
    if (target) cli[target] = asset(a);
  }
  return {
    version: raw.tag_name.replace(/^v/, ''),
    tag: raw.tag_name,
    name: raw.name || raw.tag_name,
    date: new Date(raw.published_at),
    prerelease: raw.prerelease,
    notes: raw.body ?? '',
    page: raw.html_url,
    dmg: dmg && asset(dmg),
    cli,
  };
}

let cached: Promise<Release[]> | undefined;

/** Published releases, newest first. */
export function getReleases(): Promise<Release[]> {
  cached ??= load();
  return cached;
}

async function load(): Promise<Release[]> {
  const required = process.env.PARALLAX_REQUIRE_RELEASES === '1';
  try {
    const fixture = process.env.PARALLAX_RELEASES_FIXTURE;
    const raw = fixture ? await readFixture(fixture) : await fetchReleases();
    return raw
      .map(toRelease)
      .filter((r): r is Release => r !== undefined)
      .sort((a, b) => b.date.getTime() - a.date.getTime());
  } catch (error) {
    if (required) throw new Error(`Could not read Parallax's releases: ${String(error)}`);
    console.warn(`Building without releases: ${String(error)}`);
    return [];
  }
}

async function readFixture(path: string): Promise<GitHubRelease[]> {
  return JSON.parse(await readFile(path, 'utf8')) as GitHubRelease[];
}

async function fetchReleases(): Promise<GitHubRelease[]> {
  const headers: Record<string, string> = {
    Accept: 'application/vnd.github+json',
    'X-GitHub-Api-Version': '2022-11-28',
  };
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  const response = await fetch(`https://api.github.com/repos/${repository}/releases?per_page=100`, {
    headers,
  });
  if (!response.ok) throw new Error(`GitHub answered ${response.status}`);
  return (await response.json()) as GitHubRelease[];
}

/** The newest release that isn't a prerelease and has a disk image. */
export function latest(releases: Release[]): Release | undefined {
  return releases.find((r) => !r.prerelease && r.dmg);
}

export function formatSize(bytes: number): string {
  return `${(bytes / 1_000_000).toFixed(1)} MB`;
}

export function formatDate(date: Date): string {
  return date.toLocaleDateString('en-GB', { day: 'numeric', month: 'long', year: 'numeric' });
}
