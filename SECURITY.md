# Security

**[Français](#français)**

## Reporting a vulnerability

Please do not open a public issue. Use **Report a vulnerability** in this repository's **Security** tab, so that it can be fixed before it is public. Include the version, your system and the steps to reproduce it.

This project is maintained in spare time, so there is no guaranteed response time.

## What is in scope

Forkbay itself: its window, updates, downloads, Steam shortcuts, cloud sync, backups and bug reports.

A problem inside an emulator belongs to that emulator's own project.

## How it works

- **Links** open in your browser, never in the Forkbay window. The window cannot load anything from the web.
- **Forkbay updates** are refused unless the file matches the SHA-256 checksum published by GitHub. The checksum comes from the same place as the file: it detects a damaged download, not a compromised GitHub account.
- **Emulator downloads** are checked the same way when the source publishes a checksum. Otherwise only the size is checked, and the result message says so. On Windows they are also scanned with Microsoft Defender. An emulator is a program that runs on your PC, so only add sources you trust.
- **Secrets** are encrypted for your account on Windows. On Linux they stay in a settings file readable only by you.
- **Bug reports** remove paths, names and keys, and you see the full text before anything is sent.
- **The program is not code-signed**, so Windows SmartScreen may warn you the first time you run it.

---

# Français

## Signaler une faille

Merci de ne pas ouvrir de ticket public. Utilise **Report a vulnerability** dans l'onglet **Security** de ce dépôt, pour qu'elle soit corrigée avant d'être rendue publique. Indique la version, ton système et les étapes pour la reproduire.

Ce projet est maintenu sur du temps libre : le délai de réponse n'est pas garanti.

## Ce qui est concerné

Forkbay lui-même : sa fenêtre, ses mises à jour, ses téléchargements, les raccourcis Steam, la synchronisation cloud, les copies de sauvegarde et les rapports de bug.

Un problème dans un émulateur concerne le projet de cet émulateur.

## Comment ça fonctionne

- **Les liens** s'ouvrent dans ton navigateur, jamais dans la fenêtre de Forkbay. Cette fenêtre ne peut rien charger depuis le web.
- **Les mises à jour de Forkbay** sont refusées si le fichier ne correspond pas à la somme de contrôle SHA-256 publiée par GitHub. Cette somme vient du même endroit que le fichier : elle détecte un téléchargement abîmé, pas un compte GitHub piraté.
- **Les téléchargements d'émulateurs** sont vérifiés de la même façon quand la source publie une somme de contrôle. Sinon, seule la taille est vérifiée, et le message de résultat l'indique. Sous Windows, ils sont aussi analysés avec Microsoft Defender. Un émulateur est un programme qui s'exécute sur ton PC : n'ajoute que des sources de confiance.
- **Les secrets** sont chiffrés pour ton compte sous Windows. Sous Linux, ils restent dans un fichier de réglages lisible par toi seul.
- **Les rapports de bug** retirent les chemins, les noms et les clés, et tu vois tout le texte avant tout envoi.
- **Le programme n'est pas signé** : Windows SmartScreen peut afficher un avertissement au premier lancement.
