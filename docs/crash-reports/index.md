# Crash reports

If Vix crashes, it saves a crash report on your computer, so you can send it
to the maintainers if you choose. Nothing is sent automatically.

## What happens when Vix crashes

Vix saves a report to the `crashes` folder in its data folder (usually
`~/.local/share/vix/crashes/` on Linux and macOS, `%APPDATA%\vix\crashes\`
on Windows) and prints the exact path after the terminal is restored. The next time you start Vix, it says once that a report
is waiting.

## What a report holds

- the Vix version, your operating system and its architecture;
- the panic message, with quoted text removed, and where in Vix's source it
  happened;
- a backtrace of Vix's own code.

Your home folder is shown as `~` and your user name as `<user>`. A report
never holds the text of your files or buffers, your environment variables or
your computer's name. Vix keeps the ten newest reports.

## Sending one

Choose **Help → Report a Crash…**. Vix opens the newest report in a read-only
tab so you can read exactly what it says, and asks whether to open a GitHub
issue with it filled in. Press **y** to open the issue in your browser.
Nothing reaches GitHub until you submit the issue there, and you can edit or
delete anything in it first. Press **n** to close the question.

If no browser opens, Vix puts the issue link in the message drawer. You can
also attach the file from the crashes folder to an issue yourself.

---

Vix™ and Vix IDE™ are trademarks.
