<script lang="ts">
  import { page } from '$app/state';
  import { Header, NavigationMenu } from 'lily-design-system-svelte-headless';
  import { ThemePicker } from 'lily-design-system-svelte-theme-picker';
  import { SharePicker, type ShareTarget } from 'lily-design-system-svelte-share-picker';

  let { children } = $props();

  type NavLink = { href: string; label: string };
  const navLinks: NavLink[] = [
    { href: '/', label: 'Home' },
    { href: '/features/', label: 'Features' },
    { href: '/install/', label: 'Install' }
  ];

  function isCurrent(href: string): boolean {
    return page.url.pathname === href;
  }

  // The `page.data.title` convention: every route's `+page.ts` load returns
  // `{ title }`, this layout renders it as the one `<title>`, and SharePicker
  // is handed the same string — so a shared link always carries the title of
  // the page it was shared from, not a generic site name.
  const pageTitle = $derived(page.data.title ?? 'Vix');

  // Vix ships no third-party endpoints of its own — each `href` builds the
  // destination URL from the shared page's own title, not a hardcoded one.
  const shareTargets: ShareTarget[] = [
    {
      id: 'linkedin',
      label: 'LinkedIn',
      href: (url) => `https://www.linkedin.com/sharing/share-offsite/?url=${encodeURIComponent(url)}`
    },
    {
      id: 'mastodon',
      label: 'Mastodon',
      href: (url, title) =>
        `https://mastodon.social/share?text=${encodeURIComponent(`${title} ${url}`)}`
    },
    {
      id: 'bluesky',
      label: 'Bluesky',
      href: (url, title) =>
        `https://bsky.app/intent/compose?text=${encodeURIComponent(`${title} ${url}`)}`
    },
    {
      id: 'reddit',
      label: 'Reddit',
      href: (url, title) =>
        `https://www.reddit.com/submit?url=${encodeURIComponent(url)}&title=${encodeURIComponent(title)}`
    }
  ];
</script>

<svelte:head>
  <title>{pageTitle}</title>
</svelte:head>

<a class="skip-link" href="#main">Skip to main content</a>

<Header class="site-header" label="Site header">
  <div class="site-header-inner">
    <a class="site-brand" href="/" aria-label="Vix home">
      <img class="site-brand-mark" src="/assets/favicon.svg" alt="" aria-hidden="true" />
      <span>Vix</span>
    </a>
    <div class="site-header-right">
      <NavigationMenu class="site-nav" label="Main">
        {#each navLinks as link (link.href)}
          <a href={link.href} aria-current={isCurrent(link.href) ? 'page' : undefined}>
            {link.label}
          </a>
        {/each}
        <a href="https://github.com/vixide/vix">GitHub</a>
      </NavigationMenu>
      <div class="site-controls">
        <ThemePicker
          label="Theme"
          themesUrl="/assets/themes/"
          themes={['light', 'dark']}
          defaultValue="light"
          detectFromSystem
          storageKey="vix:theme"
        />
        <SharePicker
          label="Share this page"
          title={pageTitle}
          targets={shareTargets}
          copyLabel="Copy link"
          copiedLabel="Link copied"
          copyFailedLabel="Could not copy — copy it from the address bar"
        />
      </div>
    </div>
  </div>
</Header>

<main id="main" class="site-main">
  {@render children()}
</main>

<footer class="site-footer">
  <div class="site-footer-inner">
    <p>
      Vix™ is free open-source software — Apache-2.0, BSD-3-Clause, MIT, GPL-2.0,
      or GPL-3.0, your choice.
    </p>
    <p class="site-footer-trademark">Vix™ and Vix IDE™ are trademarks.</p>
    <nav class="site-footer-links" aria-label="Footer">
      <a href="https://github.com/vixide/vix">GitHub</a>
      <a href="https://github.com/vixide/vix/blob/main/CHANGELOG.md">Changelog</a>
      <a href="/features/">Features</a>
      <a href="/install/">Install</a>
    </nav>
  </div>
</footer>
