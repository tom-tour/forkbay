# Adding a language

No code to change, and no build needed to try it.

1. Copy `en.json` to `xx.json`, where `xx` is the two-letter language code (`de`, `es`, `it`...).
2. Change `name` to the language's own name (`Deutsch`), then translate the **values** only. Each key is the French source text, and must stay exactly as it is.
3. To try it, drop the file in `Files/Manager/locales/` next to your copy of the program, restart it, and choose the language in Settings.
4. To share it, add the file to this `locales` folder and open a pull request. It is picked up automatically when the program is built.

Anything left out falls back to English, so a partial translation works.

## What is in the file

| Section | Content |
|---|---|
| `ui` | Window text: `"French text": "translation"` |
| `uiPatterns` | Window text with a changing part: `["pattern", "translation with $1"]`. An optional third item maps words inside `$1`. |
| `messages` | Messages from the program, same form. Keep `$1`, `$2`, etc., and leave the pattern untouched. |
| `categories` | Import categories: `"French label": ["label", "hint"]` |
| `importWords` | Lowercase category names used in the import summary |
| `importHeading` | Heading of the import summary |

The app lets you check your work: **Settings** lists every language found, and the program reports how many strings each one covers.

French is built into the code, so it has no file here.
