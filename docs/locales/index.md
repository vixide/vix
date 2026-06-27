# Locales

Vix can show its user interface in many languages. This covers localization
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

Translation coverage varies by language. English, Spanish, French, German, and
Welsh are the fullest. Any key that is not translated for the active language
**falls back to English**, so the interface is always fully usable.

## Adding a Language

Adding a language is a matter of filling in its column in `locales/app.yml` and
listing it in `locale_model`. See also `../internationalization/index.md`.

## Available Languages

| ISO 639-1 code | Endonym          | English equivalent |
| -------------- | ---------------- | ------------------ |
| ar             | العربية          | Arabic             |
| bg             | Български        | Bulgarian          |
| bh             | भोजपुरी          | Bhojpuri           |
| bn             | বাংলা            | Bengali            |
| cs             | Čeština          | Czech              |
| cy             | Cymraeg          | Welsh              |
| da             | Dansk            | Danish             |
| de             | Deutsch          | German             |
| el             | Ελληνικά         | Greek              |
| en             | English          | English            |
| es             | Español          | Spanish            |
| et             | Eesti            | Estonian           |
| eu             | Euskara          | Basque             |
| fa             | فارسی            | Persian            |
| fi             | Suomi            | Finnish            |
| fr             | Français         | French             |
| ga             | Gaeilge          | Irish              |
| gu             | ગુજરાતી          | Gujarati           |
| ha             | Harshen Hausa    | Hausa              |
| hi             | हिन्दी           | Hindi              |
| hr             | Hrvatski         | Croatian           |
| hu             | Magyar           | Hungarian          |
| id             | Bahasa Indonesia | Indonesian         |
| it             | Italiano         | Italian            |
| ja             | 日本語           | Japanese           |
| jv             | Basa Jawa        | Javanese           |
| ko             | 한국어           | Korean             |
| lt             | Lietuvių         | Lithuanian         |
| lv             | Latviešu         | Latvian            |
| mr             | मराठी            | Marathi            |
| mt             | Malti            | Maltese            |
| nl             | Nederlands       | Dutch              |
| pa             | ਪੰਜਾਬੀ           | Punjabi            |
| pl             | Polski           | Polish             |
| pt             | Português        | Portuguese         |
| ro             | Română           | Romanian           |
| ru             | Русский          | Russian            |
| sk             | Slovenčina       | Slovak             |
| sl             | Slovenščina      | Slovenian          |
| sv             | Svenska          | Swedish            |
| ta             | தமிழ்            | Tamil              |
| te             | తెలుగు           | Telugu             |
| tr             | Türkçe           | Turkish            |
| ur             | اردو             | Urdu               |
| vi             | Tiếng Việt       | Vietnamese         |
| zh             | 普通话           | Mandarin           |
