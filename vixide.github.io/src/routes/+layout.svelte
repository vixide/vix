<script lang="ts">
  import { page } from '$app/state';
  import { Header, NavigationMenu } from '@lilydesignsystem/svelte-headless';
  import PickerBar from '@lilydesignsystem/svelte-picker-bar';
  import type { ShareTarget } from '@lilydesignsystem/svelte-share-picker';
  import { setLocale } from '#lib/i18n/index.svelte.ts';

  let { children } = $props();

  // The site's own locale set — the same 15 languages the Vix editor itself
  // ships translations for (locales/*.yml), so the language switcher here
  // never offers a choice the product doesn't actually have. LocalePicker
  // sets `lang`/`dir` on <html> and reports the choice through `onChange`
  // (wired to src/lib/i18n below, which swaps the home page's text) — see
  // AGENTS.md.
  const siteLocales = [
    'ar', 'bn', 'cy', 'de', 'en', 'es', 'fr', 'ga', 'gd', 'hi', 'ja', 'pl', 'pt', 'ru', 'zh'
  ];

  type NavLink = { href: string; label: string };
  const navLinks: NavLink[] = [
    { href: '/', label: 'Home' },
    { href: '/features/', label: 'Features' },
    { href: '/whats-new/', label: 'What’s new' },
    { href: '/install/', label: 'Install' }
  ];

  // The link picker lists every page, plus the docs and the repository.
  const pickerLinks = [
    ...navLinks.map((l) => ({ href: l.href, label: l.label })),
    { href: 'https://vixide.github.io/vix/', label: 'Docs' },
    { href: 'https://github.com/vixide/vix', label: 'GitHub' }
  ];

  // The Vix documentation (the main repo's docs/, built with mdBook) is
  // published at /vix/ on this same host. PickerBar's search box sends its
  // query there: mdBook opens its own search with the terms from a
  // `?search=` parameter, so a header search lands on real results instead
  // of a search page this static site would have to build itself.
  const docsUrl = 'https://vixide.github.io/vix/';

  // SearchPicker hands `navigate` its default href, `${action}?<encoded
  // query>` (a bare query, no parameter name), so the query is read back
  // from that and re-sent under the name mdBook reads.
  function searchDocs(href: string): void {
    const query = decodeURIComponent(href.slice(href.indexOf('?') + 1));
    location.assign(`${docsUrl}?search=${encodeURIComponent(query)}`);
  }

  function isCurrent(href: string): boolean {
    return page.url.pathname === href;
  }

  // The `page.data.title` convention: every route's `+page.ts` load returns
  // `{ title }`, this layout renders it as the one `<title>`, and SharePicker
  // is handed the same string — so a shared link always carries the title of
  // the page it was shared from, not a generic site name.
  const pageTitle = $derived(page.data.title ?? 'Vix');

  // The same convention for the description: each route's load returns
  // `{ description }`, and this layout renders it once as the page's
  // `<meta name="description">` and again for Open Graph and Twitter.
  const pageDescription = $derived(
    page.data.description ?? 'Vix, a keyboard-friendly terminal text editor and IDE.'
  );
  const pageUrl = $derived(`https://vixide.github.io${page.url.pathname}`);

  // Social cards need an absolute URL. The card is the main repo's
  // docs/demos/social-card.png, copied verbatim.
  const socialImage = 'https://vixide.github.io/assets/social-card.png';
  const socialImageAlt =
    'Vix, a keyboard-friendly terminal text editor and IDE: a real screenshot of Vix with a menu bar, a file explorer, a Rust file open in the editor, and a status bar.';

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

<!--
  The one <title> and description, plus Open Graph and Twitter card tags
  (T740), so a shared link to any page shows that page's title, its
  description and the social card: a 1200x630 PNG built around a real
  screenshot of Vix (scripts/render-theme-gallery.py in the main repo).
-->
<svelte:head>
  <title>{pageTitle}</title>
  <meta name="description" content={pageDescription} />
  <link rel="canonical" href={pageUrl} />
  <meta property="og:type" content="website" />
  <meta property="og:site_name" content="Vix" />
  <meta property="og:locale" content="en_US" />
  <meta property="og:title" content={pageTitle} />
  <meta property="og:description" content={pageDescription} />
  <meta property="og:url" content={pageUrl} />
  <meta property="og:image" content={socialImage} />
  <meta property="og:image:type" content="image/png" />
  <meta property="og:image:width" content="1200" />
  <meta property="og:image:height" content="630" />
  <meta property="og:image:alt" content={socialImageAlt} />
  <meta name="twitter:card" content="summary_large_image" />
  <meta name="twitter:title" content={pageTitle} />
  <meta name="twitter:description" content={pageDescription} />
  <meta name="twitter:image" content={socialImage} />
  <meta name="twitter:image:alt" content={socialImageAlt} />
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
        <a href={docsUrl}>Docs</a>
        <a href="https://github.com/vixide/vix">GitHub</a>
      </NavigationMenu>
      <!--
        The full PickerBar: link (home icon, leftmost), search, theme, locale,
        time zone, text size and share. themes/sizes are deliberately not
        passed — PickerBar defaults to DEFAULT_THEMES (all Lily reference
        themes) and DEFAULT_SIZES (the 7-step scale).
      -->
      <PickerBar
        labels={{
          link: 'Site pages',
          timeZone: 'Time zone',
          search: 'Search the documentation',
          searchInput: 'Search terms',
          searchSubmit: 'Search',
          theme: 'Theme',
          locale: 'Language',
          textSize: 'Text size',
          share: 'Share this page'
        }}
        searchProps={{ action: docsUrl, navigate: searchDocs, placeholder: 'Search the docs…' }}
        links={pickerLinks}
        linkProps={{ navigate: (href: string) => location.assign(href) }}
        timeZoneProps={{ locale: 'en' }}
        themesUrl="/assets/themes/"
        locales={siteLocales}
        shareTargets={shareTargets}
        themeProps={{ defaultValue: 'light', detectFromSystem: true, storageKey: 'vix:theme' }}
        localeProps={{
          defaultValue: 'en',
          detectFromNavigator: true,
          storageKey: 'vix:locale',
          onChange: setLocale
        }}
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
      <a href={docsUrl}>Documentation</a>
      <a href="https://github.com/vixide/vix/blob/main/CHANGELOG.md">Changelog</a>
      <a href="/features/">Features</a>
      <a href="/whats-new/">What’s new</a>
      <a href="/install/">Install</a>
    </nav>
  </div>
</footer>
