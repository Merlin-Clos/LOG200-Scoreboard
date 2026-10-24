# Serveur de jeu Isolation (application Qt6 Windows x86-64) sous Wine.
# L'affichage est un X virtuel (Xvfb) exposé dans le navigateur par noVNC sur le port 6080.
FROM debian:trixie-slim

# Wine de WineHQ, branche devel : Qt 6.10 appelle mscms.WcsCreateIccProfile, implémentée
# seulement depuis Wine 11.15 (absente de Debian et de la branche stable 11.0). Version épinglée (https://dl.winehq.org/wine-builds/debian/)
ARG WINE_BRANCH=devel
ARG WINE_VERSION=11.20~trixie-1

RUN dpkg --add-architecture i386 \
    && apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get install -y --no-install-recommends ca-certificates curl gnupg \
    && mkdir -pm755 /etc/apt/keyrings \
    && curl -fsSL https://dl.winehq.org/wine-builds/winehq.key \
        | gpg --dearmor -o /etc/apt/keyrings/winehq-archive.key \
    && curl -fsSL -o /etc/apt/sources.list.d/winehq-trixie.sources \
        https://dl.winehq.org/wine-builds/debian/dists/trixie/winehq-trixie.sources \
    && apt-get update \
    && apt-get install -y --install-recommends \
        "winehq-${WINE_BRANCH}=${WINE_VERSION}" \
        "wine-${WINE_BRANCH}=${WINE_VERSION}" \
        "wine-${WINE_BRANCH}-amd64=${WINE_VERSION}" \
        "wine-${WINE_BRANCH}-i386=${WINE_VERSION}" \
    && apt-get install -y --no-install-recommends \
        xvfb \
        xauth \
        x11vnc \
        novnc \
        websockify \
        fluxbox \
        fonts-dejavu \
        fonts-liberation \
        procps \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

# Page d'accueil de noVNC : connexion automatique, mise à l'échelle de l'écran
RUN printf '%s\n' '<!doctype html>' \
        '<meta http-equiv="refresh" content="0; url=vnc.html?autoconnect=1&resize=scale&reconnect=1">' \
        > /usr/share/novnc/index.html

ENV DISPLAY=:99 \
    WINEPREFIX=/home/wine/.wine \
    WINEDEBUG=-all \
    WINEDLLOVERRIDES="mscoree,mshtml=" \
    QT_QPA_PLATFORM=windows:fontengine=freetype \
    QT_OPENGL=software

RUN useradd --create-home --shell /bin/bash wine
USER wine

# Préfixe Wine créé au build : le serveur démarre rapidement
RUN xvfb-run -a wineboot --init && wineserver --wait

COPY --chmod=0755 server-entrypoint.sh /usr/local/bin/server-entrypoint.sh

EXPOSE 6080 8888
ENTRYPOINT ["/usr/local/bin/server-entrypoint.sh"]
