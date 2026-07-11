# Welcome Panel

The first-run welcome screen's scroll state.

Vix shows this overlay the first time it runs (and on demand from **Help →
Welcome…**). The *text* lives in the host's i18n catalog (the `welcome.body`
locale key) so it is translatable; this crate is pure state — it holds the
lines the host hands it and tracks the scroll offset. The host renders the
visible window with a scrollbar and forwards scroll keys.
