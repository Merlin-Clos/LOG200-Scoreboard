# Image unique des bacs a sable : l'arbitre et les joueurs (jars des equipes ou bots de test).
# Construite par sandbox/build.sh, qui fournit nos binaires et les scripts d'entree.
FROM eclipse-temurin:24-jre@sha256:8cb2387a28af84cf0db0948d9c67d4480192f4e567027a3963f145d218e8b4f2

RUN apt-get update \
    && apt-get install -y --no-install-recommends socat \
    && rm -rf /var/lib/apt/lists/*

COPY isolation-referee isolation-bot referee-entry.sh player-entry.sh /usr/local/bin/
