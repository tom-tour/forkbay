# Image sources

**[Français](#français)**

Forkbay can download game images from sources you add yourself. It ships with none, apart from the optional SteamGridDB key.

A source is a small text file that describes where to look. It contains no code. Images are downloaded on your PC, only when you click **Download covers**, and stay in `Files/Covers`. You choose the source and the image types, and existing images stay unless you tick **Replace**.

**You choose the sources you add, and you are responsible for following their terms.** Only add a source you are allowed to use, and do not share images you have no right to share. Sources that point at a console maker's catalogue are not accepted in this project.

## Adding a source

Open **Settings**, then **Image sources**, and click **Add a source** to choose a `.json` file. You can also put the file in the folder that **Open folder** shows. Each source can be turned on or off there, SteamGridDB included.

### A site with one address per game

```json
{
  "name": "My images",
  "direct": {
    "grid": "https://example.com/covers/{id}.png",
    "wide": "https://example.com/banners/{name}.jpg"
  }
}
```

### A site with a search that answers in JSON

```json
{
  "name": "My search",
  "url": "https://example.com/search?q={name}",
  "results": "response.items",
  "title": "title",
  "require": { "field": "platforms", "startsWith": "switch" },
  "images": {
    "grid": ["cover_large", "cover"],
    "wide": ["banner"]
  }
}
```

| Field | Meaning |
|---|---|
| `{name}` | The game's name, for use in an address |
| `{id}` | The 16-character ID in the file name, such as `[0400000000000000]` |
| `results` | Where the list of results is in the answer, with dots between levels |
| `title` | The field holding each result's name. The closest name wins, if it is close enough |
| `require` | Optional: keeps only results whose field, or one of its values, starts with this text |
| `images` | For each image type, the fields to try in order |

Image types: `grid` (cover), `wide` (Steam banner), `hero` (Steam background), `logo`, `icon`.

Addresses must use `https`. Files that are not PNG, JPEG, WebP or ICO images, or that are over 25 MB, are refused. A file that cannot be read is skipped and noted in the journal.

---

# Français

Forkbay peut télécharger les images de tes jeux depuis des sources que tu ajoutes toi-même. Il n'en fournit aucune, à part la clé SteamGridDB facultative.

Une source est un petit fichier texte qui indique où chercher. Elle ne contient aucun code. Les images sont téléchargées sur ton PC, seulement quand tu cliques sur **Récupérer les jaquettes**, et restent dans `Files/Covers`. Tu choisis la source et les types d'images, et les images existantes restent sauf si tu coches **Remplacer**.

**Tu choisis les sources que tu ajoutes, et c'est à toi de respecter leurs conditions.** N'ajoute qu'une source que tu as le droit d'utiliser, et ne partage pas d'images que tu n'as pas le droit de partager. Les sources qui visent le catalogue d'un fabricant de consoles ne sont pas acceptées dans ce projet.

## Ajouter une source

Ouvre **Réglages**, puis **Sources d'images**, et clique sur **Ajouter une source** pour choisir un fichier `.json`. Tu peux aussi placer le fichier dans le dossier qu'ouvre **Ouvrir le dossier**. Chaque source s'y active ou se désactive, SteamGridDB compris.

### Un site avec une adresse par jeu

```json
{
  "name": "Mes images",
  "direct": {
    "grid": "https://example.com/covers/{id}.png",
    "wide": "https://example.com/banners/{name}.jpg"
  }
}
```

### Un site avec une recherche qui répond en JSON

```json
{
  "name": "Ma recherche",
  "url": "https://example.com/search?q={name}",
  "results": "response.items",
  "title": "title",
  "require": { "field": "platforms", "startsWith": "switch" },
  "images": {
    "grid": ["cover_large", "cover"],
    "wide": ["banner"]
  }
}
```

| Champ | Rôle |
|---|---|
| `{name}` | Le nom du jeu, à utiliser dans une adresse |
| `{id}` | L'identifiant de 16 caractères du nom de fichier, comme `[0400000000000000]` |
| `results` | L'endroit de la réponse où se trouve la liste des résultats, avec des points entre les niveaux |
| `title` | Le champ qui contient le nom de chaque résultat. Le nom le plus proche l'emporte, s'il est assez proche |
| `require` | Facultatif : ne garde que les résultats dont le champ, ou l'une de ses valeurs, commence par ce texte |
| `images` | Pour chaque type d'image, les champs à essayer dans l'ordre |

Types d'images : `grid` (jaquette), `wide` (bannière Steam), `hero` (fond Steam), `logo`, `icon`.

Les adresses doivent utiliser `https`. Les fichiers qui ne sont pas des images PNG, JPEG, WebP ou ICO, ou qui dépassent 25 Mo, sont refusés. Un fichier illisible est ignoré et noté dans le journal.
