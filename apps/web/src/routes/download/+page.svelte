<script lang="ts">
  import { onMount } from 'svelte';

  const REPO = 'mkpoli/phonia';
  const RELEASES_URL = `https://github.com/${REPO}/releases`;
  // The list endpoint (not /releases/latest) so prereleases and in-progress
  // builds still resolve; we take the newest release that carries installers.
  const RELEASES_API = `https://api.github.com/repos/${REPO}/releases?per_page=8`;

  type OsId = 'mac' | 'windows' | 'linux';

  interface Build {
    /** The download URL for this platform's primary installer, once resolved. */
    href: string | null;
    /** A secondary format for the same platform (Linux .deb), when present. */
    altHref?: string | null;
    altLabel?: string;
  }

  const PLATFORMS: { id: OsId; name: string; note: string }[] = [
    { id: 'mac', name: 'macOS', note: 'Universal — Apple Silicon and Intel · .dmg' },
    { id: 'windows', name: 'Windows', note: 'Installer · .exe' },
    { id: 'linux', name: 'Linux', note: 'Portable · .AppImage' }
  ];

  let detected = $state<OsId | null>(null);
  let version = $state<string | null>(null);
  let builds = $state<Record<OsId, Build>>({
    mac: { href: null },
    windows: { href: null },
    linux: { href: null }
  });
  // 'loading' until the release resolves; 'ready' with at least one asset;
  // 'pending' when no published release carries installers yet.
  let status = $state<'loading' | 'ready' | 'pending'>('loading');

  function detectOs(): OsId | null {
    const s = `${navigator.userAgent} ${navigator.platform}`.toLowerCase();
    if (s.includes('mac')) return 'mac';
    if (s.includes('win')) return 'windows';
    if (s.includes('linux') || s.includes('x11')) return 'linux';
    return null;
  }

  function pick(assets: { name: string; browser_download_url: string }[], test: (n: string) => boolean) {
    return assets.find((a) => test(a.name.toLowerCase()))?.browser_download_url ?? null;
  }

  onMount(async () => {
    detected = detectOs();
    try {
      const res = await fetch(RELEASES_API, { headers: { Accept: 'application/vnd.github+json' } });
      if (!res.ok) throw new Error(String(res.status));
      const list: { draft: boolean; tag_name: string; assets: { name: string; browser_download_url: string }[] }[] =
        await res.json();
      const rel = list.find((r) => !r.draft && (r.assets?.length ?? 0) > 0) ?? list.find((r) => !r.draft);
      const assets = rel?.assets ?? [];
      version = (rel?.tag_name ?? '').replace(/^desktop-v?/, '') || null;
      const resolved: Record<OsId, Build> = {
        mac: { href: pick(assets, (n) => n.endsWith('.dmg')) },
        windows: {
          href:
            pick(assets, (n) => n.endsWith('-setup.exe')) ??
            pick(assets, (n) => n.endsWith('.exe')) ??
            pick(assets, (n) => n.endsWith('.msi'))
        },
        linux: {
          href: pick(assets, (n) => n.endsWith('.appimage')),
          altHref: pick(assets, (n) => n.endsWith('.deb')),
          altLabel: '.deb'
        }
      };
      builds = resolved;
      status = Object.values(resolved).some((b) => b.href) ? 'ready' : 'pending';
    } catch {
      // No release yet, rate-limited, or offline: fall back to the releases page.
      status = 'pending';
    }
  });

  const ordered = $derived(
    detected ? [...PLATFORMS].sort((a) => (a.id === detected ? -1 : 0)) : PLATFORMS
  );
</script>

<svelte:head>
  <title>Download Phonia for desktop</title>
  <meta
    name="description"
    content="Download the offline desktop build of Phonia for macOS, Windows, or Linux. Free and open source."
  />
  <meta name="theme-color" content="#0c1211" />
</svelte:head>

<main class="dl">
  <div class="card">
    <a class="back" href="/?app=1">← Open Phonia in the browser</a>

    <h1>Phonia for desktop</h1>
    <p class="lede">
      The full workstation as a native app — offline, faster analysis, and native file access.
      Free and open source.{#if version}
        <span class="ver">Latest: v{version}</span>{/if}
    </p>

    <div class="grid">
      {#each ordered as p (p.id)}
        {@const build = builds[p.id]}
        <div class="plat" class:you={detected === p.id}>
          <div class="head">
            <span class="name">{p.name}</span>
            {#if detected === p.id}<span class="badge">Detected</span>{/if}
          </div>
          <p class="note">{p.note}</p>
          {#if status === 'loading'}
            <span class="btn ghost" aria-disabled="true">Checking latest release…</span>
          {:else if build.href}
            <a class="btn" href={build.href} data-testid={`download-${p.id}`}
              >Download for {p.name}</a
            >
            {#if build.altHref}
              <a class="alt" href={build.altHref}>or {build.altLabel}</a>
            {/if}
          {:else}
            <a class="btn ghost" href={RELEASES_URL} target="_blank" rel="noopener">
              See releases →
            </a>
          {/if}
        </div>
      {/each}
    </div>

    {#if status === 'pending'}
      <p class="pending">
        Desktop installers are published on
        <a href={RELEASES_URL} target="_blank" rel="noopener">GitHub Releases</a>. If a build isn't
        listed for your platform yet, it's on the way — the browser app works offline in the
        meantime.
      </p>
    {/if}

    <p class="foot">
      Prefer no install? <a href="/?app=1">Phonia runs entirely in your browser</a> and works
      offline once loaded.
    </p>
  </div>
</main>

<style>
  :root {
    --dl-bg: #0c1211;
    --dl-panel: #121a19;
    --dl-line: #223330;
    --dl-text: #e7efec;
    --dl-muted: #8ea69f;
    --dl-accent: #22c9a8;
    --dl-ink: #04231d;
  }

  .dl {
    min-height: 100dvh;
    margin: 0;
    display: grid;
    place-items: center;
    padding: 2rem 1.25rem;
    background:
      radial-gradient(1200px 600px at 50% -10%, #14201e 0%, transparent 60%), var(--dl-bg);
    color: var(--dl-text);
    font-family:
      ui-sans-serif, system-ui, -apple-system, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
  }

  .card {
    width: 100%;
    max-width: 46rem;
  }

  .back {
    display: inline-block;
    margin-bottom: 1.5rem;
    color: var(--dl-muted);
    text-decoration: none;
    font-size: 0.9rem;
  }
  .back:hover {
    color: var(--dl-text);
  }

  h1 {
    margin: 0 0 0.5rem;
    font-size: clamp(1.8rem, 4vw, 2.6rem);
    letter-spacing: -0.02em;
    text-wrap: balance;
  }

  .lede {
    margin: 0 0 2rem;
    max-width: 40rem;
    line-height: 1.6;
    color: var(--dl-muted);
  }
  .ver {
    display: inline-block;
    margin-left: 0.5rem;
    color: var(--dl-accent);
    font-variant-numeric: tabular-nums;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr));
    gap: 1rem;
  }

  .plat {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 1.1rem;
    border: 1px solid var(--dl-line);
    border-radius: 14px;
    background: var(--dl-panel);
  }
  .plat.you {
    border-color: color-mix(in oklab, var(--dl-accent) 55%, var(--dl-line));
    box-shadow: 0 0 0 1px color-mix(in oklab, var(--dl-accent) 30%, transparent);
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .name {
    font-weight: 650;
    font-size: 1.05rem;
  }
  .badge {
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--dl-ink);
    background: var(--dl-accent);
    border-radius: 999px;
    padding: 0.1rem 0.45rem;
  }
  .note {
    margin: 0;
    color: var(--dl-muted);
    font-size: 0.82rem;
    min-height: 2.2em;
  }

  .btn {
    display: inline-block;
    text-align: center;
    margin-top: auto;
    padding: 0.6rem 0.9rem;
    border-radius: 10px;
    background: var(--dl-accent);
    color: var(--dl-ink);
    font-weight: 650;
    text-decoration: none;
    transition: filter 0.12s ease;
  }
  .btn:hover {
    filter: brightness(1.08);
  }
  .btn.ghost {
    background: transparent;
    color: var(--dl-text);
    border: 1px solid var(--dl-line);
  }
  .btn.ghost[aria-disabled='true'] {
    color: var(--dl-muted);
    cursor: default;
  }

  .alt {
    text-align: center;
    font-size: 0.8rem;
    color: var(--dl-muted);
    text-decoration: none;
  }
  .alt:hover {
    color: var(--dl-text);
  }

  .pending,
  .foot {
    margin-top: 1.5rem;
    color: var(--dl-muted);
    font-size: 0.9rem;
    line-height: 1.6;
  }
  .pending a,
  .foot a {
    color: var(--dl-accent);
  }
</style>
