<script module lang="ts">
  // Colours picked anywhere, most-recent first, shared across every field so a
  // colour used on one plot is one click away on the next.
  const recents: string[] = [];

  export function addRecentColour(hex: string): void {
    const h = hex.toLowerCase();
    const at = recents.indexOf(h);
    if (at >= 0) recents.splice(at, 1);
    recents.unshift(h);
    if (recents.length > 8) recents.length = 8;
  }

  export function recentColours(): string[] {
    return [...recents];
  }
</script>

<script lang="ts">
  import { untrack } from 'svelte';

  interface Props {
    /** Current colour as `#rrggbb`. */
    value: string;
    /** Called with the new colour on every change. */
    onChange: (hex: string) => void;
    /** Called once when the popover opens, before any edit — for an undo snapshot. */
    onOpen?: () => void;
    title?: string;
    testId?: string;
  }

  let { value, onChange, onOpen, title, testId }: Props = $props();

  const PRESETS = [
    '#111111',
    '#555555',
    '#8ea099',
    '#0d9488',
    '#9cc4ff',
    '#ff5a52',
    '#ffcc33',
    '#c80000',
    '#2563eb',
    '#16a34a',
    '#7c3aed',
    '#ffffff'
  ];

  let open = $state(false);
  let root = $state<HTMLElement | null>(null);
  let recentList = $state<string[]>([]);
  let hexDraft = $state(untrack(() => value));

  function toggle() {
    if (!open) {
      onOpen?.();
      recentList = recentColours();
      hexDraft = value;
    }
    open = !open;
  }

  function pick(hex: string) {
    onChange(hex);
    addRecentColour(hex);
  }

  function onHexInput(raw: string) {
    hexDraft = raw;
    const m = /^#?([0-9a-f]{6})$/i.exec(raw.trim());
    if (m) onChange(`#${m[1]}`);
  }

  function onWindowPointerDown(event: PointerEvent) {
    if (!open) return;
    if (root && event.target instanceof Node && !root.contains(event.target)) {
      open = false;
      addRecentColour(value);
    }
  }
</script>

<svelte:window onpointerdown={onWindowPointerDown} />

<div class="colour-field" bind:this={root}>
  <button
    type="button"
    class="swatch-btn"
    data-testid={testId}
    aria-haspopup="dialog"
    aria-expanded={open}
    {title}
    style:background={value}
    onclick={toggle}
  ></button>

  {#if open}
    <div class="popover" role="dialog" aria-label="Colour picker">
      <div class="swatches">
        {#each PRESETS as c (c)}
          <button
            type="button"
            class="chip"
            class:on={value.toLowerCase() === c}
            style:background={c}
            title={c}
            onclick={() => pick(c)}
          ></button>
        {/each}
      </div>

      {#if recentList.length > 0}
        <div class="recent-row">
          <span class="lbl">Recent</span>
          <div class="swatches recent">
            {#each recentList as c (c)}
              <button
                type="button"
                class="chip"
                style:background={c}
                title={c}
                onclick={() => pick(c)}
              ></button>
            {/each}
          </div>
        </div>
      {/if}

      <div class="custom">
        <input
          type="color"
          aria-label="Pick a custom colour"
          value={/^#[0-9a-f]{6}$/i.test(value) ? value : '#000000'}
          oninput={(e) => pick(e.currentTarget.value)}
        />
        <input
          type="text"
          class="hex"
          aria-label="Hex colour"
          spellcheck="false"
          value={hexDraft}
          oninput={(e) => onHexInput(e.currentTarget.value)}
        />
      </div>
    </div>
  {/if}
</div>

<style>
  .colour-field {
    position: relative;
    display: inline-flex;
  }

  .swatch-btn {
    width: 2.2rem;
    height: 1.8rem;
    padding: 0;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    cursor: pointer;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.15);
  }

  .swatch-btn:hover {
    border-color: color-mix(in oklab, var(--accent) 45%, var(--chrome-strong));
  }

  .popover {
    position: absolute;
    top: calc(100% + 0.35rem);
    left: 0;
    z-index: 40;
    width: 12.5rem;
    padding: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-md, 8px);
    background: var(--panel);
    box-shadow: var(--shadow-lg, 0 8px 30px rgba(0, 0, 0, 0.4));
  }

  .swatches {
    display: grid;
    grid-template-columns: repeat(6, 1fr);
    gap: 0.25rem;
  }

  .chip {
    height: 1.15rem;
    border: 1px solid var(--chrome-strong);
    border-radius: 3px;
    padding: 0;
    cursor: pointer;
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.12);
  }

  .chip:hover {
    border-color: var(--accent);
  }

  .chip.on {
    border-color: var(--accent);
    box-shadow: 0 0 0 1px var(--accent);
  }

  .recent-row {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .lbl {
    font-size: 0.62rem;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .swatches.recent {
    grid-template-columns: repeat(8, 1fr);
  }

  .custom {
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .custom input[type='color'] {
    flex: none;
    width: 2rem;
    height: 1.7rem;
    padding: 1px;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    cursor: pointer;
  }

  .hex {
    flex: 1;
    min-width: 0;
    min-height: 1.7rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    font: inherit;
    font-size: 0.78rem;
    font-variant-numeric: tabular-nums;
    padding: 0 0.4rem;
  }
</style>
