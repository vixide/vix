import type { PageLoad } from './$types';

export const load: PageLoad = () => ({
  title: 'Vix — a keyboard-friendly terminal text editor and IDE',
  description:
    'Vix is a terminal text editor and IDE written in Rust, with familiar Ctrl/Cmd keys and a menu bar, plus built-in LSP, a debugger, Git and Jujutsu, Org mode, a database workbench, and an integrated terminal.'
});
