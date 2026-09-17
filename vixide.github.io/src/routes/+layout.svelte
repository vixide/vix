<script lang="ts">
  import { page } from '$app/state';
  import { Header, NavigationMenu } from '@lilydesignsystem/svelte-headless';
  import PickerBar from '@lilydesignsystem/svelte-picker-bar';
  import type { ShareTarget } from '@lilydesignsystem/svelte-share-picker';

  let { children } = $props();

  // The site's own locale set — the same 15 languages the Vix editor itself
  // ships translations for (locales/*.yml), so the language switcher here
  // never offers a choice the product doesn't actually have. LocalePicker
  // only sets `lang`/`dir` on <html>; there is no page-content translation
  // wired up yet — see AGENTS.md.
  const siteLocales = [
    'ar', 'bn', 'cy', 'de', 'en', 'es', 'fr', 'ga', 'gd', 'hi', 'ja', 'pl', 'pt', 'ru', 'zh'
  ];

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
  // Order and labels follow lily-design-system-svelte-with-picker-bar/index.md;
  // "Copy Link" isn't in this list — it's SharePicker's own built-in
  // copy-to-clipboard button, wired via `copyLabel` below.
  const shareTargets: ShareTarget[] = [
    {
      id: 'email',
      label: 'Email Link',
      href: (url, title) =>
        `mailto:?subject=${encodeURIComponent(title)}&body=${encodeURIComponent(url)}`
    },
    {
      id: 'linkedin',
      label: 'Share on LinkedIn',
      href: (url) => `https://www.linkedin.com/sharing/share-offsite/?url=${encodeURIComponent(url)}`
    },
    {
      id: 'reddit',
      label: 'Share on Reddit',
      href: (url, title) =>
        `https://www.reddit.com/submit?url=${encodeURIComponent(url)}&title=${encodeURIComponent(title)}`
    },
    {
      id: 'bluesky',
      label: 'Share on Bluesky',
      href: (url, title) =>
        `https://bsky.app/intent/compose?text=${encodeURIComponent(`${title} ${url}`)}`
    },
    {
      id: 'mastodon',
      label: 'Share on Mastodon',
      href: (url, title) =>
        `https://mastodonshare.com/?text=${encodeURIComponent(title)}&url=${encodeURIComponent(url)}`
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
      <!--
        themes/sizes are deliberately not passed — PickerBar defaults to
        DEFAULT_THEMES (all 45 Lily reference themes, alphabetical, UK/US
        government themes grouped last) and DEFAULT_SIZES (the 7-step text
        size scale), which is exactly the "Lily default themes/sizes, not
        any application-specific custom set" this header is required to use.
      -->
      <PickerBar
        labels={{
          theme: 'Theme',
          locale: 'Language',
          textSize: 'Text size',
          share: 'Share this page'
        }}
        themesUrl="/assets/themes/"
        locales={siteLocales}
        shareTargets={shareTargets}
        themeProps={{ defaultValue: 'light', detectFromSystem: true, storageKey: 'vix:theme' }}
        localeProps={{ defaultValue: 'en', detectFromNavigator: true, storageKey: 'vix:locale' }}
        textSizeProps={{ storageKey: 'vix:text-size' }}
        shareProps={{
          title: pageTitle,
          copyLabel: 'Copy Link',
          copiedLabel: 'Link copied',
          copyFailedLabel: 'Could not copy — copy it from the address bar'
        }}
      />
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
