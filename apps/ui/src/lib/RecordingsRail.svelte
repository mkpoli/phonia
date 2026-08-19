<script lang="ts">
  import IconCheck from '~icons/lucide/check';
  import IconFolderOpen from '~icons/lucide/folder-open';
  import IconListMusic from '~icons/lucide/list-music';
  import IconPanelLeftClose from '~icons/lucide/panel-left-close';
  import IconPanelLeftOpen from '~icons/lucide/panel-left-open';
  import IconSearch from '~icons/lucide/search';
  import IconTrash from '~icons/lucide/trash-2';
  import InlineRename from './InlineRename.svelte';
  import WaveThumb from './WaveThumb.svelte';
  import { formatSampleRate, formatTime, type AudioId, type CoreClientLike } from './types';

  interface RailRecording {
    mediaId: number;
    name: string;
    duration: number;
    sampleRate: number;
    audioId: AudioId | null;
  }

  interface Props {
    client: CoreClientLike | null;
    theme: 'light' | 'dark';
    recordings: RailRecording[];
    currentRecordingId: number | null;
    open: boolean;
    onToggle: () => void;
    onSwitch: (mediaId: number) => void;
    /** Opens the host's audio file picker; absent hides the Import button. */
    onImport?: () => void;
    /** Renames a recording in place; absent leaves the rail names read-only. */
    onRename?: (mediaId: number, name: string) => void;
    /** Removes a recording from the project; absent hides the row delete
     *  affordances. */
    onDelete?: (mediaId: number) => void;
  }

  let {
    client,
    theme,
    recordings,
    currentRecordingId,
    open,
    onToggle,
    onSwitch,
    onImport,
    onRename,
    onDelete,
  }: Props = $props();

  function handleOpenKeydown(event: KeyboardEvent, mediaId: number) {
    if (event.target !== event.currentTarget) return;
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onSwitch(mediaId);
    } else if (onDelete && (event.key === 'Delete' || event.key === 'Backspace')) {
      event.preventDefault();
      onDelete(mediaId);
    }
  }

  let query = $state('');

  const visible = $derived.by(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return recordings;
    return recordings.filter((r) => r.name.toLowerCase().includes(needle));
  });

  // Comparison marking is local UI state: the engine has no cross-file
  // analysis path yet (selectionReadout and voiceReport both take a single
  // audio id), so marking two or more recordings here records intent without
  // claiming an overlay or diff that does not exist.
  let compareSet = $state(new Set<number>());

  $effect(() => {
    const live = new Set(recordings.map((r) => r.mediaId));
    let changed = false;
    const next = new Set<number>();
    for (const id of compareSet) {
      if (live.has(id)) next.add(id);
      else changed = true;
    }
    if (changed) compareSet = next;
  });

  function toggleCompare(mediaId: number, event: MouseEvent) {
    event.stopPropagation();
    const next = new Set(compareSet);
    if (next.has(mediaId)) next.delete(mediaId);
    else next.add(mediaId);
    compareSet = next;
  }
</script>

<aside class="rail" class:collapsed={!open} data-testid="recordings-rail" data-collapsed={!open}>
  {#if open}
    <header class="head">
      <IconListMusic aria-hidden="true" />
      <h2>Recordings</h2>
      <span class="count">{recordings.length}</span>
      <button
        type="button"
        class="toggle"
        data-testid="recordings-rail-toggle"
        aria-label="Collapse recordings rail"
        title="Collapse"
        onclick={onToggle}
      >
        <IconPanelLeftClose aria-hidden="true" />
      </button>
    </header>

    <label class="filter">
      <IconSearch aria-hidden="true" />
      <input
        type="search"
        data-testid="recordings-rail-filter"
        placeholder="Filter recordings…"
        aria-label="Filter recordings by name"
        bind:value={query}
      />
    </label>

    <ul class="list">
      {#if visible.length === 0}
        <li class="no-match" data-testid="recordings-rail-no-match">No recording matches.</li>
      {/if}
      {#each visible as rec (rec.mediaId)}
        {@const active = rec.mediaId === currentRecordingId}
        {@const marked = compareSet.has(rec.mediaId)}
        <li>
          <div class="row" class:active data-testid="recordings-rail-row" data-media-id={rec.mediaId} data-active={active}>
            <button
              type="button"
              class="mark"
              class:marked
              data-testid="recordings-rail-compare"
              data-marked={marked}
              aria-pressed={marked}
              aria-label={marked ? `Unmark ${rec.name} for comparison` : `Mark ${rec.name} for comparison`}
              title={marked ? 'Marked for comparison' : 'Mark for comparison'}
              onclick={(event) => toggleCompare(rec.mediaId, event)}
            >
              {#if marked}<IconCheck aria-hidden="true" />{/if}
            </button>
            <div
              class="open-row"
              role="button"
              tabindex="0"
              data-testid="recordings-rail-open"
              onclick={() => onSwitch(rec.mediaId)}
              onkeydown={(event) => handleOpenKeydown(event, rec.mediaId)}
            >
              <span class="thumb">
                <WaveThumb {client} audioId={rec.audioId} duration={rec.duration} {theme} width={68} height={18} />
              </span>
              <span class="meta">
                {#if onRename}
                  <InlineRename
                    name={rec.name}
                    class="rail-name"
                    label="Rename recording"
                    testId="rename-rail-recording"
                    onRename={(next) => onRename?.(rec.mediaId, next)}
                  />
                {:else}
                  <span class="name">{rec.name}</span>
                {/if}
                <span class="sub">{formatTime(rec.duration)} · {formatSampleRate(rec.sampleRate)}</span>
              </span>
            </div>
            {#if onDelete}
              <button
                type="button"
                class="del"
                data-testid="recordings-rail-delete"
                aria-label="Remove recording"
                title="Remove recording"
                onclick={(event) => {
                  event.stopPropagation();
                  onDelete(rec.mediaId);
                }}
              >
                <IconTrash aria-hidden="true" />
              </button>
            {/if}
          </div>
        </li>
      {/each}
    </ul>

    {#if compareSet.size > 0}
      <p class="compare-note" data-testid="recordings-rail-compare-note">
        {compareSet.size} marked for comparison. Overlay comparison across recordings is not
        implemented yet.
      </p>
    {/if}

    {#if onImport}
      <footer class="foot">
        <button type="button" class="import" data-testid="recordings-rail-import" onclick={onImport}>
          <IconFolderOpen aria-hidden="true" />
          <span>Import audio</span>
        </button>
      </footer>
    {/if}
  {:else}
    <button
      type="button"
      class="expand"
      data-testid="recordings-rail-toggle"
      aria-label="Show recordings"
      title="Recordings"
      onclick={onToggle}
    >
      <IconPanelLeftOpen aria-hidden="true" />
      <span class="count-chip">{recordings.length}</span>
    </button>
  {/if}
</aside>

<style>
  .rail {
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
    border-right: 1px solid var(--chrome-strong);
    background: var(--panel);
    color: var(--text);
  }

  .rail.collapsed {
    width: 2.25rem;
    min-width: 2.25rem;
    align-items: center;
  }

  .rail:not(.collapsed) {
    width: 13.5rem;
    min-width: 13.5rem;
  }

  .head {
    flex: none;
    display: flex;
    align-items: center;
    gap: 0.4rem;
    min-height: 2.1rem;
    padding: 0.4rem 0.5rem;
    border-bottom: 1px solid var(--chrome-strong);
    background: var(--panel-soft);
  }

  .head :global(svg) {
    flex: none;
    font-size: 0.9rem;
    color: var(--muted);
  }

  .head h2 {
    margin: 0;
    flex: 1;
    min-width: 0;
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .count {
    flex: none;
    color: var(--muted);
    font-size: 0.7rem;
    font-variant-numeric: tabular-nums;
  }

  .toggle {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 1.5rem;
    height: 1.5rem;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--muted);
    transition:
      background var(--t-fast),
      color var(--t-fast);
  }

  .toggle:hover {
    background: var(--panel-soft);
    color: var(--text);
  }

  .expand {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.3rem;
    width: 100%;
    margin-top: 0.5rem;
    padding: 0.4rem 0;
    border: none;
    background: transparent;
    color: var(--muted);
    transition: color var(--t-fast);
  }

  .expand:hover {
    color: var(--text);
  }

  .expand :global(svg) {
    font-size: 1rem;
  }

  .count-chip {
    font-size: 0.62rem;
    font-variant-numeric: tabular-nums;
  }

  .filter {
    flex: none;
    display: flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.3rem 0.5rem;
    border-bottom: 1px solid var(--chrome-strong);
    color: var(--muted);
  }

  .filter :global(svg) {
    flex: none;
    font-size: 0.8rem;
  }

  .filter input {
    flex: 1;
    min-width: 0;
    border: none;
    background: transparent;
    color: var(--text);
    font-size: 0.74rem;
    padding: 0.15rem 0;
    outline: none;
  }

  .filter input::placeholder {
    color: var(--muted);
  }

  .list {
    flex: 1 1 auto;
    min-height: 0;
    margin: 0;
    padding: 0.3rem;
    list-style: none;
    overflow-y: auto;
  }

  .no-match {
    padding: 0.5rem 0.4rem;
    color: var(--muted);
    font-size: 0.72rem;
  }

  .foot {
    flex: none;
    padding: 0.4rem 0.5rem;
    border-top: 1px solid var(--chrome-strong);
    background: var(--panel-soft);
  }

  .import {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.4rem;
    width: 100%;
    min-height: 1.8rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel);
    color: var(--text);
    font-size: 0.74rem;
    font-weight: 500;
    transition:
      background var(--t-fast),
      border-color var(--t-fast);
  }

  .import:hover {
    background: var(--panel-soft);
    border-color: color-mix(in oklab, var(--accent) 32%, var(--chrome-strong));
  }

  .import :global(svg) {
    font-size: 0.85rem;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 0.3rem;
    border-radius: var(--radius-sm);
    margin-bottom: 0.15rem;
  }

  .row:hover {
    background: var(--panel-soft);
  }

  .row.active {
    background: var(--accent-tint);
    box-shadow: inset 2px 0 0 var(--accent);
  }

  .mark {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 1rem;
    height: 1rem;
    margin-left: 0.3rem;
    border: 1px solid var(--chrome-strong);
    border-radius: 2px;
    background: transparent;
    color: transparent;
  }

  .mark.marked {
    border-color: var(--accent);
    background: var(--accent-tint);
    color: var(--accent-strong);
  }

  .mark :global(svg) {
    font-size: 0.65rem;
    stroke-width: 3;
  }

  .open-row {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    min-width: 0;
    flex: 1;
    padding: 0.35rem 0.4rem 0.35rem 0.15rem;
    border: none;
    background: transparent;
    color: inherit;
    text-align: left;
    cursor: pointer;
    border-radius: var(--radius-sm);
  }

  .open-row:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
  }

  .del {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 1.25rem;
    height: 1.25rem;
    margin-right: 0.3rem;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--muted);
    opacity: 0;
    transition:
      opacity var(--t-fast),
      color var(--t-fast),
      background var(--t-fast),
      border-color var(--t-fast);
  }

  .row:hover .del,
  .row:focus-within .del {
    opacity: 1;
  }

  .del:hover {
    color: var(--danger);
    background: var(--panel-soft);
    border-color: color-mix(in oklab, var(--danger) 45%, transparent);
  }

  .del :global(svg) {
    font-size: 0.8rem;
  }

  .thumb {
    flex: none;
    line-height: 0;
  }

  .thumb :global(.thumb) {
    border: 1px solid var(--chrome-strong);
  }

  .meta {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 0.05rem;
  }

  .meta :global(.inline-rename) {
    width: 100%;
  }

  .name,
  .meta :global(.rail-name) {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.76rem;
    font-weight: 500;
  }

  .row.active .name,
  .row.active .meta :global(.rail-name) {
    color: var(--accent-strong);
    font-weight: 600;
  }

  .sub {
    color: var(--muted);
    font-size: 0.66rem;
    font-variant-numeric: tabular-nums;
  }

  .compare-note {
    flex: none;
    margin: 0;
    padding: 0.5rem 0.6rem;
    border-top: 1px solid var(--chrome-strong);
    background: var(--panel-soft);
    color: var(--muted);
    font-size: 0.66rem;
    line-height: 1.4;
  }

  @media (max-width: 720px) {
    .rail {
      display: none;
    }
  }
</style>
