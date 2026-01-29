# BillForge Production Deployment Guide

This guide covers deploying BillForge to a production environment.

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Environment Configuration](#environment-configuration)
3. [Docker Deployment](#docker-deployment)
4. [Kubernetes Deployment](#kubernetes-deployment)
5. [Database Setup](#database-setup)
6. [SSL/TLS Configuration](#ssltls-configuration)
7. [Monitoring & Observability](#monitoring--observability)
8. [Backup & Recovery](#backup--recovery)
9. [Scaling](#scaling)
10. [Security Checklist](#security-checklist)

---

## Prerequisites

- Docker 24+ and Docker Compose v2
- OR Kubernetes 1.28+ cluster
- PostgreSQL 15+ (recommended for production) or SQLite
- S3-compatible storage (AWS S3, MinIO, etc.)
- Domain with SSL certificate
- SMTP server for email notifications

## Environment Configuration

### Backend Environment Variables

Create a `.env.production` file:

```bash
# Server Configuration
ENVIRONMENT=production
BACKEND_HOST=0.0.0.0
BACKEND_PORT=8080

# Database
DATABASE_URL=sqlite:///app/data/billforge.db
TENANT_DB_PATH=/app/data/tenants

# Authentication (REQUIRED - generate secure values)
JWT_SECRET=your-256-bit-secret-key-here
JWT_EXPIRY=24

# CORS - Allowed Origins (comma-separated)
FRONTEND_URL=https://app.yourdomain.com
ALLOWED_ORIGINS=https://app.yourdomain.com,https://admin.yourdomain.com

# Rate Limiting
RATE_LIMIT_RPM=100
RATE_LIMIT_BURST=20

# Storage
LOCAL_STORAGE_PATH=/app/data/files
# For S3 storage:
# S3_BUCKET=billforge-files
# S3_REGION=us-east-1
# AWS_ACCESS_KEY_ID=your-key
# AWS_SECRET_ACCESS_KEY=your-secret

# OCR Provider (tesseract, textract, google_vision)
OCR_PROVIDER=tesseract
# For AWS Textract:
# AWS_TEXTRACT_REGION=us-east-1
# For Google Vision:
# GOOGLE_VISION_CREDENTIALS=/path/to/credentials.json

# Email (SMTP)
SMTP_HOST=smtp.yourdomain.com
SMTP_PORT=587
SMTP_USERNAME=noreply@yourdomain.com
SMTP_PASSWORD=your-smtp-password
SMTP_FROM=BillForge <noreply@yourdomain.com>

# Logging
RUST_LOG=info,billforge_api=info
```

### Frontend Environment Variables

Create `apps/web/.env.production`:

```bash
NEXT_PUBLIC_API_URL=https://api.yourdomain.com
NEXT_PUBLIC_APP_URL=https://app.yourdomain.com
```

### Generating Secure Secrets

```bash
# Generate JWT secret
openssl rand -base64 32

# Generate database encryption key
openssl rand -hex 32
```

## Docker Deployment

### Using Docker Compose (Recommended for single-server)

1. **Create production compose file:**

```yaml
# docker-compose.production.yml
version: '3.8'

services:
  api:
    image: ghcr.io/your-org/billforge-api:latest
    ports:
      - "8080:8080"
    volumes:
      - billforge-data:/app/data
    env_file:
      - .env.production
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 5s
      retries: 3
      start_period: 10s

  web:
    image: ghcr.io/your-org/billforge-web:latest
    ports:
      - "3000:3000"
    environment:
      - NEXT_PUBLIC_API_URL=http://api:8080
    depends_on:
      api:
        condition: service_healthy
    restart: unless-stopped

  nginx:
    image: nginx:alpine
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
      - ./certs:/etc/nginx/certs:ro
    depends_on:
      - api
      - web
    restart: unless-stopped

volumes:
  billforge-data:
```

2. **Deploy:**

```bash
docker compose -f docker-compose.production.yml up -d
```

### Building Images Locally

```bash
# Build backend
docker build -t billforge-api:latest -f docker/Dockerfile.backend .

# Build frontend
docker build -t billforge-web:latest -f docker/Dockerfile.frontend .
```

## Kubernetes Deployment

### Prerequisites

- kubectl configured for your cluster
- Helm 3.x installed
- Container registry access

### Deployment Manifests

Create `k8s/` directory with the following:

**k8s/namespace.yaml:**
```yaml
apiVersion: v1
kind: Namespace
metadata:
  name: billforge
```

**k8s/configmap.yaml:**
```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: billforge-config
  namespace: billforge
data:
  ENVIRONMENT: "production"
  BACKEND_PORT: "8080"
  OCR_PROVIDER: "tesseract"
  RUST_LOG: "info"
```

**k8s/secret.yaml:**
```yaml
apiVersion: v1
kind: Secret
metadata:
  name: billforge-secrets
  namespace: billforge
type: Opaque
stringData:
  JWT_SECRET: "your-jwt-secret"
  DATABASE_URL: "sqlite:///app/data/billforge.db"
```

**k8s/api-deployment.yaml:**
```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: billforge-api
  namespace: billforge
spec:
  replicas: 2
  selector:
    matchLabels:
      app: billforge-api
  template:
    metadata:
      labels:
        app: billforge-api
    spec:
      containers:
      - name: api
        image: ghcr.io/your-org/billforge-api:latest
        ports:
        - containerPort: 8080
        envFrom:
        - configMapRef:
            name: billforge-config
        - secretRef:
            name: billforge-secrets
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "512Mi"
            cpu: "500m"
        livenessProbe:
          httpGet:
            path: /health/live
            port: 8080
          initialDelaySeconds: 10
          periodSeconds: 30
        readinessProbe:
          httpGet:
            path: /health/ready
            port: 8080
          initialDelaySeconds: 5
          periodSeconds: 10
        volumeMounts:
        - name: data
          mountPath: /app/data
      volumes:
      - name: data
        persistentVolumeClaim:
          claimName: billforge-data
```

### Deploy to Kubernetes

```bash
kubectl apply -f k8s/
```

## Database Setup

### SQLite (Default)

SQLite is suitable for small to medium deployments. Data is stored in:
- `/app/data/billforge.db` - Metadata database
- `/app/data/tenants/` - Per-tenant databases

### PostgreSQL (Recommended for Scale)

For high-availability deployments, use PostgreSQL:

1. Set up PostgreSQL cluster
2. Update `DATABASE_URL`:
   ```
   DATABASE_URL=postgres://user:password@host:5432/billforge
   ```

### Database Migrations

Migrations run automatically on startup. For manual migration:

```bash
# Check migration status
docker exec billforge-api /app/billforge-server migrate status

# Run migrations
docker exec billforge-api /app/billforge-server migrate up
```

## SSL/TLS Configuration

### Using Let's Encrypt with Certbot

```bash
# Install certbot
apt-get install certbot

# Get certificate
certbot certonly --standalone -d api.yourdomain.com -d app.yourdomain.com

# Certificate locations
/etc/letsencrypt/live/yourdomain.com/fullchain.pem
/etc/letsencrypt/live/yourdomain.com/privkey.pem
```

### Nginx Configuration

```nginx
server {
    listen 443 ssl http2;
    server_name api.yourdomain.com;

    ssl_certificate /etc/nginx/certs/fullchain.pem;
    ssl_certificate_key /etc/nginx/certs/privkey.pem;

    # Security headers
    add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;
    add_header X-Content-Type-Options nosniff always;
    add_header X-Frame-Options DENY always;
    add_header X-XSS-Protection "1; mode=block" always;

    location / {
        proxy_pass http://api:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

## Monitoring & Observability

### Health Endpoints

- `GET /health` - Basic health check
- `GET /health/live` - Liveness probe (Kubernetes)
- `GET /health/ready` - Readiness probe (Kubernetes)
- `GET /health/detailed` - Detailed component health

### Prometheus Metrics (Coming Soon)

Metrics endpoint will be available at `/metrics`.

### Logging

Logs are output in JSON format for easy parsing:

```json
{
  "timestamp": "2024-01-20T10:30:00Z",
  "level": "INFO",
  "target": "billforge_api",
  "message": "Request completed",
  "method": "POST",
  "uri": "/api/v1/invoices",
  "status": 200,
  "duration_ms": 45
}
```

### Log Aggregation

Recommended tools:
- **Grafana Loki** - Log aggregation
- **Grafana** - Visualization
- **Alertmanager** - Alerting

## Backup & Recovery

### Automated Backups

```bash
#!/bin/bash
# backup.sh - Run daily via cron

BACKUP_DIR=/backups/billforge
DATE=$(date +%Y%m%d_%H%M%S)

# Backup SQLite databases
docker exec billforge-api sqlite3 /app/data/billforge.db ".backup '/tmp/billforge_${DATE}.db'"
docker cp billforge-api:/tmp/billforge_${DATE}.db ${BACKUP_DIR}/

# Backup tenant databases
docker exec billforge-api tar -czf /tmp/tenants_${DATE}.tar.gz /app/data/tenants/
docker cp billforge-api:/tmp/tenants_${DATE}.tar.gz ${BACKUP_DIR}/

# Backup documents
docker exec billforge-api tar -czf /tmp/files_${DATE}.tar.gz /app/data/files/
docker cp billforge-api:/tmp/files_${DATE}.tar.gz ${BACKUP_DIR}/

# Retain 30 days of backups
find ${BACKUP_DIR} -mtime +30 -delete
```

### Recovery Procedure

1. Stop the application
2. Restore database files
3. Restore document files
4. Restart the application
5. Verify data integrity

## Scaling

### Horizontal Scaling

The API is stateless and can be scaled horizontally:

```yaml
# Increase replicas
kubectl scale deployment billforge-api --replicas=4
```

### Database Scaling

For high-traffic deployments:
1. Use PostgreSQL with read replicas
2. Implement connection pooling (PgBouncer)
3. Consider database sharding by tenant

### Caching (Recommended)

Add Redis for caching:
```yaml
redis:
  image: redis:7-alpine
  ports:
    - "6379:6379"
```

## Security Checklist

Before going to production, verify:

- [ ] JWT_SECRET is a strong, random value
- [ ] CORS is configured for specific origins only
- [ ] SSL/TLS is enabled for all traffic
- [ ] Database credentials are secure
- [ ] File upload size limits are configured
- [ ] Rate limiting is enabled
- [ ] Audit logging is enabled
- [ ] Backups are configured and tested
- [ ] Health checks are configured
- [ ] Firewall rules restrict access
- [ ] No debug endpoints exposed
- [ ] Secrets are not in version control

## Troubleshooting

### Common Issues

**API not starting:**
```bash
# Check logs
docker logs billforge-api

# Common causes:
# - Invalid JWT_SECRET
# - Database connection failed
# - Port already in use
```

**Database errors:**
```bash
# Check database file permissions
ls -la /app/data/

# Check disk space
df -h
```

**OCR not working:**
```bash
# Check Tesseract installation
docker exec billforge-api tesseract --version

# Check file permissions
docker exec billforge-api ls -la /app/data/files/
```

### Getting Help

- GitHub Issues: https://github.com/your-org/billforge/issues
- Documentation: https://docs.billforge.com
- Email Support: support@billforge.com
