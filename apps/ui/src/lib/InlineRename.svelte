<script lang="ts">
  import IconPencil from '~icons/lucide/pencil';

  interface Props {
    name: string;
    onRename: (next: string) => void;
    /**
     * Given when the name doubles as the control that opens or selects its
     * object. A plain click, Enter or Space then acts on the object and the
     * rename gestures still reach the field underneath.
     */
    onActivate?: () => void;
    /** Rendered class for the display span, so callers can match surrounding typography. */
    class?: string;
    /** aria-label for the edit affordance, e.g. "Rename project" / "Rename recording". */
    label: string;
    /**
     * Whether to render the pencil. Rows that already carry their own edit
     * control pass false, so the name keeps its gestures without a second
     * pencil beside the first.
     */
    pencil?: boolean;
    testId?: string;
  }

  let {
    name,
    onRename,
    onActivate,
    class: className = '',
    label,
    pencil = true,
    testId = 'inline-rename',
  }: Props = $props();

  const LONG_PRESS_MS = 500;
  /** Movement past this many pixels reads as a scroll or drag, never as a press. */
  const LONG_PRESS_SLOP = 10;

  let editing = $state(false);
  let draft = $state('');
  let inputEl = $state<HTMLInputElement | null>(null);

  let pressTimer: ReturnType<typeof setTimeout> | null = null;
  let pressOrigin: { x: number; y: number } | null = null;
  /** A finished long press must not also activate through the click the platform sends after it. */
  let swallowClick = false;

  function startEdit() {
    draft = name;
    editing = true;
  }

  function cancelPress() {
    if (pressTimer !== null) {
      clearTimeout(pressTimer);
      pressTimer = null;
    }
    pressOrigin = null;
  }

  function handlePointerDown(event: PointerEvent) {
    // A stale swallow flag would eat an unrelated click, so every press clears it.
    swallowClick = false;
    if (editing || event.pointerType === 'mouse') return;
    pressOrigin = { x: event.clientX, y: event.clientY };
    pressTimer = setTimeout(() => {
      pressTimer = null;
      swallowClick = true;
      startEdit();
    }, LONG_PRESS_MS);
  }

  function handlePointerMove(event: PointerEvent) {
    if (pressTimer === null || !pressOrigin) return;
    const dx = event.clientX - pressOrigin.x;
    const dy = event.clientY - pressOrigin.y;
    if (Math.hypot(dx, dy) > LONG_PRESS_SLOP) cancelPress();
  }

  function handleClick(event: MouseEvent) {
    if (swallowClick) {
      swallowClick = false;
      event.preventDefault();
      event.stopPropagation();
      return;
    }
    onActivate?.();
  }

  /**
   * Pointer work inside the field belongs to the field. Rows, crumbs and chips
   * that open or navigate on click surround it, and placing a caret must not
   * reach them.
   */
  function stopEvent(event: Event) {
    event.stopPropagation();
  }

  function handleContextMenu(event: MouseEvent) {
    event.preventDefault();
    event.stopPropagation();
    cancelPress();
    startEdit();
  }

  /** Enters edit mode from an owning widget (a treegrid row's F2, say). */
  export function edit() {
    startEdit();
  }

  function commit() {
    if (!editing) return;
    const next = draft.trim();
    editing = false;
    if (next && next !== name) onRename(next);
  }

  function cancel() {
    editing = false;
  }

  function handleDisplayKeydown(event: KeyboardEvent) {
    if (editing || (event.key !== 'F2' && event.key !== 'Enter' && event.key !== ' ')) return;
    // Owning widgets (a treegrid row) bind these same keys; the rename
    // affordance claims them while focused so a row does not also act on them.
    event.preventDefault();
    event.stopPropagation();
    if (event.key !== 'F2' && onActivate) onActivate();
    else startEdit();
  }

  function handleInputKeydown(event: KeyboardEvent) {
    // Keystrokes while editing belong to the field, never to an owning row.
    event.stopPropagation();
    if (event.key === 'Enter') {
      event.preventDefault();
      commit();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      cancel();
    }
  }

  $effect(() => {
    if (editing && inputEl) {
      inputEl.focus();
      inputEl.select();
    }
  });

  $effect(() => cancelPress);
</script>

<span class="inline-rename" data-testid={testId}>
  {#if editing}
    <input
      bind:this={inputEl}
      bind:value={draft}
      class={className}
      type="text"
      size={Math.max(draft.length + 1, 4)}
      aria-label={label}
      data-testid="{testId}-input"
      onkeydown={handleInputKeydown}
      onblur={commit}
      onclick={stopEvent}
      ondblclick={stopEvent}
      onpointerdown={stopEvent}
    />
  {:else}
    <span
      class="display {className}"
      data-testid="{testId}-name"
      role="button"
      tabindex="0"
      aria-label={onActivate ? name : `${label}: ${name}`}
      ondblclick={startEdit}
      oncontextmenu={handleContextMenu}
      onpointerdown={handlePointerDown}
      onpointermove={handlePointerMove}
      onpointerup={cancelPress}
      onpointercancel={cancelPress}
      onpointerleave={cancelPress}
      onclick={handleClick}
      onkeydown={handleDisplayKeydown}
    >
      {name}
    </span>
    {#if pencil}
      <button
        type="button"
        class="edit"
        aria-label={label}
        title={label}
        data-testid="{testId}-edit"
        onclick={(event) => {
          // The pencil sits inside rows and crumbs that open or navigate on
          // click; reaching for it must not also trigger them.
          event.stopPropagation();
          startEdit();
        }}
      >
        <IconPencil aria-hidden="true" />
      </button>
    {/if}
  {/if}
</span>

<style>
  .inline-rename {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    max-width: 100%;
  }

  .display {
    min-width: 0;
    border-radius: var(--radius-sm);
    cursor: pointer;
    /* A long press must reach the rename gesture rather than raise the
       platform's own text selection handles or callout menu. */
    -webkit-touch-callout: none;
    user-select: none;
    touch-action: manipulation;
  }

  .display:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .edit {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    padding: 0.15rem;
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

  .edit :global(svg) {
    font-size: 0.85em;
  }

  .inline-rename:hover .edit,
  .inline-rename:focus-within .edit {
    opacity: 1;
  }

  .edit:hover {
    color: var(--accent-strong);
    background: var(--accent-tint);
  }

  .edit:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
    opacity: 1;
  }

  input {
    font: inherit;
    color: var(--text);
    background: var(--panel);
    border: 1px solid var(--accent);
    border-radius: var(--radius-sm);
    padding: 0.05rem 0.35rem;
    min-width: 0;
    /* `size` sets the natural width; a narrow rail or column caps it here so
       the field never overflows the row it sits in. */
    max-width: 100%;
  }

  input:focus {
    outline: none;
    box-shadow: 0 0 0 3px color-mix(in oklab, var(--accent) 22%, transparent);
  }
</style>
