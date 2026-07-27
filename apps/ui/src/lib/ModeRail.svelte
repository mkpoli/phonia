<script lang="ts">
  interface Props {
    /** Which mode is current; drives aria-current and the active visual state. */
    active: 'library' | 'analyze';
    /** False disables the Analyse button (no recording is open to analyse). */
    analyzeEnabled: boolean;
    onNavigate: (mode: 'library' | 'analyze') => void;
  }

  let { active, analyzeEnabled, onNavigate }: Props = $props();

  type ModeId = 'library' | 'analyze' | 'plots';

  const MODES: { id: ModeId; label: string }[] = [
    { id: 'library', label: 'Library' },
    { id: 'analyze', label: 'Analyse' },
    // Plots stays disabled until the figure view exists; its click is inert.
    { id: 'plots', label: 'Plots' }
    // Studio and Script are planned modes; add each as one more entry here
    // plus one icon block below.
  ];

  function disabledFor(id: ModeId): boolean {
    if (id === 'analyze') return !analyzeEnabled;
    if (id === 'plots') return true;
    return false;
  }

  function titleFor(id: ModeId, label: string): string {
    if (id === 'analyze' && !analyzeEnabled) return 'Analyse — open a recording first';
    if (id === 'plots') return 'Plots — not yet available';
    return label;
  }

  function navigate(id: ModeId) {
    if (id === 'plots') return;
    onNavigate(id);
  }
</script>

<nav class="rail" aria-label="Modes">
  <a class="brand" href="/landing" aria-label="Phonia — about this app">
    <svg
      class="mark"
      aria-hidden="true"
      viewBox="0 0 64 64"
      fill="none"
      stroke-width="6"
      stroke-linecap="round"
    >
      <path class="m-ring" pathLength="100" stroke="currentColor" d="M46.5 12.9 A 22 22 0 1 0 52.2 20.4" />
      <path class="m-wave" pathLength="100" stroke="currentColor" d="M14 36 C20 24 25 24 31 32 C37 40 41 40 50 24" />
      <circle class="m-dot" cx="52" cy="20" r="5" />
    </svg>
  </a>

  <div class="modes">
    {#each MODES as mode (mode.id)}
      <button
        type="button"
        class="mode"
        class:active={active === mode.id}
        aria-current={active === mode.id ? 'page' : undefined}
        disabled={disabledFor(mode.id)}
        title={titleFor(mode.id, mode.label)}
        onclick={() => navigate(mode.id)}
      >
        <!-- Icons share the brand mark's language: 64 grid, stroke 6, round
             caps, one signal motif per glyph, gold for the point of interest. -->
        {#if mode.id === 'library'}
          <svg aria-hidden="true" viewBox="0 0 64 64" fill="none" stroke-width="6" stroke-linecap="round">
            <rect class="i-frame" pathLength="100" stroke="currentColor" x="10" y="14" width="44" height="36" rx="7" />
            <path class="i-sig" pathLength="100" stroke="currentColor" d="M22 28 V36 M32 24 V40 M42 29 V35" />
          </svg>
        {:else if mode.id === 'analyze'}
          <svg aria-hidden="true" viewBox="0 0 64 64" fill="none" stroke-width="6" stroke-linecap="round">
            <path class="i-sig" pathLength="100" stroke="currentColor" d="M8 36 C16 20 22 20 30 32 C38 44 44 44 56 22" />
          </svg>
        {:else if mode.id === 'plots'}
          <svg aria-hidden="true" viewBox="0 0 64 64" fill="none" stroke-width="6" stroke-linecap="round">
            <path class="i-frame" pathLength="100" stroke="currentColor" d="M14 10 V50 H54" />
            <path class="i-sig" pathLength="100" stroke="currentColor" d="M22 42 C28 40 32 32 38 27 C42 23 46 20 49 18" />
            <circle class="i-dot" cx="50" cy="17" r="4.5" />
          </svg>
        {/if}
        <span>{mode.label}</span>
      </button>
    {/each}
  </div>
</nav>

<style>
  .rail {
    position: fixed;
    inset: 0 auto 0 0;
    /* Keep this width in sync with the host app-content offset. */
    width: 4.75rem;
    z-index: 10;
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border-right: 1px solid var(--chrome-strong);
  }

  .brand {
    height: 4rem;
    display: grid;
    place-items: center;
    color: inherit;
    text-decoration: none;
    border-radius: var(--radius-sm);
  }

  .mark {
    width: 1.6rem;
    height: 1.6rem;
    color: var(--accent);
  }

  .m-dot,
  .i-dot {
    fill: var(--warn);
  }

  .modes {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .mode {
    width: 100%;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.35rem;
    padding: 0.65rem 0.25rem;
    border: none;
    border-left: 2px solid transparent;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
    transition:
      background var(--t-fast),
      color var(--t-fast),
      border-color var(--t-fast);
  }

  .mode svg {
    width: 1.2rem;
    height: 1.2rem;
  }

  .mode span {
    font-size: 0.62rem;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .mode:hover:not(:disabled):not(.active) {
    color: var(--text);
    background: var(--panel-soft);
  }

  .mode.active {
    color: var(--accent);
    border-left-color: var(--accent);
    background: var(--accent-tint);
  }

  .mode:disabled {
    color: color-mix(in oklab, var(--muted), transparent 45%);
    cursor: default;
  }

  @media (prefers-reduced-motion: no-preference) {
    /* The brand draws itself in once on load — ring, then wave, then the
       gold dot pops and settles into a slow breathe. Hovering re-pops the dot. */
    .mark .m-ring {
      stroke-dasharray: 100;
      stroke-dashoffset: 100;
      animation: draw 1.15s cubic-bezier(0.55, 0.06, 0.25, 1) 0.1s forwards;
    }

    .mark .m-wave {
      stroke-dasharray: 100;
      stroke-dashoffset: 100;
      animation: draw 0.85s cubic-bezier(0.55, 0.06, 0.25, 1) 0.42s forwards;
    }

    .mark .m-dot {
      opacity: 0;
      transform-box: fill-box;
      transform-origin: center;
      transform: scale(0);
      animation:
        dotpop 0.5s cubic-bezier(0.34, 1.56, 0.64, 1) 1.05s forwards,
        dotbreathe 4.4s ease-in-out 3s infinite;
    }

    .brand:hover .m-dot {
      opacity: 1;
      transform: scale(1);
      animation: dotpop 0.5s cubic-bezier(0.34, 1.56, 0.64, 1);
    }

    /* Becoming active replays the same sequence in miniature: the glyph
       nudges, its frame draws, the signal traces through it, gold pops last. */
    .mode.active svg {
      animation: nudge 0.45s cubic-bezier(0.34, 1.56, 0.64, 1);
    }

    .mode.active .i-frame {
      stroke-dasharray: 100;
      stroke-dashoffset: 100;
      animation: draw 0.5s cubic-bezier(0.55, 0.06, 0.25, 1) forwards;
    }

    .mode.active .i-sig {
      stroke-dasharray: 100;
      stroke-dashoffset: 100;
      animation: draw 0.45s cubic-bezier(0.55, 0.06, 0.25, 1) 0.14s forwards;
    }

    .mode.active .i-dot {
      opacity: 0;
      transform-box: fill-box;
      transform-origin: center;
      transform: scale(0);
      animation: dotpop 0.45s cubic-bezier(0.34, 1.56, 0.64, 1) 0.42s forwards;
    }
  }

  @keyframes draw {
    to {
      stroke-dashoffset: 0;
    }
  }

  @keyframes dotpop {
    0% {
      opacity: 0;
      transform: scale(0);
    }
    60% {
      opacity: 1;
      transform: scale(1.4);
    }
    80% {
      transform: scale(0.88);
    }
    100% {
      opacity: 1;
      transform: scale(1);
    }
  }

  @keyframes dotbreathe {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.55;
    }
  }

  @keyframes nudge {
    35% {
      transform: scale(1.12);
    }
    70% {
      transform: scale(0.97);
    }
    100% {
      transform: scale(1);
    }
  }
</style>
