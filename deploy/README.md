# Déployer le ladder

Le ladder et sa page tournent avec un `docker compose up -d`. Chaque jar joue dans un bac à sable gVisor sans réseau (voir « Isolation des joueurs » dans `docs/formats.md`).

## Prérequis

- Une machine avec Docker : Linux de préférence, Docker Desktop fonctionne aussi.
- Au moins 4 cœurs : chaque partie réserve un cœur par joueur (`cpuset_red`, `cpuset_black` dans `ladder.toml`).
- Le service `ladder` tourne en mode privilégié, parce qu'il héberge le Docker interne et gVisor qui isolent les jars. Le code des équipes, lui, n'a ni privilège ni réseau.

## Installation

1. Télécharger `isolation-scoreboard-deploy.zip` depuis la [dernière release](https://github.com/VianAtETS/LOG200-Scoreboard/releases/latest) et l'extraire.
2. Créer la config : `cp ladder.example.toml ladder.toml`, puis régler la session, le temps par coup et les équipes.
3. Déposer chaque jar en `jars/<équipe>/current.jar`, avec une entrée `[[team]]` correspondante dans `ladder.toml`.
4. Lancer `docker compose up -d`. La page est sur `http://<machine>:8080/` (autre port : `WEB_PORT=80 docker compose up -d`).

## Au quotidien

| Pour… | Faire |
| --- | --- |
| Suivre les parties | `docker compose logs -f ladder` |
| Remplacer le jar d'une équipe | copier le nouveau jar à côté puis `mv` sur `current.jar` ; il sert dès la partie suivante, et son nouveau hash compte comme une nouvelle soumission |
| Ajouter une équipe | ajouter le jar et l'entrée `[[team]]`, puis `docker compose restart ladder` |
| Mettre à jour | `docker compose pull && docker compose up -d` |
| Fixer une version | `SCOREBOARD_TAG=v0.2.0 docker compose up -d` |
| Arrêter | `docker compose down` (les parties restent dans `data/`) |
| Sauvegarder | copier `data/games.jsonl` |

`mv` plutôt qu'une copie par-dessus `current.jar` : une partie en cours garde l'ancien fichier au lieu de lire un jar à moitié écrit.

Les journaux des joueurs vont dans `data/logs/`. La page ne les sert pas.

## Nom de domaine et HTTPS

Hors du périmètre de ce compose : placer le reverse proxy de votre choix (Caddy, Nginx…) devant le port 8080.
