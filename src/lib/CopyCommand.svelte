<script lang="ts">
  // One shell command in a code block, with a button that copies it.
  //
  // The button is Lily's headless `ClipboardCopyButton`; this wrapper adds
  // the visible text and a polite live region, because the headless button
  // only flips `data-copied` and announces nothing by itself.
  import { ClipboardCopyButton } from '@lilydesignsystem/svelte-headless';

  let {
    command,
    label = 'Copy command'
  }: {
    /** The exact command; it is what's shown and what's copied. */
    command: string;
    /** Accessible name for the copy button. */
    label?: string;
  } = $props();

  let status = $state('');
  let timer: ReturnType<typeof setTimeout> | undefined;

  function announce(message: string): void {
    status = message;
    clearTimeout(timer);
    timer = setTimeout(() => (status = ''), 2000);
  }
</script>

<div class="copy-command">
  <pre><code>{command}</code></pre>
  <ClipboardCopyButton
    text={command}
    {label}
    onsuccess={() => announce('Copied')}
    onerror={() => announce('Could not copy — select the text instead')}
  >
    {status === 'Copied' ? 'Copied' : 'Copy'}
  </ClipboardCopyButton>
  <span class="visually-hidden" role="status">{status}</span>
</div>
