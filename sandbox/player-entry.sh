#!/bin/sh
# Dans le conteneur d'un joueur. Le jar se connecte a localhost:PORT comme sur le serveur officiel ; un
# relais lance en root transmet a la socket de sa couleur (une seule connexion), puis le jar tourne en
# nobody, sans capacites, et ne peut ni tuer le relais ni ouvrir la socket directement.
# Usage : player-entry.sh PORT COMMANDE...
set -eu
port=$1
shift
socat "TCP-LISTEN:$port,bind=127.0.0.1" UNIX-CONNECT:/chan/sock &
hex=$(printf '%04X' "$port")
for _ in $(seq 100); do
    grep -q ":$hex 00000000:0000 0A" /proc/net/tcp 2>/dev/null && break
    sleep 0.02
done
exec setpriv --reuid=65534 --regid=65534 --clear-groups --inh-caps=-all "$@"
