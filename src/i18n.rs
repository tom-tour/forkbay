// Interface language. French text is the source; other languages translate it here,
// in one place, so that messages stay readable where they are written.
use regex::Regex;
use std::sync::OnceLock;

/// The language to display: the setting, or the system language when set to automatic.
pub fn current() -> &'static str {
    static LANG: OnceLock<&'static str> = OnceLock::new();
    LANG.get_or_init(|| resolve(&crate::core::load_settings().language))
}

pub fn resolve(setting: &str) -> &'static str {
    match setting {
        "fr" => "fr",
        "en" => "en",
        _ => if system().starts_with("fr") { "fr" } else { "en" },
    }
}

pub fn is_en() -> bool {
    current() == "en"
}

/// The system language, lowercase (S2M_LANG overrides it for tests).
pub fn system() -> String {
    if let Ok(l) = std::env::var("S2M_LANG") {
        return l.to_lowercase();
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Globalization::GetUserDefaultLocaleName;
        let mut buf = [0u16; 85];
        let n = unsafe { GetUserDefaultLocaleName(buf.as_mut_ptr(), buf.len() as i32) };
        if n > 1 {
            return String::from_utf16_lossy(&buf[..n as usize - 1]).to_lowercase();
        }
        String::new()
    }
    #[cfg(not(windows))]
    {
        ["LC_ALL", "LC_MESSAGES", "LANG"].iter().find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty())).unwrap_or_default().to_lowercase()
    }
}

const TABLE: &[(&str, &str)] = &[
    // Setup
    (r"^(.+) est un vrai dossier : déplace son contenu dans Data, puis supprime-le\.$", "$1 is a real folder: move its content to Data, then delete it."),
    (r"^Impossible de créer le lien : (.+)$", "Could not create the link: $1"),
    (r"^Copie impossible : (.+)$", "Could not copy: $1"),
    (r"^Choisis ton fichier prod\.keys\.$", "Choose your prod.keys file."),
    (r"^Ce fichier n'est pas un fichier de clés lisible\.$", "This is not a readable key file."),
    (r"^Ce fichier ne ressemble pas à un fichier de clés\.$", "This does not look like a key file."),
    (r"^prod\.keys et title\.keys installé\.$", "prod.keys and title.keys installed."),
    (r"^prod\.keys installé\.$", "prod.keys installed."),
    (r"^Ce fichier n'est pas une archive zip de firmware\.$", "This is not a firmware zip archive."),
    (r"^Aucun fichier \.nca trouvé : ce n'est pas un firmware\.$", "No .nca file found: this is not a firmware."),
    (r"^Firmware installé : (\d+) fichiers\.$", "Firmware installed: $1 files."),
    (r"^Ferme les émulateurs avant de déplacer les données\.$", "Close the emulators before moving the data."),
    (r"^Les données sont déjà à cet endroit\.$", "The data is already there."),
    (r"^Le dossier cible n'est pas vide : (.+)$", "The target folder is not empty: $1"),
    (r"^La copie est incomplète : rien n'a changé\.$", "The copy is incomplete: nothing changed."),
    (r"^Données déplacées vers (.+)\. L'ancienne copie reste dans (.+) : supprime-la quand tout fonctionne\.$", "Data moved to $1. The old copy stays in $2: delete it once everything works."),
    // Emulators
    (r"^Réseau : (.+)$", "Network: $1"),
    (r"^Pas de version pour ce système\.$", "No version for this system."),
    (r"^Aucune version trouvée pour ce système\.$", "No version found for this system."),
    (r"^Téléchargement : (.+)$", "Download: $1"),
    (r"^(.+) : téléchargement (\d+) %$", "$1: downloading $2%"),
    (r"^(.+) : vérification et installation\.\.\.$", "$1: checking and installing..."),
    (r"^Émulateur inconnu : (.+)$", "Unknown emulator: $1"),
    (r"^(.+) est une copie locale : elle se met à jour à la main\.$", "$1 is a local copy: update it by hand."),
    (r"^Ferme les émulateurs avant d'installer\.$", "Close the emulators before installing."),
    (r"^(.+) est déjà à jour \((.+)\)\.$", "$1 is already up to date ($2)."),
    (r"^Téléchargement incomplet : taille différente\.$", "Incomplete download: the size differs."),
    (r"^Condensat différent : fichier refusé\.$", "Checksum mismatch: file refused."),
    (r"^Aucun exécutable d'émulateur dans l'archive\.$", "No emulator program in the archive."),
    (r"^Defender signale une menace : installation annulée\.$", "Defender reports a threat: installation cancelled."),
    (r"^(.+) est un vrai dossier : installation annulée\.$", "$1 is a real folder: installation cancelled."),
    (r"^(.+) : (.+) -> (.+) \(vérifié : condensat SHA-256\)\. Ancienne version gardée dans Old Versions\.$", "$1: $2 -> $3 (verified: SHA-256 checksum). Previous version kept in Old Versions."),
    (r"^(.+) : (.+) -> (.+) \(vérifié : taille\)\. Ancienne version gardée dans Old Versions\.$", "$1: $2 -> $3 (verified: size). Previous version kept in Old Versions."),
    (r"^(.+) (\S+) installé \(vérifié : condensat SHA-256\)\.$", "$1 $2 installed (verified: SHA-256 checksum)."),
    (r"^(.+) (\S+) installé \(vérifié : taille\)\.$", "$1 $2 installed (verified: size)."),
    (r"^Nom invalide : lettres, chiffres, espaces, points et tirets seulement\.$", "Invalid name: letters, digits, spaces, dots and dashes only."),
    (r"^Un émulateur s'appelle déjà (.+)\.$", "An emulator is already called $1."),
    (r"^Ce nom est réservé : (.+)\.$", "This name is reserved: $1."),
    (r"^Dépôt invalide : écris propriétaire/projet, ou colle son adresse GitHub\.$", "Invalid repository: write owner/project, or paste its GitHub address."),
    (r"^Filtre invalide\.$", "Invalid filter."),
    (r"^Aucune version utilisable sur (.+) : (.+)$", "No usable version on $1: $2"),
    (r"^(.+) ajouté : dernière version (.+)\. Clique sur Installer\.$", "$1 added: latest version $2. Click Install."),
    (r"^Choisis le programme de l'émulateur\.$", "Choose the emulator program."),
    (r"^Le dossier existe déjà : (.+)$", "The folder already exists: $1"),
    (r"^(.+) ajouté\. Ses mises à jour restent à faire à la main\.$", "$1 added. Update it by hand."),
    (r"^(.+) retiré de la liste\. Son dossier est conservé\.$", "$1 removed from the list. Its folder is kept."),
    (r"^(.+) n'est pas installé\.$", "$1 is not installed."),
    (r"^(.+) n'est pas installé\. L'installer maintenant \?$", "$1 is not installed. Install it now?"),
    // Import
    (r"^Dossier introuvable\.$", "Folder not found."),
    (r"^Dossier introuvable : (.+)$", "Folder not found: $1"),
    (r"^Aucune donnée d'émulateur reconnue dans ce dossier\.$", "No emulator data recognised in this folder."),
    (r"^Ferme les émulateurs avant d'importer\.$", "Close the emulators before importing."),
    (r"^Rien de nouveau à importer : ce qui existe déjà n'est jamais écrasé\.$", "Nothing new to import: existing files are never overwritten."),
    // Steam
    (r"^Fichier tronqué\.$", "Truncated file."),
    (r"^Format de shortcuts\.vdf inattendu\.$", "Unexpected shortcuts.vdf format."),
    (r"^Relecture différente de l'original : fichier non modifié\.$", "Read-back differs from the original: file left unchanged."),
    (r"^Steam introuvable sur ce PC\.$", "Steam not found on this PC."),
    (r"^Steam ne s'est pas fermé : rien n'a été modifié\.$", "Steam did not close: nothing was changed."),
    (r"^Enregistré : (\d+) jeu\(x\) dans Steam, (\d+) image\(s\)\.$", "Saved: $1 game(s) in Steam, $2 image(s)."),
    // Art
    (r"^Images : (.+)\.\.\.$", "Images: $1..."),
    (r"^(\d+) image\(s\) téléchargée\(s\)\. Sans jaquette : (.+)\.$", "$1 image(s) downloaded. No cover: $2."),
    (r"^(\d+) image\(s\) téléchargée\(s\)\.$", "$1 image(s) downloaded."),
    // Cloud
    (r"^Remplacement annulé, version locale restaurée : contenu différent de la version distante$", "Replacement cancelled, local version restored: content differs from the remote version"),
    (r"^Remplacement annulé, version locale restaurée : (.+)$", "Replacement cancelled, local version restored: $1"),
    (r"^Aucun dossier de synchronisation choisi\.$", "No sync folder chosen."),
    (r"^Dossier de synchronisation introuvable : (.+)$", "Sync folder not found: $1"),
    (r"^Compte Google non connecté sur ce PC : clique sur Connecter dans Cloud\.$", "Google account not connected on this PC: click Connect in Cloud."),
    (r"^Google n'a pas donné de jeton\.$", "Google gave no token."),
    (r"^Renseigne d'abord l'identifiant et le secret du client OAuth\.$", "First enter the OAuth client ID and secret."),
    (r"^Connexion Google non terminée après 5 minutes\.$", "Google sign-in not finished after 5 minutes."),
    (r"^Réponse Google non reconnue : connexion refusée\.$", "Unrecognised Google response: sign-in refused."),
    (r"^Connexion refusée par Google\.$", "Sign-in refused by Google."),
    (r"^Google n'a pas renvoyé de jeton durable\.$", "Google returned no lasting token."),
    (r"^Compte Google connecté\.$", "Google account connected."),
    (r"^Fichier distant introuvable$", "Remote file not found"),
    (r"^Un émulateur est ouvert : synchronisation impossible pour l'instant\.$", "An emulator is open: cannot sync for now."),
    (r"^Choisis un dossier de synchronisation existant\.$", "Choose an existing sync folder."),
    (r"^Déjà synchronisé : les deux versions sont identiques\.$", "Already in sync: both versions are identical."),
    (r"^Version locale envoyée dans le cloud\.$", "Local version sent to the cloud."),
    (r"^Version distante installée\. Une copie de la version locale est dans Save Backups\.$", "Remote version installed. A copy of the local version is in Save Backups."),
    (r"^Synchronisation annulée : rien n'a changé\.$", "Sync cancelled: nothing changed."),
    // Launching
    (r"^Jeu introuvable : (.+)$", "Game not found: $1"),
    (r"(?s)^Vérification du cloud impossible :\n(.*)\n\nJouer quand même \? Les versions seront comparées au prochain échange\.$", "Could not check the cloud:\n$1\n\nPlay anyway? Versions will be compared at the next exchange."),
    (r"(?s)^Envoi vers le cloud impossible :\n(.*)\n\nTa partie est bien gardée sur ce PC et sera envoyée au prochain échange\.$", "Could not send to the cloud:\n$1\n\nYour game is safe on this PC and will be sent at the next exchange."),
    // Updates of the tool
    (r"^Aucune version publiée pour l'instant\.$", "No version published yet."),
    (r"^Version de l'outil introuvable pour ce système\.$", "No version of the tool for this system."),
    (r"^Mise à jour : téléchargement (\d+) %$", "Update: downloading $1%"),
    (r"^Mise à jour installée : redémarrage\.\.\.$", "Update installed: restarting..."),
    (r"^Déjà à jour \((.+)\)\.$", "Already up to date ($1)."),
    (r"^Commande inconnue : (.+)$", "Unknown command: $1"),
];

const CATEGORIES: &[(&str, &str)] = &[
    ("clés", "keys"), ("firmware", "firmware"), ("sauvegardes", "saves"), ("profils", "profiles"), ("manettes", "controllers"), ("réseau", "network"),
    ("graphismes et audio", "graphics and audio"), ("réglages par jeu", "per-game settings"), ("mods", "mods"), ("cache de shaders", "shader cache"),
    ("captures d'écran", "screenshots"), ("carte sd virtuelle", "virtual SD card"),
];

/// Translates a message written in French. Anything unknown is returned as it is.
pub fn t(text: &str) -> String {
    if !is_en() {
        return text.to_string();
    }
    if let Some(list) = text.strip_prefix("Importé : ").and_then(|s| s.strip_suffix('.')) {
        let mut out = list.to_string();
        for (fr, en) in CATEGORIES { out = out.replace(fr, en); }
        return format!("Imported: {out}.");
    }
    static COMPILED: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();
    let table = COMPILED.get_or_init(|| TABLE.iter().map(|(p, r)| (Regex::new(p).expect("translation pattern"), *r)).collect());
    for (re, rep) in table {
        if re.is_match(text) {
            return re.replace(text, *rep).into_owned();
        }
    }
    text.to_string()
}

/// Category labels sent to the page.
pub fn category(label: &str, hint: &str) -> (String, String) {
    if !is_en() {
        return (label.to_string(), hint.to_string());
    }
    let en = match label {
        "Clés" => ("Keys", "prod.keys and title.keys"),
        "Firmware" => ("Firmware", "The console's system software"),
        "Sauvegardes" => ("Saves", "Your game progress"),
        "Profils" => ("Profiles", "User accounts and avatars"),
        "Manettes" => ("Controllers", "Buttons and ports"),
        "Réseau" => ("Network", "Network, online account, multiplayer"),
        "Graphismes et audio" => ("Graphics and audio", "Rendering, processor, sound"),
        "Réglages par jeu" => ("Per-game settings", "config/custom files"),
        "Mods" => ("Mods", "load folder"),
        "Cache de shaders" => ("Shader cache", "Avoids some stutter, may not work on another emulator"),
        "Captures d'écran" => ("Screenshots", "screenshots folder"),
        "Carte SD virtuelle" => ("Virtual SD card", "sdmc folder"),
        _ => return (label.to_string(), hint.to_string()),
    };
    (en.0.to_string(), en.1.to_string())
}
