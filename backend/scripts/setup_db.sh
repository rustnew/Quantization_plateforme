#!/bin/bash
# ================================================================
# SCRIPT DE CONFIGURATION POSTGRESQL
# ================================================================
# Ce script configure PostgreSQL avec les extensions nécessaires
# et crée les utilisateurs/databases pour tous les environnements.
#
# À exécuter une fois après le premier démarrage de PostgreSQL.
# ================================================================

# Activer les extensions nécessaires
echo " Activation des extensions..."
$PSQL -d quant_dev -c "CREATE EXTENSION IF NOT EXISTS pgcrypto;"
$PSQL -d quant_dev -c "CREATE EXTENSION IF NOT EXISTS pg_stat_statements;"
$PSQL -d quant_staging -c "CREATE EXTENSION IF NOT EXISTS pgcrypto;"
$PSQL -d quant_staging -c "CREATE EXTENSION IF NOT EXISTS pg_stat_statements;"
$PSQL -d quant_prod -c "CREATE EXTENSION IF NOT EXISTS pgcrypto;"
$PSQL -d quant_prod -c "CREATE EXTENSION IF NOT EXISTS pg_stat_statements;"

# Optimiser les paramètres pour le workload
echo "⚡ Optimisation des paramètres PostgreSQL..."

$PSQL -c "ALTER SYSTEM SET shared_buffers = '256MB';"
$PSQL -c "ALTER SYSTEM SET effective_cache_size = '768MB';"
$PSQL -c "ALTER SYSTEM SET work_mem = '4MB';"
$PSQL -c "ALTER SYSTEM SET maintenance_work_mem = '64MB';"
$PSQL -c "ALTER SYSTEM SET max_connections = '100';"
$PSQL -c "ALTER SYSTEM SET checkpoint_completion_target = '0.9';"
$PSQL -c "ALTER SYSTEM SET wal_buffers = '8MB';"
$PSQL -c "ALTER SYSTEM SET default_statistics_target = '100';"
$PSQL -c "ALTER SYSTEM SET random_page_cost = '1.1';"
$PSQL -c "ALTER SYSTEM SET effective_io_concurrency = '200';"
$PSQL -c "ALTER SYSTEM SET max_worker_processes = '8';"
$PSQL -c "ALTER SYSTEM SET max_parallel_workers_per_gather = '4';"
$PSQL -c "ALTER SYSTEM SET max_parallel_workers = '8';"
$PSQL -c "ALTER SYSTEM SET max_parallel_maintenance_workers = '4';"

echo "✅ Configuration PostgreSQL terminée."
echo "💡 Redémarre PostgreSQL pour appliquer les paramètres: docker-compose restart db"