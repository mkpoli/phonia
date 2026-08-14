<script lang="ts">
  import IconX from '~icons/lucide/x';

  interface Props {
    onClose: () => void;
  }

  let { onClose }: Props = $props();

  type Tab = 'standard' | 'sino' | 'ext';
  let tab = $state<Tab>('standard');

  // A labelled run of glyphs. Combining marks are prefixed with U+25CC (dotted
  // circle) for display only, so a bare diacritic still shows on its own key.
  interface Group {
    label: string;
    glyphs: string[];
  }

  const STANDARD: Group[] = [
    {
      label: 'Pulmonic consonants',
      glyphs: [
        'p', 'b', 't', 'd', 'ʈ', 'ɖ', 'c', 'ɟ', 'k', 'ɡ', 'q', 'ɢ', 'ʔ',
        'm', 'ɱ', 'n', 'ɳ', 'ɲ', 'ŋ', 'ɴ',
        'ʙ', 'r', 'ʀ', 'ⱱ', 'ɾ', 'ɽ',
        'ɸ', 'β', 'f', 'v', 'θ', 'ð', 's', 'z', 'ʃ', 'ʒ', 'ʂ', 'ʐ', 'ç', 'ʝ',
        'x', 'ɣ', 'χ', 'ʁ', 'ħ', 'ʕ', 'h', 'ɦ',
        'ɬ', 'ɮ', 'ʋ', 'ɹ', 'ɻ', 'j', 'ɰ', 'l', 'ɭ', 'ʎ', 'ʟ'
      ]
    },
    {
      label: 'Non-pulmonic',
      glyphs: ['ʘ', 'ǀ', 'ǃ', 'ǂ', 'ǁ', 'ɓ', 'ɗ', 'ʄ', 'ɠ', 'ʛ', 'ʼ']
    },
    {
      label: 'Vowels',
      glyphs: [
        'i', 'y', 'ɨ', 'ʉ', 'ɯ', 'u',
        'ɪ', 'ʏ', 'ʊ',
        'e', 'ø', 'ɘ', 'ɵ', 'ɤ', 'o', 'ə',
        'ɛ', 'œ', 'ɜ', 'ɞ', 'ʌ', 'ɔ',
        'æ', 'ɐ', 'a', 'ɶ', 'ɑ', 'ɒ'
      ]
    },
    {
      label: 'Diacritics',
      glyphs: [
        '̥', '̬', 'ʰ', '̹', '̜', '̟', '̠', '̈', '̽', '̩', '̯', '˞',
        '̤', '̰', '̼', 'ʷ', 'ʲ', 'ˠ', 'ˤ', '̝', '̞', '̘', '̙',
        '̪', '̺', '̻', '̃', 'ⁿ', 'ˡ', '̚'
      ]
    },
    {
      label: 'Length & suprasegmentals',
      glyphs: ['ː', 'ˑ', '̆', 'ˈ', 'ˌ', '.', '|', '‖', '‿', '↗', '↘']
    },
    {
      label: 'Tones (Chao letters & diacritics)',
      glyphs: ['˥', '˦', '˧', '˨', '˩', '˩˥', '˥˩', '˧˥', '˥˧', '̋', '́', '̄', '̀', '̏', 'ꜛ', 'ꜜ']
    }
  ];

  const SINO: Group[] = [
    {
      label: 'Apical & back vowels',
      glyphs: ['ɿ', 'ʅ', 'ʮ', 'ʯ', 'ᴇ', 'ᴀ']
    },
    {
      label: 'Alveolo-palatals',
      glyphs: ['ȶ', 'ȡ', 'ȵ', 'ȴ']
    },
    {
      label: 'Affricate ligatures',
      glyphs: ['ʦ', 'ʣ', 'ʧ', 'ʤ', 'ʨ', 'ʥ']
    },
    {
      label: 'Chao tone numerals',
      glyphs: ['¹', '²', '³', '⁴', '⁵', '⁵⁵', '³⁵', '²¹⁴', '⁵¹', '²¹']
    }
  ];

  // Extended IPA (extIPA, for disordered speech) — real and Unicode-encoded.
  // canIPA (Canepari) proper is largely outside Unicode; only its encoded
  // members appear here, the rest needing a dedicated font.
  const EXT: Group[] = [
    {
      label: 'extIPA',
      glyphs: ['ʪ', 'ʫ', 'ʬ', 'ʭ', 'ʩ']
    },
    {
      label: 'R-coloured & other',
      glyphs: ['ɚ', 'ɝ', 'ᶑ', 'ꞎ', 'ɫ', 'ɧ', 'ɺ', 'ɥ', 'ʍ', 'ɕ', 'ʑ']
    }
  ];

  const GROUPS = $derived(tab === 'standard' ? STANDARD : tab === 'sino' ? SINO : EXT);

  const COMBINING = /[̀-ͯ᷀-᷿]/;
  function display(glyph: string): string {
    // Show a bare combining mark on a dotted circle so the key is legible.
    return COMBINING.test(glyph[0]) ? `◌${glyph}` : glyph;
  }

  function insert(glyph: string) {
    const el = document.activeElement;
    if (!(el instanceof HTMLInputElement) && !(el instanceof HTMLTextAreaElement)) return;
    const input = el;
    const start = input.selectionStart ?? input.value.length;
    const end = input.selectionEnd ?? start;
    input.value = input.value.slice(0, start) + glyph + input.value.slice(end);
    const caret = start + glyph.length;
    input.setSelectionRange(caret, caret);
    input.dispatchEvent(new Event('input', { bubbles: true }));
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<section
  class="ipa-pad"
  data-testid="ipa-pad"
  aria-label="IPA input"
  onmousedown={(event) => event.preventDefault()}
>
  <header>
    <div class="tabs" role="tablist">
      <button
        type="button"
        role="tab"
        data-testid="ipa-tab-standard"
        aria-selected={tab === 'standard'}
        class:on={tab === 'standard'}
        onclick={() => (tab = 'standard')}
      >IPA</button>
      <button
        type="button"
        role="tab"
        data-testid="ipa-tab-sino"
        aria-selected={tab === 'sino'}
        class:on={tab === 'sino'}
        onclick={() => (tab = 'sino')}
      >Sinological</button>
      <button
        type="button"
        role="tab"
        data-testid="ipa-tab-ext"
        aria-selected={tab === 'ext'}
        class:on={tab === 'ext'}
        onclick={() => (tab = 'ext')}
      >Extended</button>
    </div>
    <button type="button" class="close" data-testid="ipa-close" aria-label="Close IPA pad" onclick={onClose}>
      <IconX aria-hidden="true" />
    </button>
  </header>

  <div class="body">
    {#each GROUPS as group (group.label)}
      <div class="group">
        <span class="glabel">{group.label}</span>
        <div class="keys">
          {#each group.glyphs as glyph, i (group.label + i)}
            <button
              type="button"
              class="key"
              data-testid="ipa-key"
              data-glyph={glyph}
              onclick={() => insert(glyph)}
            >{display(glyph)}</button>
          {/each}
        </div>
      </div>
    {/each}
    {#if tab === 'ext'}
      <p class="fontnote">
        canIPA (Canepari) symbols are mostly outside Unicode; only the encoded extIPA and related
        additions appear here. Full canIPA needs a dedicated font.
      </p>
    {/if}
  </div>
</section>

<style>
  .ipa-pad {
    border-top: 1px solid var(--chrome-strong);
    background: var(--panel);
    display: flex;
    flex-direction: column;
    max-height: 14rem;
  }

  header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.35rem 0.55rem;
    border-bottom: 1px solid var(--chrome-strong);
  }

  .tabs {
    display: flex;
    gap: 0.3rem;
  }

  .tabs button {
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--muted);
    font: inherit;
    font-size: 0.76rem;
    padding: 0.2rem 0.55rem;
    cursor: pointer;
  }

  .tabs button.on {
    background: color-mix(in oklab, var(--accent) 18%, var(--panel-soft));
    color: var(--text);
    border-color: color-mix(in oklab, var(--accent) 45%, var(--chrome-strong));
  }

  .close {
    margin-left: auto;
    display: grid;
    place-items: center;
    border: none;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }

  .close:hover {
    color: var(--text);
  }

  .body {
    overflow-y: auto;
    padding: 0.35rem 0.55rem 0.6rem;
    min-height: 0;
  }

  .group {
    margin-bottom: 0.5rem;
  }

  .glabel {
    display: block;
    font-size: 0.66rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
    margin-bottom: 0.25rem;
  }

  .keys {
    display: flex;
    flex-wrap: wrap;
    gap: 0.2rem;
  }

  .key {
    min-width: 1.9rem;
    height: 1.9rem;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-sm);
    background: var(--panel-soft);
    color: var(--text);
    font-family: var(--font-ipa);
    font-size: 1.05rem;
    line-height: 1;
    cursor: pointer;
    padding: 0 0.25rem;
  }

  .key:hover {
    background: var(--panel);
    border-color: color-mix(in oklab, var(--accent) 40%, var(--chrome-strong));
  }

  .fontnote {
    margin: 0.3rem 0 0;
    font-size: 0.72rem;
    color: var(--muted);
  }
</style>
