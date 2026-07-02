# Hunspell dictionary path

Spellcheck uses Hunspell dictionary files.

Spellcheck reads settings `dictionary_path` (not `dictionary_dir`).

Spellcheck also autodetects hunspell dictionary path.

Default dictionary path:

Unix:

- /usr/share/hunspell/
- /usr/local/share/hunspell/
- $XDG_DATA_HOME/hunspell (falling back to ~/.local/share/hunspell)

macOS:

- /Library/Spelling/
- /Users/${HOME}/Library/Spelling/
- /opt/homebrew/share/hunspell/
- /System/Library/Services/AppleSpell.service/Contents/Resources/AppleSpell.8
- /Users/${HOME}/Library/Dictionaries/
- $XDG_DATA_HOME/hunspell (falling back to ~/.local/share/hunspell)

Also call system command:

```sh
hunspell -D
```
