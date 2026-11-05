<script lang="ts">
  // Renders a translated message that carries a little inline markup:
  // <k>Ctrl+S</k> becomes <kbd>, <c>vix</c> becomes <code>, and <a>text</a>
  // becomes a link to `href`. Anything else is plain text (never HTML), so a
  // catalog string cannot inject markup.
  let { text, href = '' }: { text: string; href?: string } = $props();

  type Part = { tag: '' | 'k' | 'c' | 'a'; text: string };

  const parts = $derived.by(() => {
    const out: Part[] = [];
    const re = /<(k|c|a)>(.*?)<\/\1>/gs;
    let last = 0;
    for (const m of text.matchAll(re)) {
      if (m.index > last) out.push({ tag: '', text: text.slice(last, m.index) });
      out.push({ tag: m[1] as Part['tag'], text: m[2] });
      last = m.index + m[0].length;
    }
    if (last < text.length) out.push({ tag: '', text: text.slice(last) });
    return out;
  });
</script>

{#each parts as part, i (i)}{#if part.tag === 'k'}<kbd>{part.text}</kbd>{:else if part.tag === 'c'}<code>{part.text}</code>{:else if part.tag === 'a'}<a {href}>{part.text}</a>{:else}{part.text}{/if}{/each}
