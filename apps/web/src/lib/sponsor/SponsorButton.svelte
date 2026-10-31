<script lang="ts">
  import { base } from '$app/paths';
  import IconCoffee from '~icons/lucide/coffee';
  import IconArrowRight from '~icons/lucide/arrow-right';
  import IconMail from '~icons/lucide/mail';
  import IconX from '~icons/lucide/x';

  // The mode rail's Sponsor button, led by the author's face, and the dialog it opens: her
  // portrait, why she needs help, then three steps that end at the Ko-fi button.
  let open = $state(false);
  let dialog = $state<HTMLDialogElement>();

  $effect(() => {
    if (open && dialog && !dialog.open) dialog.showModal();
  });

  // The dialog lives under <body>, outside the fixed rail and its stacking context.
  function portal(node: HTMLElement) {
    document.body.append(node);
    return { destroy: () => node.remove() };
  }

  // A click on the backdrop closes the dialog; one on its own scrollbar also targets the dialog,
  // so only a click outside its box counts.
  function outside(event: MouseEvent) {
    if (!dialog || event.target !== dialog) return;
    const box = dialog.getBoundingClientRect();
    const { clientX: x, clientY: y } = event;
    if (x < box.left || x > box.right || y < box.top || y > box.bottom) dialog.close();
  }

  const heart =
    'M12 21s-8.5-5.1-8.5-11.3A4.7 4.7 0 0 1 12 6.9a4.7 4.7 0 0 1 8.5 2.8C20.5 15.9 12 21 12 21Z';
</script>

<svelte:head>
  <script type="module" src="https://supporters.mkpo.li/roll.js"></script>
</svelte:head>

<button
  type="button"
  class="sponsor"
  aria-haspopup="dialog"
  title="Sponsor"
  data-testid="sponsor"
  onclick={() => (open = true)}
>
  <span class="face" aria-hidden="true">
    <img src="{base}/sponsor-face.webp" alt="" width="30" height="30" decoding="async" />
    <svg class="face-heart" viewBox="0 0 24 24"><path d={heart} /></svg>
  </span>
  <span class="label">Sponsor</span>
</button>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <dialog
    class="sponsor-dialog"
    bind:this={dialog}
    use:portal
    aria-labelledby="sponsor-title"
    onclose={() => (open = false)}
    onclick={outside}
  >
    <header>
      <h2 id="sponsor-title">Sponsor</h2>
      <button type="button" class="close" aria-label="Close" onclick={() => dialog?.close()}>
        <IconX aria-hidden="true" />
      </button>
    </header>
    <div class="sheet">
      <div class="hero" aria-hidden="true">
        <span class="portrait">
          <img src="{base}/sponsor-portrait.webp" alt="" width="112" height="112" decoding="async" />
          <svg class="heart" viewBox="0 0 24 24">
            <path d={heart} />
            <ellipse class="shine" cx="7.6" cy="9.6" rx="1.5" ry="2.1" transform="rotate(-30 7.6 9.6)" />
          </svg>
        </span>
        <span class="spark s1">✦</span><span class="spark s2">✦</span><span class="spark s3">✦</span>
      </div>
      <p class="lead">
        I make Phonia on my own, out of love for speech sounds, and I pay for it myself. I don't have
        a job, and my small TA pay doesn't cover rent and food. Some days I can barely afford to eat.
      </p>
      <p class="ask">Please help me keep going 🥺 Even one dollar helps me eat today.</p>
      <section class="how">
        <h3>How to help</h3>
        <ol class="steps" role="list">
          <li>Tap Ko-fi below.</li>
          <li>Pick any amount. Small is fine.</li>
          <li>Pay with PayPal or a card. No sign-up needed.</li>
        </ol>
      </section>
      <a class="kofi" href="https://ko-fi.com/mkpoli" target="_blank" rel="noreferrer">
        <span class="mark" aria-hidden="true"><IconCoffee /></span>
        <span class="copy"><strong>Ko-fi</strong><small>One-time or monthly</small></span>
        <IconArrowRight class="arrow" aria-hidden="true" />
      </a>
      <a class="company" href="mailto:mkpoli@mkpo.li?subject=Sponsorship%20on%20phonia.app">
        <IconMail aria-hidden="true" />
        <span class="copy">
          <strong>Company sponsorship</strong>
          <small>I'd love to put your ad on this site. Please write to mkpoli@mkpo.li.</small>
        </span>
      </a>
      <supporters-roll label="Recent supporters" anonymous="Someone"></supporters-roll>
      <p class="thanks">Thank you for reading this far. Your help keeps me fed and keeps Phonia growing&nbsp;💗</p>
    </div>
  </dialog>
{/if}

<style>
  .sponsor,
  .sponsor-dialog {
    --sp-ink: #c2185b;
    --sp-heart: #ff4f88;
    --sp-line: #ffc2d6;
    --sp-soft: #fff1f5;
    --sp-from: #ffe6ef;
    --sp-to: #fff0e0;
  }

  :global(:root.dark) .sponsor,
  :global(:root.dark) .sponsor-dialog {
    --sp-ink: #ffb3cd;
    --sp-heart: #ff7aa6;
    --sp-line: #6b2c47;
    --sp-soft: #2f1822;
    --sp-from: #3d1a2b;
    --sp-to: #3a2318;
  }

  .sponsor {
    width: 3.75rem;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.4rem;
    padding: 0.6rem 0.25rem 0.5rem;
    border: 1px solid var(--sp-line);
    border-radius: var(--radius-xl);
    background: linear-gradient(160deg, var(--sp-from), var(--sp-to));
    color: var(--sp-ink);
    cursor: pointer;
    box-shadow: 0 1px 2px rgb(194 24 91 / 0.12);
  }

  .sponsor:focus-visible,
  .close:focus-visible,
  .kofi:focus-visible,
  .company:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }

  .face {
    position: relative;
    flex: none;
    width: 1.9rem;
    height: 1.9rem;
  }

  .face img {
    display: block;
    width: 100%;
    height: 100%;
    border-radius: 50%;
    object-fit: cover;
    box-shadow:
      0 0 0 1.5px var(--panel),
      0 0 0 2.5px var(--sp-line);
  }

  .face-heart {
    position: absolute;
    right: -4px;
    bottom: -3px;
    width: 13px;
    height: 13px;
    fill: var(--sp-heart);
    stroke: var(--panel);
    stroke-width: 2.5;
    paint-order: stroke;
    overflow: visible;
    transform-origin: 50% 60%;
  }

  .label {
    font-size: 0.6rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }

  @media (max-width: 720px) {
    .sponsor {
      width: auto;
      flex: 1 1 0;
      height: calc(var(--mobile-rail-height) - 0.7rem);
      justify-content: center;
      gap: 0.2rem;
      margin: 0.35rem;
      padding: 0.3rem 0.25rem;
    }

    .face {
      width: 1.6rem;
      height: 1.6rem;
    }

    .label {
      font-size: 0.6rem;
      letter-spacing: 0.04em;
    }
  }

  .sponsor-dialog {
    width: min(480px, calc(100vw - 32px));
    max-height: calc(100dvh - 32px);
    margin: auto;
    padding: 0;
    border: 1px solid var(--chrome-strong);
    border-radius: var(--radius-xl);
    background: var(--panel);
    color: var(--text);
    box-shadow: var(--shadow-lg);
    overflow: auto;
  }

  .sponsor-dialog::backdrop {
    background: rgb(0 0 0 / 0.4);
    backdrop-filter: blur(4px);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.85rem 0.85rem 0 1.5rem;
  }

  h2 {
    margin: 0;
    font-size: 1rem;
    font-weight: 700;
  }

  .close {
    display: grid;
    place-items: center;
    width: 2.25rem;
    height: 2.25rem;
    padding: 0;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }

  .close:hover {
    background: var(--panel-soft);
    color: var(--text);
  }

  .close :global(svg) {
    width: 18px;
    height: 18px;
  }

  .sheet {
    display: flex;
    flex-direction: column;
    gap: 0.875rem;
    padding: 0.25rem 1.5rem 1.375rem;
  }

  .hero {
    position: relative;
    display: grid;
    place-items: center;
    height: 8.5rem;
    background: radial-gradient(closest-side, color-mix(in oklab, var(--sp-heart) 22%, transparent), transparent);
  }

  .portrait {
    position: relative;
    width: 7rem;
    height: 7rem;
  }

  .portrait img {
    display: block;
    width: 100%;
    height: 100%;
    border-radius: 50%;
    object-fit: cover;
    box-shadow:
      0 0 0 4px var(--panel),
      0 0 0 6px var(--sp-line),
      0 10px 24px rgb(255 79 136 / 0.3);
  }

  .heart {
    position: absolute;
    right: -2px;
    bottom: 2px;
    width: 34px;
    height: 34px;
    fill: var(--sp-heart);
    stroke: var(--panel);
    stroke-width: 2;
    paint-order: stroke;
    overflow: visible;
    transform-origin: 50% 60%;
    filter: drop-shadow(0 3px 6px rgb(255 79 136 / 0.35));
  }

  .heart .shine {
    fill: #fff;
    stroke: none;
    opacity: 0.6;
  }

  .spark {
    position: absolute;
    color: var(--sp-heart);
    opacity: 0.75;
  }

  .s1 {
    top: 22px;
    left: calc(50% - 84px);
    font-size: 14px;
  }

  .s2 {
    top: 12px;
    left: calc(50% + 66px);
    font-size: 10px;
  }

  .s3 {
    bottom: 20px;
    left: calc(50% - 76px);
    font-size: 11px;
  }

  .lead {
    margin: 0;
    font-size: 0.875rem;
    line-height: 1.65;
  }

  .ask {
    margin: 0;
    font-size: 0.9rem;
    font-weight: 650;
    line-height: 1.5;
    color: var(--sp-ink);
  }

  .how {
    padding: 0.75rem 0.875rem;
    border: 1px dashed var(--sp-line);
    border-radius: var(--radius-xl);
    background: var(--sp-soft);
  }

  h3 {
    margin: 0 0 0.5rem;
    font-size: 0.75rem;
    font-weight: 700;
    color: var(--sp-ink);
  }

  .steps {
    display: flex;
    flex-direction: column;
    gap: 0.375rem;
    margin: 0;
    padding: 0;
    list-style: none;
    counter-reset: step;
  }

  .steps li {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    font-size: 0.8125rem;
    line-height: 1.45;
    counter-increment: step;
  }

  .steps li::before {
    content: counter(step);
    display: grid;
    flex: none;
    place-items: center;
    width: 1.25rem;
    height: 1.25rem;
    border-radius: 50%;
    background: var(--sp-ink);
    color: var(--sp-soft);
    font-size: 0.6875rem;
    font-weight: 700;
    line-height: 1;
    transform: translateY(-1px);
  }

  .kofi {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem 1.125rem 0.75rem 0.75rem;
    border-radius: 9999px;
    background: linear-gradient(135deg, #d81b60, #b0124f);
    color: #fff;
    text-decoration: none;
    box-shadow: 0 6px 16px rgb(216 27 96 / 0.32);
  }

  .mark {
    display: grid;
    flex: none;
    place-items: center;
    width: 2.375rem;
    height: 2.375rem;
    border-radius: 50%;
    background: rgb(255 255 255 / 0.22);
  }

  .mark :global(svg) {
    width: 20px;
    height: 20px;
  }

  .kofi :global(.arrow) {
    flex: none;
    width: 18px;
    height: 18px;
  }

  .copy {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .copy strong {
    font-size: 0.84rem;
    font-weight: 700;
  }

  .copy small {
    font-size: 0.75rem;
    line-height: 1.4;
  }

  .kofi .copy strong {
    font-size: 0.94rem;
  }

  .company {
    display: flex;
    align-items: flex-start;
    gap: 0.625rem;
    padding: 0.5rem 0.75rem;
    border-radius: var(--radius-lg);
    color: var(--muted);
    text-decoration: none;
  }

  .company:hover {
    background: var(--panel-soft);
    color: var(--text);
  }

  .company > :global(svg) {
    flex: none;
    width: 16px;
    height: 16px;
    margin-top: 2px;
  }

  .company .copy strong {
    font-size: 0.78rem;
    color: var(--text);
  }

  supporters-roll {
    --sp-text: var(--text);
    --sp-muted: var(--muted);
  }

  .thanks {
    margin: 0;
    font-size: 0.78rem;
    line-height: 1.5;
    color: var(--muted);
    text-align: center;
  }

  @media (prefers-reduced-motion: no-preference) {
    .sponsor {
      transition:
        transform var(--t-fast),
        box-shadow var(--t-fast);
    }

    .sponsor:hover {
      transform: translateY(-1px);
      box-shadow: 0 4px 12px rgb(255 79 136 / 0.28);
    }

    .face-heart,
    .heart {
      animation: beat 2.8s ease-in-out infinite;
    }

    .sponsor:hover .face-heart,
    .sponsor:focus-visible .face-heart {
      animation: bounce 0.6s ease-in-out infinite;
    }

    .spark {
      animation: twinkle 2.4s ease-in-out infinite;
    }

    .s2 {
      animation-delay: 0.8s;
    }

    .s3 {
      animation-delay: 1.6s;
    }

    .sponsor-dialog[open] {
      animation: arrive 0.32s cubic-bezier(0.2, 0.8, 0.2, 1);
    }

    .kofi {
      transition:
        transform var(--t-fast),
        box-shadow var(--t-fast);
    }

    .kofi:hover {
      transform: translateY(-1px);
      box-shadow: 0 8px 22px rgb(216 27 96 / 0.42);
    }
  }

  @keyframes beat {
    0%,
    70%,
    100% {
      transform: scale(1);
    }
    78% {
      transform: scale(1.18);
    }
    86% {
      transform: scale(0.96);
    }
    93% {
      transform: scale(1.1);
    }
  }

  @keyframes bounce {
    0%,
    100% {
      transform: translateY(0) scale(1);
    }
    40% {
      transform: translateY(-2px) scale(1.18) rotate(-8deg);
    }
  }

  @keyframes twinkle {
    0%,
    100% {
      opacity: 0.2;
      transform: scale(0.7);
    }
    50% {
      opacity: 0.9;
      transform: scale(1);
    }
  }

  @keyframes arrive {
    from {
      opacity: 0;
      transform: translateY(14px) scale(0.98);
    }
  }
</style>
