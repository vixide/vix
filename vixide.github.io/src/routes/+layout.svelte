<script lang="ts">
  import { page } from '$app/state';
  import { Header, NavigationMenu } from 'lily-design-system-svelte-headless';

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
</script>

<a class="skip-link" href="#main">Skip to main content</a>

<Header class="site-header" label="Site header">
  <div class="site-header-inner">
    <a class="site-brand" href="/" aria-label="Vix home">
      <img class="site-brand-mark" src="/assets/favicon.svg" alt="" aria-hidden="true" />
      <span>Vix</span>
    </a>
    <NavigationMenu class="site-nav" label="Main">
      {#each navLinks as link (link.href)}
        <a href={link.href} aria-current={isCurrent(link.href) ? 'page' : undefined}>
          {link.label}
        </a>
      {/each}
      <a href="https://github.com/vixide/vix">GitHub</a>
    </NavigationMenu>
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
