<script lang="ts">
  import { onMount } from 'svelte';
  import IconDownload from '~icons/lucide/download';
  import IconShare from '~icons/lucide/share';
  import IconWifiOff from '~icons/lucide/wifi-off';
  import IconX from '~icons/lucide/x';

  interface InstallPromptEvent extends Event {
    prompt: () => Promise<void>;
    userChoice: Promise<{ outcome: 'accepted' | 'dismissed'; platform: string }>;
  }

  const DISMISSED_KEY = 'phonia:pwa-install-dismissed';

  let installPrompt = $state<InstallPromptEvent | null>(null);
  let installAvailable = $state(false);
  let iosInstructions = $state(false);
  let online = $state(true);
  let ready = $state(false);
  let dismissed = false;

  onMount(() => {
    ready = true;
    online = navigator.onLine;

    const standalone =
      window.matchMedia('(display-mode: standalone)').matches ||
      Boolean((navigator as Navigator & { standalone?: boolean }).standalone);
    const isiOS =
      /iPad|iPhone|iPod/.test(navigator.userAgent) ||
      (navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1);
    try {
      dismissed = localStorage.getItem(DISMISSED_KEY) === '1';
    } catch {
      dismissed = false;
    }

    if (isiOS && !standalone && !dismissed) installAvailable = true;

    const onInstallPrompt = (event: Event) => {
      event.preventDefault();
      installPrompt = event as InstallPromptEvent;
      if (!dismissed) installAvailable = true;
    };
    const onInstalled = () => {
      installPrompt = null;
      installAvailable = false;
      iosInstructions = false;
    };
    const onOnline = () => (online = true);
    const onOffline = () => (online = false);

    window.addEventListener('beforeinstallprompt', onInstallPrompt);
    window.addEventListener('appinstalled', onInstalled);
    window.addEventListener('online', onOnline);
    window.addEventListener('offline', onOffline);
    return () => {
      window.removeEventListener('beforeinstallprompt', onInstallPrompt);
      window.removeEventListener('appinstalled', onInstalled);
      window.removeEventListener('online', onOnline);
      window.removeEventListener('offline', onOffline);
    };
  });

  async function install() {
    if (!installPrompt) {
      iosInstructions = true;
      return;
    }
    const prompt = installPrompt;
    await prompt.prompt();
    const choice = await prompt.userChoice;
    installPrompt = null;
    if (choice.outcome === 'accepted') installAvailable = false;
    else dismiss();
  }

  function dismiss() {
    dismissed = true;
    installAvailable = false;
    try {
      localStorage.setItem(DISMISSED_KEY, '1');
    } catch {
      // The dismissal lasts for this page when browser storage is unavailable.
    }
  }
</script>

{#if ready}<span data-testid="pwa-status-ready" hidden></span>{/if}

{#if !online}
  <div class="offline" role="status" data-testid="offline-status">
    <IconWifiOff aria-hidden="true" />
    <span>Working offline</span>
  </div>
{/if}

{#if installAvailable}
  <aside class="install" aria-label="Install Phonia" data-testid="pwa-install-prompt">
    <div class="message">
      <strong>Install Phonia</strong>
      <span>Open Phonia from your home screen. The app remains available offline.</span>
    </div>
    <button type="button" class="install-button" onclick={install}>
      <IconDownload aria-hidden="true" />
      <span>Install</span>
    </button>
    <button type="button" class="dismiss" aria-label="Dismiss install prompt" onclick={dismiss}>
      <IconX aria-hidden="true" />
    </button>
  </aside>
{/if}

{#if iosInstructions}
  <div class="backdrop">
    <div
      class="instructions"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-labelledby="install-title"
    >
      <IconShare class="share" aria-hidden="true" />
      <h2 id="install-title">Add Phonia to your Home Screen</h2>
      <p>Open Safari’s Share menu, then choose <strong>Add to Home Screen</strong>.</p>
      <button type="button" onclick={() => (iosInstructions = false)}>Close</button>
    </div>
  </div>
{/if}

<style>
  .offline,
  .install {
    position: fixed;
    z-index: 70;
    border: 1px solid var(--chrome-strong);
    background: var(--panel);
    color: var(--text);
    box-shadow: var(--shadow-lg);
  }

  .offline {
    top: calc(0.75rem + var(--safe-top));
    left: 50%;
    transform: translateX(-50%);
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.45rem 0.7rem;
    border-radius: 999px;
    font-size: 0.78rem;
    white-space: nowrap;
  }

  .install {
    right: calc(1rem + var(--safe-right));
    bottom: calc(1rem + var(--safe-bottom));
    display: flex;
    align-items: center;
    gap: 0.75rem;
    width: min(32rem, calc(100vw - 2rem - var(--safe-left) - var(--safe-right)));
    padding: 0.7rem;
    border-radius: var(--radius-xl);
  }

  .message {
    min-width: 0;
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 0.15rem;
    font-size: 0.78rem;
    line-height: 1.35;
    color: var(--muted);
  }

  .message strong {
    color: var(--text);
    font-size: 0.86rem;
  }

  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    min-height: 2.25rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-md);
    background: var(--panel-soft);
    color: var(--text);
  }

  .install-button {
    padding: 0.35rem 0.7rem;
    border-color: var(--accent);
    background: var(--accent);
    color: var(--on-accent);
    font-size: 0.82rem;
    font-weight: 600;
  }

  .dismiss {
    width: 2.25rem;
    border-color: transparent;
    background: transparent;
    color: var(--muted);
  }

  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: grid;
    place-items: center;
    padding: calc(0.5rem + var(--safe-top)) calc(0.5rem + var(--safe-right))
      calc(0.5rem + var(--safe-bottom)) calc(0.5rem + var(--safe-left));
    background: color-mix(in oklab, #000 55%, transparent);
  }

  .instructions {
    width: min(24rem, 100%);
    padding: 1.4rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-xl);
    background: var(--panel);
    color: var(--text);
    text-align: center;
    box-shadow: var(--shadow-lg);
  }

  .instructions :global(.share) {
    font-size: 1.6rem;
    color: var(--accent);
  }

  .instructions h2 {
    margin: 0.6rem 0 0.45rem;
    font-size: 1.05rem;
  }

  .instructions p {
    margin: 0 0 1rem;
    color: var(--muted);
    line-height: 1.5;
  }

  .instructions button {
    width: 100%;
  }

  @media (max-width: 720px) {
    .install {
      right: calc(0.5rem + var(--safe-right));
      bottom: calc(var(--mobile-rail-height) + var(--safe-bottom) + 0.5rem);
      width: calc(100vw - 1rem - var(--safe-left) - var(--safe-right));
    }

    .message span {
      display: none;
    }

    .dismiss {
      width: 2.75rem;
      min-width: 2.75rem;
      min-height: 2.75rem;
    }

    .offline {
      top: calc(0.5rem + var(--safe-top));
    }
  }
</style>
