# ================================================================
# DOCKERFILE - BUILD ET RUNTIME POUR PRODUCTION
# ================================================================
# Architecture multi-stage pour un build optimisé:
# 1. Stage builder: compile l'application Rust
# 2. Stage python-runtime: prépare l'environnement Python pour les bindings
# 3. Stage runtime: image finale légère avec uniquement les dépendances nécessaires
#
# Pour construire:
#   docker build -t quantization-mvp:latest .
#
# Pour exécuter:
#   docker run -p 8080:8080 --env-file .env quantization-mvp:latest
# ================================================================

# ========== STAGE 1: BUILDER (Rust) ==========
FROM rust:1.78-slim AS builder

# Installer les dépendances système nécessaires
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    libpq-dev \
    libssl-dev \
    pkg-config \
    ca-certificates \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Créer un utilisateur non-root pour la sécurité
RUN useradd -m -u 1000 appuser
WORKDIR /app

# Copier les fichiers de configuration Cargo
COPY Cargo.toml Cargo.lock ./
COPY config ./config

# Créer un dummy main.rs pour le cache Docker
RUN mkdir -p src && \
    echo "fn main() { println!(\"dummy build\"); }" > src/main.rs

# Build dépendances (cache layer)
RUN cargo build --release --bin backend && \
    rm -rf src/main.rs target/release/deps/backend*

# Copier le code source
COPY src ./src

# Build l'application complète
RUN cargo build --release --bin backend

# ========== STAGE 2: PYTHON RUNTIME ==========
FROM python:3.11-slim AS python-runtime

# Installer les dépendances Python nécessaires
WORKDIR /app
COPY --from=builder /app/python/requirements.txt ./
RUN pip install --no-cache-dir --upgrade pip && \
    pip install --no-cache-dir -r requirements.txt

# ========== STAGE 3: RUNTIME FINAL ==========
FROM debian:bookworm-slim

# Installer les dépendances système minimales
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    libpq5 \
    libssl3 \
    libgcc1 \
    libpython3.11 \
    && rm -rf /var/lib/apt/lists/*

# Copier le binaire Rust depuis le stage builder
COPY --from=builder /app/target/release/backend /usr/local/bin/
COPY --from=builder /app/config /app/config

# Copier les dépendances Python
COPY --from=python-runtime /usr/local/lib/python3.11/site-packages /usr/local/lib/python3.11/site-packages
COPY --from=python-runtime /app/python /app/python

# Configurer les permissions
RUN mkdir -p /app/data && \
    chmod -R 755 /app && \
    chown -R 1000:1000 /app

# Créer un utilisateur non-root
RUN useradd -m -u 1000 appuser
USER appuser
WORKDIR /app

# Exposer le port
EXPOSE 8080

# Point d'entrée
CMD ["backend", "--config", "/app/config/production.toml"]