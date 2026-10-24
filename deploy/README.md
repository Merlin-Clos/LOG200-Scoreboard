# Déployer le ladder

Le ladder et sa page tournent avec un `docker compose up -d`. Chaque jar joue dans un bac à sable gVisor sans réseau (voir « Isolation des joueurs » dans `docs/formats.md`).

## Prérequis

- Une machine avec Docker : Linux de préférence, Docker Desktop fonctionne aussi.
- Au moins 4 cœurs : chaque partie réserve un cœur par joueur (`cpuset_red`, `cpuset_black` dans `ladder.toml`).
- Le service `ladder` tourne en mode privilégié, parce qu'il héberge le Docker interne et gVisor qui isolent les jars. Le code des équipes, lui, n'a ni privilège ni réseau.

## Installation

1. Télécharger `isolation-scoreboard-deploy.zip` depuis la [dernière release](https://github.com/VianAtETS/LOG200-Scoreboard/releases/latest) et l'extraire.
2. Créer la config : `cp ladder.example.toml ladder.toml`, puis régler au moins la session et le temps par coup.
3. Lancer `docker compose up -d`. La page est sur `http://<machine>:8080/` (autre port : `WEB_PORT=80 docker compose up -d`).

## Inscrire les équipes

Les équipes soumettent elles-mêmes leur jar (formulaire de la page, `curl` ou CI ; voir « Soumission des jars » dans `docs/formats.md`). Pour chaque équipe :

```sh
docker compose exec ladder isolation-ladder add-team eq07
```

La commande affiche le token de l'équipe, une seule fois : le lui transmettre en privé. Une équipe entre au classement dès que son premier jar a passé le smoke test ; inutile de redémarrer.

## Au quotidien

| Pour… | Faire |
| --- | --- |
| Suivre les parties | `docker compose logs -f ladder` |
| Voir les soumissions | `jars/submissions.jsonl` (verdict et raison de chaque refus) |
| Retirer une équipe | supprimer sa ligne dans `jars/teams.toml` et son dossier `jars/<équipe>/` |
| Mettre à jour | `docker compose pull && docker compose up -d` |
| Fixer une version | `SCOREBOARD_TAG=v0.2.0 docker compose up -d` |
| Arrêter | `docker compose down` (les parties restent dans `data/`) |
| Sauvegarder | copier `data/games.jsonl` |

Les journaux des joueurs vont dans `data/logs/`, les jars et les tokens dans `jars/`. La page ne sert ni l'un ni l'autre.

## Nom de domaine et HTTPS

Hors du périmètre de ce compose : placer le reverse proxy de votre choix (Caddy, Nginx…) devant le port 8080.
