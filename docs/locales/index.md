# Locales

Vix™ can show its user interface in many languages. This covers localization
(l10n) and internationalization (i18n) of the UI text.

**Status:** Shipped. 27 languages are selectable today: English, Spanish, French,
German, Welsh, Irish, Scottish Gaelic, Polish, Portuguese, Russian, Arabic,
Hindi, Bengali, Chinese, Japanese, Italian, Korean, Turkish, Dutch, Vietnamese,
Indonesian, Thai, Persian, Ukrainian, Greek — plus the constructed languages
Klingon (`tlh`) and Sindarin (`sjn`).

## Choosing a Language

Open **View → Locale…** to pick the active UI language. The change applies live —
the interface re-renders in the selected language immediately.

## Configuration

The selected language persists in the `locale` setting, so it applies the next
time you start Vix.

To override the saved language for a single run without persisting it, pass the
`--locale` flag at startup, for example:

```sh
vix --locale fr
```

## Coverage and Fallback

Translation coverage varies. The first fifteen (English through Japanese in the
table below) have every UI string translated; Italian, Korean, Turkish, Dutch,
Vietnamese, Indonesian, Thai, Persian, Ukrainian and Greek have only a handful
of keys so far, and Klingon and Sindarin a few more. Any key that is not
translated for the active language **falls back to English**, so the interface
is always fully usable.

## Adding a Language

Adding a language is a matter of filling in its column across the `locales/*.yml`
files and listing it in `locale_model`. See also `../internationalization/index.md`.

## Available Languages

The list is `LOCALES` in `crates/vix-locale-model`, in this order.

| Code | Endonym | English equivalent |
| ---- | ------- | ------------------ |
| en | English | English |
| es | Español | Spanish |
| fr | Français | French |
| de | Deutsch | German |
| cy | Cymraeg | Welsh |
| ga | Gaeilge | Irish |
| gd | Gàidhlig | Scottish Gaelic |
| pl | Polski | Polish |
| pt | Português | Portuguese |
| ru | Русский | Russian |
| ar | العربية | Arabic |
| hi | हिन्दी | Hindi |
| bn | বাংলা | Bengali |
| zh | 中文 | Chinese |
| ja | 日本語 | Japanese |
| it | Italiano | Italian |
| ko | 한국어 | Korean |
| tr | Türkçe | Turkish |
| nl | Nederlands | Dutch |
| vi | Tiếng Việt | Vietnamese |
| id | Bahasa Indonesia | Indonesian |
| th | ไทย | Thai |
| fa | فارسی | Persian |
| uk | Українська | Ukrainian |
| el | Ελληνικά | Greek |
| tlh | tlhIngan Hol | Klingon (constructed) |
| sjn | Edhellen | Sindarin (constructed) |

---

Vix™ and Vix IDE™ are trademarks.
