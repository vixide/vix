<script lang="ts">
  // The theme gallery (T741): one real screenshot of Vix per bundled theme,
  // in a carousel modelled on Micro's colorscheme carousel.
  //
  // The slides sit in Lily's headless `Carousel`, which every Lily theme
  // styles as a horizontal scroll-snap row, so the gallery still works by
  // swiping or scrolling with JavaScript off. With JavaScript, the previous
  // and next buttons and the row of theme names scroll to a slide, and the
  // caption follows whichever slide is in view.
  //
  // The screenshots are the main repo's docs/demos/themes/*.png, copied
  // verbatim; scripts/render-theme-gallery.py there captures each one from
  // the real `vix` binary in a real pseudo-terminal.
  import { Carousel } from '@lilydesignsystem/svelte-headless';

  type Theme = { slug: string; name: string };

  let { themes }: { themes: Theme[] } = $props();

  let current = $state(0);
  let row: HTMLElement | undefined;

  function show(index: number): void {
    current = (index + themes.length) % themes.length;
    const slide = document.getElementById(`theme-${themes[current].slug}`);
    if (!row || !slide) return;
    const reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    row.scrollTo({ left: slide.offsetLeft, behavior: reduce ? 'auto' : 'smooth' });
  }

  function onscroll(event: Event): void {
    row = event.currentTarget as HTMLElement;
    const index = Math.round(row.scrollLeft / row.clientWidth);
    if (index >= 0 && index < themes.length) current = index;
  }

  function locate(node: HTMLElement): void {
    row = node.querySelector<HTMLElement>('.carousel') ?? undefined;
  }
</script>

<div class="site-theme-gallery" {@attach locate}>
  <div class="site-theme-gallery-controls">
    <button type="button" class="button button-secondary" onclick={() => show(current - 1)}>
      <span aria-hidden="true">←</span> Previous
    </button>
    <p class="site-theme-gallery-status" aria-live="polite">
      <strong>{themes[current].name}</strong>
      <span>{current + 1} of {themes.length}</span>
    </p>
    <button type="button" class="button button-secondary" onclick={() => show(current + 1)}>
      Next <span aria-hidden="true">→</span>
    </button>
  </div>

  <Carousel label="Vix theme gallery" {onscroll}>
    {#each themes as theme, i (theme.slug)}
      <figure
        class="demo site-theme-slide"
        id={`theme-${theme.slug}`}
        role="group"
        aria-roledescription="slide"
        aria-label={`${theme.name}, ${i + 1} of ${themes.length}`}
      >
        <img
          src={`/assets/demos/themes/${theme.slug}.png`}
          width="1200"
          height="672"
          loading={i === 0 ? 'eager' : 'lazy'}
          alt={`Vix with the ${theme.name} theme: the menu bar, the file explorer, a Rust file with syntax highlighting, the Messages and Output docks, and the status bar.`}
        />
        <figcaption><strong>{theme.name}</strong></figcaption>
      </figure>
    {/each}
  </Carousel>

  <ul class="site-theme-gallery-names" aria-label="Choose a theme">
    {#each themes as theme, i (theme.slug)}
      <li>
        <button
          type="button"
          class="site-theme-gallery-name"
          aria-pressed={i === current}
          onclick={() => show(i)}
        >
          {theme.name}
        </button>
      </li>
    {/each}
  </ul>
</div>
