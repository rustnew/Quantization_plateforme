-- 1. users: table ultra-simple avec juste nom, email, mot de passe
-- 2. jobs: suivi des quantifications
-- 3. subscriptions: gestion des abonnements payants
-- 4. payments: suivi des paiements
-- 5. quantization_reports: rapports de qualité pour preuve de valeur


-- Table des utilisateurs MINIMALISTE (comme demandé)
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) NOT NULL UNIQUE,
    password_hash VARCHAR(255),  -- NULL si authentification sociale
    auth_provider VARCHAR(50),   -- 'email', 'google', 'github', NULL
    auth_provider_id VARCHAR(255), -- ID du provider si auth sociale
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    is_active BOOLEAN NOT NULL DEFAULT true
);

-- Index pour connexion rapide
CREATE INDEX idx_users_email ON users(email);
CREATE INDEX idx_users_auth ON users(auth_provider, auth_provider_id);

-- Table des jobs de quantification
CREATE TABLE jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    model_name VARCHAR(255) NOT NULL,
    original_size_bytes BIGINT NOT NULL,
    quantized_size_bytes BIGINT,
    quantization_method VARCHAR(50) NOT NULL DEFAULT 'int8' CHECK (quantization_method IN ('int8', 'int4', 'gptq', 'awq')),
    status VARCHAR(20) NOT NULL DEFAULT 'queued' CHECK (status IN ('queued', 'processing', 'completed', 'failed')),
    error_message TEXT,
    reduction_percent FLOAT,
    download_url TEXT,  -- URL sécurisée pour téléchargement
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Index pour performance
CREATE INDEX idx_jobs_user ON jobs(user_id);
CREATE INDEX idx_jobs_status ON jobs(status);
CREATE INDEX idx_jobs_created_at ON jobs(created_at);

-- Table des abonnements (modèle économique simple)
CREATE TABLE subscriptions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    plan_name VARCHAR(50) NOT NULL DEFAULT 'free' CHECK (plan_name IN ('free', 'starter', 'pro')),
    monthly_credits INTEGER NOT NULL DEFAULT 1,  -- Nombre de quantifications gratuites/mois
    credits_used INTEGER NOT NULL DEFAULT 0,    -- Crédits utilisés ce mois
    stripe_customer_id VARCHAR(255),
    stripe_subscription_id VARCHAR(255) UNIQUE,
    is_active BOOLEAN NOT NULL DEFAULT true,
    current_period_end TIMESTAMP WITH TIME ZONE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Index pour gestion abonnements
CREATE INDEX idx_subscriptions_user ON subscriptions(user_id);
CREATE INDEX idx_subscriptions_stripe ON subscriptions(stripe_subscription_id);
CREATE INDEX idx_subscriptions_active ON subscriptions(is_active);

-- Table des paiements
CREATE TABLE payments (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    amount DECIMAL(10,2) NOT NULL,
    currency VARCHAR(3) NOT NULL DEFAULT 'EUR',
    description VARCHAR(255) NOT NULL,
    stripe_payment_id VARCHAR(255) UNIQUE,
    status VARCHAR(20) NOT NULL DEFAULT 'succeeded' CHECK (status IN ('succeeded', 'failed', 'refunded')),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Index pour analytics
CREATE INDEX idx_payments_user ON payments(user_id);
CREATE INDEX idx_payments_status ON payments(status);
CREATE INDEX idx_payments_created_at ON payments(created_at);

-- Table des rapports de quantification (pour preuve de valeur)
CREATE TABLE quantization_reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    original_perplexity FLOAT,
    quantized_perplexity FLOAT,
    quality_loss_percent FLOAT,
    latency_improvement_percent FLOAT,
    cost_savings_percent FLOAT,  -- Économie estimée sur l'inférence
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- Index pour rapports
CREATE INDEX idx_reports_job ON quantization_reports(job_id);

-- Trigger pour mise à jour automatique de updated_at
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Appliquer le trigger aux tables qui en ont besoin
CREATE TRIGGER update_users_updated_at BEFORE UPDATE ON users FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_jobs_updated_at BEFORE UPDATE ON jobs FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_subscriptions_updated_at BEFORE UPDATE ON subscriptions FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

-- Création de l'utilisateur admin par défaut
INSERT INTO users (name, email, password_hash, auth_provider, is_active)
VALUES (
    'Admin',
    'admin@quantmvp.com',
    '$argon2id$v=19$m=19456,t=2,p=1$cmFuZG9tc2FsdA$/JZP6hY5KqWx7qLXcR5v0Z9yJ6X2H1K8F3G7D9E2B5C8A0',
    'email',
    true
) ON CONFLICT (email) DO NOTHING;

-- Création abonnement admin (illimité)
INSERT INTO subscriptions (user_id, plan_name, monthly_credits, credits_used, is_active, current_period_end)
SELECT id, 'pro', 1000, 0, true, NOW() + INTERVAL '1 year'
FROM users WHERE email = 'admin@quantmvp.com'
ON CONFLICT (user_id) DO NOTHING;