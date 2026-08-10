/// <reference types="@sveltejs/kit" />
/// <reference lib="webworker" />

// Makes Phonia a true offline app: the shell, code, and WASM engine are cached
// so it keeps working with the network off, and it can be installed from the
// browser with no download or installer. `build` is the hashed app output,
// `files` the static assets, `version` a per-deploy cache key.
import { build, files, version } from '$service-worker';

const sw = self as unknown as ServiceWorkerGlobalScope;

const CACHE = `phonia-${version}`;
const PRECACHE = [...build, ...files];
const STATIC = new Set(PRECACHE);
// App shells to fall back to for offline navigations. `/?app=1` is the installed
// app's entry (see the manifest start_url); `/` covers a plain visit.
const SHELLS = ['/?app=1', '/'];

sw.addEventListener('install', (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(CACHE);
      await cache.addAll(PRECACHE);
      // The shells are best-effort — a transient failure must not abort install.
      await Promise.allSettled(SHELLS.map((s) => cache.add(s)));
      await sw.skipWaiting();
    })()
  );
});

sw.addEventListener('activate', (event) => {
  event.waitUntil(
    (async () => {
      for (const key of await caches.keys()) {
        if (key !== CACHE) await caches.delete(key);
      }
      await sw.clients.claim();
    })()
  );
});

sw.addEventListener('fetch', (event) => {
  const { request } = event;
  if (request.method !== 'GET') return;

  const url = new URL(request.url);
  // Leave cross-origin requests alone — the download page's GitHub API calls,
  // fonts from elsewhere, and so on must reach the network untouched.
  if (url.origin !== sw.location.origin) return;

  event.respondWith(respond(request, url));
});

async function respond(request: Request, url: URL): Promise<Response> {
  const cache = await caches.open(CACHE);

  // Hashed build assets never change under a URL: serve them from cache first.
  if (STATIC.has(url.pathname)) {
    const hit = await cache.match(url.pathname);
    if (hit) return hit;
  }

  try {
    const response = await fetch(request);
    // Cache same-origin successes so a later offline visit has them.
    if (response.ok && response.type === 'basic') {
      cache.put(request, response.clone());
    }
    return response;
  } catch {
    const hit = await cache.match(request);
    if (hit) return hit;
    // For a navigation with nothing cached, hand back the app shell so client
    // routing can take over offline.
    if (request.mode === 'navigate') {
      for (const shell of SHELLS) {
        const cached = await cache.match(shell);
        if (cached) return cached;
      }
    }
    throw new Error('offline and not in cache');
  }
}

export {};
