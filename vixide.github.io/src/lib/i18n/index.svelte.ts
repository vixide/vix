// Page-content translation for the home page (T748).
//
// The header's LocalePicker owns the language choice: it detects it from
// the browser on a first visit, stores it under `vix:locale`, and sets
// `lang`/`dir` on <html>. This module mirrors that choice as reactive
// state, fed by the picker's `onChange` (wired in +layout.svelte), which
// fires once when the picker mounts and again on every change.
//
// The state starts as 'en' so the prerendered HTML and the first client
// render are both English and hydration matches; the swap to another
// language happens after hydration.

import { catalogs, sourceLocale, type MessageKey } from './messages';

/** The language the page is currently shown in. */
const state = $state({ locale: sourceLocale });

/** The language currently shown, reactive. */
export function currentLocale(): string {
  return state.locale;
}

/**
 * Called with the picker's chosen code (any BCP 47 form, e.g. `zh` or
 * `pt-BR`); unknown languages fall back to English.
 */
export function setLocale(code: string): void {
  const base = code.replace(/_/g, '-').split('-')[0].toLowerCase();
  state.locale = base in catalogs ? base : sourceLocale;
}

/**
 * Look a message up in the current language, falling back to English for a
 * missing string, and substitute `{name}` placeholders from `params`.
 */
export function t(key: MessageKey, params: Record<string, string> = {}): string {
  const text = catalogs[state.locale]?.[key] ?? catalogs[sourceLocale][key] ?? key;
  return text.replace(/\{(\w+)\}/g, (whole, name: string) => params[name] ?? whole);
}
