<img src="assets/icon.png" width="64" alt="" align="right">

# Switch 2 Manager

[![Ko-fi](https://img.shields.io/badge/Ko--fi-Support-FF5E5B?logo=ko-fi&logoColor=white)](https://ko-fi.com/tom_t)
[![License: GPL v3+](https://img.shields.io/badge/License-GPLv3+-blue.svg)](LICENSE)

One window for your Switch 2 emulators on Windows and Linux: launch games, keep emulators up to date, share one data folder between them, add games to Steam and sync your saves. It is a single small program, written in Rust.

**[Français](#français)**

## What it does

- **Guided setup:** imports what already exists on your PC, walks you through your keys and firmware, then installs an emulator.
- **Choose what you import:** keys, firmware, saves, profiles, controllers, network, graphics, per-game settings, mods, shader cache, screenshots, virtual SD card.
- **One data folder:** every emulator uses the same saves, keys, firmware and settings.
- **Portable or installed:** keep your data next to the tool, in your user folder, or anywhere you choose.
- **One emulator per game:** pick it once, and Steam follows your choice.
- **Updates:** compares each emulator with its latest release, and keeps the previous version. On Windows, downloads are also scanned with Microsoft Defender.
- **Your own emulators:** add one from a GitHub repository or from a local program.
- **Steam:** adds games to your library and leaves your other shortcuts alone.
- **Covers (optional):** no key needed, thanks to Nintendo's public catalogue. A free SteamGridDB key adds Steam logos and banners.
- **Cloud saves (optional):** syncs before and after each game, with a choice of version when both sides changed.
- **English and French:** follows your system language, and can be changed in the settings.
- **Updates of the tool itself** from GitHub releases, with an About page for the license and links.
- **Dark theme by default**, with a light one.

## What you need

- Windows 10 or 11 (WebView2, included in current versions), or Linux with WebKitGTK 4.1.
- Your own keys, firmware and games, dumped from your own console. **None of them are provided here, and they must never be shared or committed.**

## Getting started

1. Put the program in the folder of your choice. It creates a `Files` folder next to itself.
2. Run it and follow the guided setup.
3. Put your games (`.nup`, `.nsp`, `.xci`) in `Files/Roms`.

To build it yourself, run `cargo build --release` (on Windows, `tools/build-release.ps1` also keeps your local paths out of the program). On Linux, first install `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev` and `libjavascriptcoregtk-4.1-dev`.

## Adding an emulator

Open **Émulateurs**, then **Ajouter un émulateur**. From GitHub, give it a name and its `owner/project`: the tool checks that a usable release exists. Only add sources you trust: an emulator is a program that runs on your PC. The built-in list is in `emulators.json`.

## Cloud saves

Saves, the user profile and settings are synced. **Keys and firmware are never sent.**

| Before playing | What happens |
|---|---|
| Only the cloud changed | The local version is replaced, after a backup in `Files/Save Backups` |
| Only this PC changed | The local version is uploaded |
| Both changed | You see both dates and choose, or cancel the launch |

- **Synced folder:** a Google Drive for desktop, OneDrive, Dropbox or NAS folder. No API needed.
- **Google Drive API:** create a project on [console.cloud.google.com](https://console.cloud.google.com), enable the Google Drive API, set the consent screen to External and **publish it** (otherwise the sign-in expires after 7 days), then create an OAuth client of type *Desktop app* and paste its ID and secret into the **Cloud** page. The tool only asks for the `drive.file` scope, so it only sees the files it created.

On Windows, secrets are encrypted for your account. On Linux, they stay in a settings file readable only by you.

## Limits

- **Linux:** emulators are started from their AppImage, with their folders pointed at the shared data. Lichii by Frizti has no Linux build.
- **Moving the folder:** on Windows, Steam shortcuts find a new drive letter on their own. After any other move, open the **Steam** page and click **Enregistrer**.
- Emulation of these games is experimental and changes daily: a newer emulator can run a game worse. The previous version stays in `Files/Old Versions`.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

---

# Français

Une seule fenêtre pour tes émulateurs Switch 2 sous Windows et Linux : lancer les jeux, garder les émulateurs à jour, partager un seul dossier de données, ajouter les jeux à Steam et synchroniser tes sauvegardes. C'est un seul petit programme, écrit en Rust.

## Ce que fait l'outil

- **Installation guidée :** importe ce qui existe déjà sur ton PC, t'accompagne pour tes clés et ton firmware, puis installe un émulateur.
- **Tu choisis ce que tu importes :** clés, firmware, sauvegardes, profils, manettes, réseau, graphismes, réglages par jeu, mods, cache de shaders, captures d'écran, carte SD virtuelle.
- **Un seul dossier de données :** tous les émulateurs utilisent les mêmes sauvegardes, clés, firmware et réglages.
- **Portable ou installé :** tes données restent à côté de l'outil, dans ton dossier utilisateur, ou où tu veux.
- **Un émulateur par jeu :** tu le choisis une fois, et Steam suit ce choix.
- **Mises à jour :** compare chaque émulateur à sa dernière version et garde la précédente. Sous Windows, les téléchargements sont aussi analysés avec Microsoft Defender.
- **Tes propres émulateurs :** ajoutes-en un depuis un dépôt GitHub ou depuis un programme local.
- **Steam :** ajoute les jeux à ta bibliothèque sans toucher à tes autres raccourcis.
- **Jaquettes (facultatif) :** aucune clé nécessaire, grâce au catalogue public de Nintendo. Une clé gratuite SteamGridDB ajoute des logos et bannières pour Steam.
- **Sauvegardes dans le cloud (facultatif) :** synchronise avant et après chaque partie, et te laisse choisir si les deux versions ont changé.
- **Anglais et français :** suit la langue de ton système, et se change dans les réglages.
- **Mise à jour de l'outil lui-même** depuis les versions GitHub, avec une page À propos pour la licence et les liens.
- **Thème sombre par défaut**, avec un thème clair.

## Ce qu'il te faut

- Windows 10 ou 11 (WebView2, inclus dans les versions à jour), ou Linux avec WebKitGTK 4.1.
- Tes propres clés, firmware et jeux, extraits de ta propre console. **Rien de tout cela n'est fourni ici, et il ne faut jamais les partager ni les ajouter au dépôt.**

## Pour commencer

1. Place le programme dans le dossier de ton choix. Il crée un dossier `Files` à côté de lui.
2. Lance-le et suis l'installation guidée.
3. Mets tes jeux (`.nup`, `.nsp`, `.xci`) dans `Files/Roms`.

Pour le compiler toi-même : `cargo build --release` (sous Windows, `tools/build-release.ps1` garde aussi tes chemins locaux hors du programme). Sous Linux, installe d'abord `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev` et `libjavascriptcoregtk-4.1-dev`.

## Ajouter un émulateur

Ouvre **Émulateurs**, puis **Ajouter un émulateur**. Depuis GitHub, donne-lui un nom et son `propriétaire/projet` : l'outil vérifie qu'une version utilisable existe. N'ajoute que des sources de confiance : un émulateur est un programme qui s'exécute sur ton PC. La liste fournie se trouve dans `emulators.json`.

## Sauvegardes dans le cloud

Les sauvegardes, le profil utilisateur et les réglages sont synchronisés. **Les clés et le firmware ne sont jamais envoyés.**

| Avant de jouer | Ce qui se passe |
|---|---|
| Seul le cloud a changé | La version locale est remplacée, après une copie dans `Files/Save Backups` |
| Seul ce PC a changé | La version locale est envoyée |
| Les deux ont changé | Tu vois les deux dates et tu choisis, ou tu annules le lancement |

- **Dossier synchronisé :** un dossier Google Drive pour ordinateur, OneDrive, Dropbox ou NAS. Aucune API nécessaire.
- **API Google Drive :** crée un projet sur [console.cloud.google.com](https://console.cloud.google.com), active Google Drive API, règle l'écran de consentement sur Externe et **publie-le** (sinon la connexion expire après 7 jours), puis crée un client OAuth de type *Application de bureau* et colle son identifiant et son secret dans la page **Cloud**. L'outil demande seulement l'accès `drive.file` : il ne voit que les fichiers qu'il a créés.

Sous Windows, les secrets sont chiffrés pour ton compte. Sous Linux, ils restent dans un fichier de réglages lisible par toi seul.

## Limites

- **Linux :** les émulateurs sont lancés depuis leur AppImage, avec leurs dossiers dirigés vers les données partagées. Lichii par Frizti n'a pas de version Linux.
- **Déplacer le dossier :** sous Windows, les raccourcis Steam retrouvent seuls une nouvelle lettre de disque. Après tout autre déplacement, ouvre la page **Steam** et clique sur **Enregistrer**.
- L'émulation de ces jeux est expérimentale et change chaque jour : un émulateur plus récent peut moins bien faire tourner un jeu. La version précédente reste dans `Files/Old Versions`.

## Licence

GPL-3.0-or-later. Voir [LICENSE](LICENSE).
