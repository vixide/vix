// The message catalogs: one JSON file per language in ./locales, English
// the source of truth (`en.json` defines every key). A language file may
// omit a key, in which case the page shows the English string for it.
// `scripts/check-i18n.js` fails the check step if a language has a key
// English lacks or uses an unknown placeholder or markup tag.

import en from './locales/en.json';
import ar from './locales/ar.json';
import bn from './locales/bn.json';
import cy from './locales/cy.json';
import de from './locales/de.json';
import es from './locales/es.json';
import fr from './locales/fr.json';
import ga from './locales/ga.json';
import gd from './locales/gd.json';
import hi from './locales/hi.json';
import ja from './locales/ja.json';
import pl from './locales/pl.json';
import pt from './locales/pt.json';
import ru from './locales/ru.json';
import zh from './locales/zh.json';

export type MessageKey = keyof typeof en;
export type Catalog = Partial<Record<MessageKey, string>>;

export const sourceLocale = 'en';

export const catalogs: Record<string, Catalog> = {
  en, ar, bn, cy, de, es, fr, ga, gd, hi, ja, pl, pt, ru, zh
};
