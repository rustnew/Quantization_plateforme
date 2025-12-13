#!/bin/bash
# ================================================================
# SCRIPT D'INITIALISATION - ENVIRONNEMENT DE DÉVELOPPEMENT
# ================================================================
# Ce script configure l'environnement de développement complet:
# 1. Crée le fichier .env à partir du template
# 2. Génère des clés de chiffrement sécurisées
# 3. Initialise les migrations de base de données
# 4. Crée le bucket MinIO
#
# Pour exécuter:
#   chmod +x scripts/setup_env.sh
#   ./scripts/setup_env.sh
# ================================================================

set -e  # Arrêter à la première erreur

echo "🚀 Démarrage de l'initialisation de l'environnement..."

# 1. Créer le fichier .env si inexistant
if [ ! -f .env ]; then
    echo "Création du fichier .env à partir du template..."
    cp .env.example .env
    
    # Générer des clés sécurisées
    echo " Génération des clés de sécurité..."
    
    # Générer une clé de chiffrement de 32 bytes (256 bits)
    ENCRYPTION_KEY=$(openssl rand -hex 32)
    sed -i "s/STORAGE_ENCRYPTION_KEY=.*/STORAGE_ENCRYPTION_KEY=$ENCRYPTION_KEY/" .env
    
    # Générer un secret JWT sécurisé
    JWT_SECRET=$(openssl rand -hex 32)
    sed -i "s/JWT_SECRET=.*/JWT_SECRET=$JWT_SECRET/" .env
    
    echo "✅ Fichier .env créé avec des clés sécurisées."
else
    echo "ℹ️  Le fichier .env existe déjà. Aucune modification apportée."
fi

# 2. Démarrer les services Docker
echo "🐳 Démarrage des services Docker..."
docker-compose up -d --wait

# 3. Vérifier que les services sont prêts
echo "🔍 Vérification de l'état des services..."
until docker-compose exec db pg_isready -U quant_user -d quant_dev; do
    echo "⏳ PostgreSQL n'est pas encore prêt. Attente..."
    sleep 2
done

echo "✅ PostgreSQL est prêt."

# 4. Appliquer les migrations
echo "📊 Application des migrations de base de données..."
docker-compose run --rm app cargo sqlx migrate run

# 5. Créer le bucket MinIO
echo "💾 Création du bucket MinIO..."
docker-compose exec minio mc alias set local http://localhost:9000 minioadmin minioadmin
docker-compose exec minio mc mb local/quant-dev
docker-compose exec minio mc policy set public local/quant-dev

echo "✅ Bucket MinIO créé et configuré."

# 6. Vérification finale
echo ""
echo "🎉 INITIALISATION TERMINÉE AVEC SUCCÈS!"
echo ""
echo "Services disponibles:"
echo "  🌐 Application: http://localhost:8080"
echo "  🗃️  Base de données: postgres://quant_user:quant_pass@localhost:5432/quant_dev"
echo "  🔍 pgAdmin: http://localhost:8081 (admin@quantmvp.com / admin123)"
echo "  📦 MinIO Console: http://localhost:9001 (minioadmin / minioadmin)"
echo ""
echo "Commandes utiles:"
echo "  docker-compose logs -f app    # Voir les logs de l'application"
echo "  docker-compose exec app bash  # Accéder au shell du conteneur"
echo "  cargo run                     # Exécuter en local (hors Docker)"
echo ""
echo "🚀 Tu peux maintenant commencer à développer!"