<script lang="ts">
  import IconSearch from '~icons/lucide/search';
  import IconChevronLeft from '~icons/lucide/chevron-left';
  import IconChevronRight from '~icons/lucide/chevron-right';

  interface Props {
    query: string;
    count: number;
    index: number;
    replacement: string;
    onQuery: (text: string) => void;
    onNext: () => void;
    onPrev: () => void;
    onReplacement: (text: string) => void;
    onReplace: () => void;
    onReplaceAll: () => void;
  }

  let {
    query,
    count,
    index,
    replacement,
    onQuery,
    onNext,
    onPrev,
    onReplacement,
    onReplace,
    onReplaceAll
  }: Props = $props();

  function handleKeydown(event: KeyboardEvent) {
    event.stopPropagation();
    if (event.key === 'Enter') {
      event.preventDefault();
      if (event.shiftKey) onPrev();
      else onNext();
    }
  }
</script>

<div class="search" data-testid="label-search">
  <div class="search-field">
    <IconSearch class="search-icon" aria-hidden="true" />
    <input
      class="search-input"
      data-testid="search-input"
      type="search"
      placeholder="Search labels"
      autocomplete="off"
      autocapitalize="off"
      autocorrect="off"
      spellcheck="false"
      value={query}
      oninput={(event) => onQuery(event.currentTarget.value)}
      onkeydown={handleKeydown}
    />
  </div>
  <span class="count" data-testid="search-count">{count === 0 ? '0' : `${index + 1}/${count}`}</span>
  <button type="button" aria-label="Previous match" disabled={count === 0} onclick={onPrev}>
    <IconChevronLeft aria-hidden="true" />
  </button>
  <button type="button" aria-label="Next match" disabled={count === 0} onclick={onNext}>
    <IconChevronRight aria-hidden="true" />
  </button>
  <div class="search-field replace-field">
    <input
      class="search-input"
      data-testid="replace-input"
      type="text"
      placeholder="Replace with"
      autocomplete="off"
      autocapitalize="off"
      autocorrect="off"
      spellcheck="false"
      value={replacement}
      oninput={(event) => onReplacement(event.currentTarget.value)}
      onkeydown={(event) => event.stopPropagation()}
    />
  </div>
  <button type="button" class="text-btn" data-testid="replace-one" disabled={count === 0} onclick={onReplace}
    >Replace</button
  >
  <button
    type="button"
    class="text-btn"
    data-testid="replace-all"
    disabled={count === 0}
    onclick={onReplaceAll}>Replace all</button
  >
  {#if query.trim() && count === 0}
    <span class="no-hits" data-testid="search-empty">No labels match.</span>
  {/if}
</div>

<style>
  .search {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.3rem;
    /* Claim a full row in the annotation toolbar so the find and replace
       controls have room instead of being squeezed off the right edge. */
    flex: 1 1 100%;
  }

  .search-field {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    padding: 0 0.45rem;
    transition:
      border-color var(--t-fast),
      box-shadow var(--t-fast);
  }

  .search-field:focus-within {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 18%, transparent);
  }

  .search-field :global(.search-icon) {
    flex: none;
    font-size: 0.85rem;
    color: var(--muted);
  }

  .search-input {
    min-width: 8.5rem;
    border: none;
    background: transparent;
    color: var(--text);
    padding: 0.2rem 0;
    font-size: 0.8rem;
    outline: none;
  }

  .count {
    min-width: 2.6rem;
    text-align: center;
    font-size: 0.76rem;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .replace-field .search-input {
    min-width: 7rem;
  }

  .text-btn {
    flex: none;
    width: auto;
    height: auto;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    padding: 0.2rem 0.5rem;
    font-size: 0.76rem;
    cursor: pointer;
    white-space: nowrap;
  }

  .text-btn:hover:not(:disabled) {
    border-color: var(--accent);
  }

  .text-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .no-hits {
    font-size: 0.76rem;
    color: var(--muted);
  }

  button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    width: 1.7rem;
    height: 1.7rem;
    line-height: 1;
    transition:
      background var(--t-fast),
      border-color var(--t-fast);
  }

  button :global(svg) {
    font-size: 0.9rem;
  }

  button:hover:not(:disabled) {
    background: var(--panel);
    border-color: color-mix(in oklab, var(--accent) 32%, var(--chrome-strong));
  }

  button:disabled {
    opacity: 0.4;
  }
</style>
