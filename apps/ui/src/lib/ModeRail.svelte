<script lang="ts">
  import IconLibrary from '~icons/lucide/library';
  import IconActivity from '~icons/lucide/activity';
  import IconChartSpline from '~icons/lucide/chart-spline';

  type ModeId = 'library' | 'analyze' | 'plots';

  interface Props {
    /** Which mode is current; drives aria-current and the active visual state. */
    active: ModeId;
    /** False disables the Analyse button (no recording is open to analyse). */
    analyzeEnabled: boolean;
    /** False disables the Plots button (no recording is open to plot). */
    plotsEnabled: boolean;
    onNavigate: (mode: ModeId) => void;
  }

  let { active, analyzeEnabled, plotsEnabled, onNavigate }: Props = $props();

  const MODES: { id: ModeId; label: string }[] = [
    { id: 'library', label: 'Library' },
    { id: 'analyze', label: 'Analyse' },
    { id: 'plots', label: 'Plots' }
    // Studio and Script are planned modes; add each as one more entry here
    // plus one icon import above.
  ];

  const ICONS = { library: IconLibrary, analyze: IconActivity, plots: IconChartSpline };

  // Remounting the mark restarts its draw-in; hovering the brand replays it
  // with the landing's shortened replay delays.
  let replays = $state(0);

  function disabledFor(id: ModeId): boolean {
    if (id === 'analyze') return !analyzeEnabled;
    if (id === 'plots') return !plotsEnabled;
    return false;
  }

  function titleFor(id: ModeId, label: string): string {
    if (id === 'analyze' && !analyzeEnabled) return 'Analyse — open a recording first';
    if (id === 'plots' && !plotsEnabled) return 'Plots — open a recording first';
    return label;
  }

  function navigate(id: ModeId) {
    onNavigate(id);
  }

  // Normalize every stroke's length to 1 so the activation redraw traces each
  // path uniformly, regardless of how long the lucide glyph's individual
  // subpaths actually are — the draw looks even across all icons.
  function normalizeStrokes(node: HTMLElement) {
    for (const shape of node.querySelectorAll(
      'path, line, polyline, polygon, rect, circle, ellipse'
    )) {
      shape.setAttribute('pathLength', '1');
    }
  }
</script>

<nav class="rail" aria-label="Modes">
  <a
    class="brand"
    href="/landing"
    aria-label="Phonia — about this app"
    onmouseenter={() => (replays += 1)}
  >
    {#key replays}
      <svg
        class="mark"
        class:replay={replays > 0}
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
    {/key}
  </a>

  <div class="modes">
    {#each MODES as mode (mode.id)}
      {@const Icon = ICONS[mode.id]}
      <button
        type="button"
        class="mode"
        class:active={active === mode.id}
        aria-current={active === mode.id ? 'page' : undefined}
        disabled={disabledFor(mode.id)}
        title={titleFor(mode.id, mode.label)}
        onclick={() => navigate(mode.id)}
        {@attach normalizeStrokes}
      >
        <Icon aria-hidden="true" />
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

  .m-dot {
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

  .mode :global(svg) {
    width: 1.15rem;
    height: 1.15rem;
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
    /* The brand draws itself in once on load — ring, then wave, then the gold
       dot pops and settles into a slow breathe. Hovering remounts the mark and
       replays the sequence with the landing's shorter replay delays. */
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

    .mark.replay .m-ring {
      animation-delay: 0s;
    }

    .mark.replay .m-wave {
      animation-delay: 0.3s;
    }

    .mark.replay .m-dot {
      animation-delay: 0.85s, 2.5s;
    }

    /* Becoming active redraws the glyph: every stroke re-traces itself from
       nothing (pathLength is normalized to 1, so the draw is even across the
       whole icon), while a warm accent glow blooms off the ink and fades. The
       shape never changes — only its ink is redrawn. */
    .mode.active :global(svg) {
      animation: bloom 0.7s ease-out;
    }

    .mode.active :global(svg path),
    .mode.active :global(svg line),
    .mode.active :global(svg polyline),
    .mode.active :global(svg polygon),
    .mode.active :global(svg rect),
    .mode.active :global(svg circle),
    .mode.active :global(svg ellipse) {
      stroke-dasharray: 1;
      stroke-dashoffset: 1;
      animation: redraw 0.6s cubic-bezier(0.65, 0, 0.35, 1) forwards;
    }
  }

  @keyframes redraw {
    to {
      stroke-dashoffset: 0;
    }
  }

  @keyframes bloom {
    0% {
      filter: drop-shadow(0 0 0 transparent);
    }
    40% {
      filter: drop-shadow(0 0 4px var(--accent));
    }
    100% {
      filter: drop-shadow(0 0 0 transparent);
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

  @keyframes draw {
    to {
      stroke-dashoffset: 0;
    }
  }
</style>
