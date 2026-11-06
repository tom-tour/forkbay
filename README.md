# Switch 2 Manager

[![Ko-fi](https://img.shields.io/badge/Ko--fi-Support-FF5E5B?logo=ko-fi&logoColor=white)](https://ko-fi.com/tom_t)
[![License: GPL v3+](https://img.shields.io/badge/License-GPLv3+-blue.svg)](LICENSE)

One window for your Switch 2 emulators on Windows: launch games, keep emulators up to date, share one data folder between them, add games to Steam and sync your saves. No install, nothing to compile: it runs on the PowerShell and .NET that ship with Windows 10 and 11.

**[Français](#français)**

## What it does

- **One data folder:** every emulator uses the same saves, keys, firmware and settings.
- **Portable or installed:** keep your data next to the tool (nothing on `C:`, ideal on an external drive), in AppData, or in a folder you choose.
- **One emulator per game:** pick it once, and launchers and Steam follow your choice.
- **Updates:** compares each emulator with its latest release, then installs it after checking the download and scanning it with Microsoft Defender. The previous version is kept.
- **Your own emulators:** add one from a GitHub repository or from a local `.exe`.
- **Steam:** adds games to your library, leaves your other shortcuts alone, and survives a drive letter change.
- **Cloud saves (optional):** syncs before and after each game, with a choice of version when both sides changed.
- **Game art:** covers and banners from SteamGridDB, for the tool and for Steam.
- **First run:** finds existing emulator data on your PC and offers to import it.

## What you need

- Windows 10 or 11.
- Your own keys, firmware and games, dumped from your own console. **None of them are provided here, and they must never be shared or committed.**

## Getting started

1. Copy `Switch 2 Manager.bat` and the `Files` folder wherever you like.
2. Put your games (`.nup`, `.nsp`, `.xci`) in `Files\Roms`.
3. Put `prod.keys` and `title.keys` in a folder inside `Files`, and your `Firmware*.zip` directly in `Files`.
4. Run `Switch 2 Manager.bat` and follow the first-run screen.

If Windows blocks the scripts of a downloaded copy, right-click the zip, choose **Properties**, tick **Unblock**, then extract it again.

## Adding an emulator

Open **Émulateurs**, then **Ajouter un émulateur**:

- **From GitHub:** give it a name and its `owner/project`. The tool checks that a Windows release exists. The optional filter is a regular expression that picks the file to download.
- **From a local `.exe`:** the folder is copied into `Files`. It has no update check.

Only add sources you trust: an emulator is a program that runs on your PC. The built-in list is in `Files/Manager/emulators.json`.

## Cloud saves

Saves, the user profile and settings are synced. **Keys and firmware are never sent.**

| Before playing | What happens |
|---|---|
| Only the cloud changed | The local version is replaced, after a backup in `Files\Save Backups` |
| Only this PC changed | The local version is uploaded |
| Both changed | You see both dates and choose, or cancel the launch |

Two options:

- **Synced folder:** a Google Drive for desktop, OneDrive, Dropbox or NAS folder. No API needed.
- **Google Drive API:** create a project on [console.cloud.google.com](https://console.cloud.google.com), enable the Google Drive API, set the consent screen to External and **publish it** (otherwise the sign-in expires after 7 days), then create an OAuth client of type *Desktop app* and paste its ID and secret into the **Cloud** page. The tool only asks for the `drive.file` scope, so it only sees the files it created.

Secrets are encrypted for your Windows account, so another PC has to sign in again.

## Game art

Get a free API key from your [SteamGridDB](https://www.steamgriddb.com) preferences and paste it into **Réglages**. You can also drop your own images into `Files\Covers`, named like `Game Name-grid.png` (`grid`, `wide`, `hero`, `logo` or `icon`).

## Limits

- **Windows only:** the interface uses WPF, and the shared folder uses NTFS junctions.
- **The interface is in French.**
- **Moving the folder:** Steam shortcuts find a new drive letter on their own. After moving to another path, open the **Steam** page and click **Enregistrer**.
- Emulation of these games is experimental and changes daily: a newer emulator can run a game worse. Your previous version stays in `Files\Old Versions`.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

---

# Français

Une seule fenêtre pour tes émulateurs Switch 2 sous Windows : lancer les jeux, garder les émulateurs à jour, partager un seul dossier de données, ajouter les jeux à Steam et synchroniser tes sauvegardes. Rien à installer ni à compiler : l'outil utilise le PowerShell et le .NET fournis avec Windows 10 et 11.

## Ce que fait l'outil

- **Un seul dossier de données :** tous les émulateurs utilisent les mêmes sauvegardes, clés, firmware et réglages.
- **Portable ou installé :** tes données restent à côté de l'outil (rien sur `C:`, idéal sur un disque externe), dans AppData, ou dans le dossier de ton choix.
- **Un émulateur par jeu :** tu le choisis une fois, et les lanceurs et Steam suivent ce choix.
- **Mises à jour :** compare chaque émulateur à sa dernière version, puis l'installe après avoir vérifié le téléchargement et l'avoir analysé avec Microsoft Defender. La version précédente est gardée.
- **Tes propres émulateurs :** ajoutes-en un depuis un dépôt GitHub ou depuis un `.exe` local.
- **Steam :** ajoute les jeux à ta bibliothèque, ne touche pas à tes autres raccourcis et résiste à un changement de lettre de disque.
- **Sauvegardes dans le cloud (facultatif) :** synchronise avant et après chaque partie, et te laisse choisir si les deux versions ont changé.
- **Images des jeux :** jaquettes et bannières depuis SteamGridDB, pour l'outil et pour Steam.
- **Premier lancement :** cherche les données d'émulateurs déjà présentes sur ton PC et propose de les importer.

## Ce qu'il te faut

- Windows 10 ou 11.
- Tes propres clés, firmware et jeux, extraits de ta propre console. **Rien de tout cela n'est fourni ici, et il ne faut jamais les partager ni les ajouter au dépôt.**

## Pour commencer

1. Copie `Switch 2 Manager.bat` et le dossier `Files` où tu veux.
2. Mets tes jeux (`.nup`, `.nsp`, `.xci`) dans `Files\Roms`.
3. Mets `prod.keys` et `title.keys` dans un dossier à l'intérieur de `Files`, et ton `Firmware*.zip` directement dans `Files`.
4. Lance `Switch 2 Manager.bat` et suis l'écran de premier lancement.

Si Windows bloque les scripts d'une copie téléchargée, fais un clic droit sur le zip, choisis **Propriétés**, coche **Débloquer**, puis extrais-le de nouveau.

## Ajouter un émulateur

Ouvre **Émulateurs**, puis **Ajouter un émulateur** :

- **Depuis GitHub :** donne-lui un nom et son `propriétaire/projet`. L'outil vérifie qu'une version Windows existe. Le filtre facultatif est une expression régulière qui choisit le fichier à télécharger.
- **Depuis un `.exe` local :** le dossier est copié dans `Files`. Ses mises à jour restent manuelles.

N'ajoute que des sources de confiance : un émulateur est un programme qui s'exécute sur ton PC. La liste fournie se trouve dans `Files/Manager/emulators.json`.

## Sauvegardes dans le cloud

Les sauvegardes, le profil utilisateur et les réglages sont synchronisés. **Les clés et le firmware ne sont jamais envoyés.**

| Avant de jouer | Ce qui se passe |
|---|---|
| Seul le cloud a changé | La version locale est remplacée, après une copie dans `Files\Save Backups` |
| Seul ce PC a changé | La version locale est envoyée |
| Les deux ont changé | Tu vois les deux dates et tu choisis, ou tu annules le lancement |

Deux possibilités :

- **Dossier synchronisé :** un dossier Google Drive pour ordinateur, OneDrive, Dropbox ou NAS. Aucune API nécessaire.
- **API Google Drive :** crée un projet sur [console.cloud.google.com](https://console.cloud.google.com), active Google Drive API, règle l'écran de consentement sur Externe et **publie-le** (sinon la connexion expire après 7 jours), puis crée un client OAuth de type *Application de bureau* et colle son identifiant et son secret dans la page **Cloud**. L'outil demande seulement l'accès `drive.file` : il ne voit que les fichiers qu'il a créés.

Les secrets sont chiffrés pour ton compte Windows : un autre PC devra se reconnecter.

## Images des jeux

Récupère une clé API gratuite dans tes préférences [SteamGridDB](https://www.steamgriddb.com) et colle-la dans **Réglages**. Tu peux aussi déposer tes images dans `Files\Covers`, sous la forme `Nom du jeu-grid.png` (`grid`, `wide`, `hero`, `logo` ou `icon`).

## Limites

- **Windows uniquement :** l'interface utilise WPF, et le dossier partagé des liens NTFS.
- **L'interface est en français.**
- **Déplacer le dossier :** les raccourcis Steam retrouvent seuls une nouvelle lettre de disque. Après un déplacement vers un autre chemin, ouvre la page **Steam** et clique sur **Enregistrer**.
- L'émulation de ces jeux est expérimentale et change chaque jour : un émulateur plus récent peut moins bien faire tourner un jeu. La version précédente reste dans `Files\Old Versions`.

## Licence

GPL-3.0-or-later. Voir [LICENSE](LICENSE).
