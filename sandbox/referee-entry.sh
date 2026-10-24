#!/bin/sh
# Dans le conteneur de l'arbitre. L'arbitre ecoute sur 127.0.0.1:PORT ; chaque couleur n'arrive que
# par sa socket Unix, et socat sans fork n'accepte qu'une connexion : un jar ne peut pas en ouvrir une
# seconde pour se faire passer pour l'autre couleur.
# Usage : referee-entry.sh PORT COMMANDE...
set -eu
port=$1
shift
for side in red black; do
    socat "UNIX-LISTEN:/chan/$side/sock,mode=600" "TCP:127.0.0.1:$port" &
done
exec "$@"
