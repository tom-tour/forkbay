# Contributing

**[Français](#français)**

Pull requests are welcome. For a large change, please open an issue first so that we can talk about it.

## Rules

- **Never add keys, firmware, games, dumps or copyrighted artwork**, in code, tests, issues or screenshots.
- Nothing that helps get them from anywhere other than your own console.
- By contributing, you agree that your work is released under this project's license, GPL-3.0-or-later.
- Bugs inside a game belong to the emulator's own project.

## Building and testing

```bash
cargo build --release
```

On Linux, first install `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev` and `libjavascriptcoregtk-4.1-dev`.

Tests run the real program on throwaway folders:

```bash
pwsh ci/run-windows.ps1 -Bin target/release/forkbay.exe
```

```bash
bash ci/run-linux.sh target/release/forkbay screenshots
```

## Texts and languages

French is the source text, written in the code, and each French text is also its translation key. If you change one, update the matching key or pattern in `locales/en.json`.

To add a language, see [locales/README.md](locales/README.md). No code is needed.

---

# Français

Les pull requests sont les bienvenues. Pour un gros changement, ouvre d'abord un ticket pour qu'on en discute.

## Règles

- **N'ajoute jamais de clés, de firmware, de jeux, de dumps ni d'images protégées**, que ce soit dans le code, les tests, les tickets ou les captures d'écran.
- Rien qui aide à les obtenir autrement que depuis ta propre console.
- En contribuant, tu acceptes que ton travail soit publié sous la licence du projet, GPL-3.0-or-later.
- Les bugs dans un jeu concernent le projet de l'émulateur.

## Compiler et tester

```bash
cargo build --release
```

Sous Linux, installe d'abord `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev` et `libjavascriptcoregtk-4.1-dev`.

Les tests lancent le vrai programme sur des dossiers jetables :

```bash
pwsh ci/run-windows.ps1 -Bin target/release/forkbay.exe
```

```bash
bash ci/run-linux.sh target/release/forkbay screenshots
```

## Textes et langues

Le français est le texte source, écrit dans le code, et chaque texte français sert aussi de clé de traduction. Si tu en modifies un, mets à jour la clé ou le motif correspondant dans `locales/en.json`.

Pour ajouter une langue, consulte [locales/README.md](locales/README.md). Aucun code n'est nécessaire.
