# Mesmo Ubuntu do CI (release.yml), pra o que funciona aqui funcionar lá.
FROM ubuntu:22.04

ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
      ca-certificates curl wget file git build-essential pkg-config \
      libwebkit2gtk-4.1-dev libxdo-dev libssl-dev \
      libayatana-appindicator3-dev librsvg2-dev \
      libgl1-mesa-dri fonts-jetbrains-mono zsh \
    && curl -fsSL https://deb.nodesource.com/setup_24.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# Usuário com o mesmo UID/GID do host: os arquivos criados no projeto não ficam com dono root.
ARG UID=1000
ARG GID=1000
RUN groupadd -g ${GID} dev && useradd -m -u ${UID} -g ${GID} -s /bin/bash dev \
    && mkdir -p /app/node_modules /app/src-tauri/target \
    && chown -R dev:dev /app \
    && install -d -m 0700 -o dev -g dev /tmp/runtime

USER dev
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal \
    && mkdir -p /home/dev/.cargo/registry

ENV PATH=/home/dev/.cargo/bin:$PATH \
    SHELL=/bin/bash \
    GSETTINGS_BACKEND=memory
WORKDIR /app
