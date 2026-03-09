#!/bin/bash
# Database backup automation script
#
# Usage:
#   ./scripts/backup.sh [tenant_slug]
#
# If tenant_slug is provided, backs up only that tenant's database
# Otherwise, backs up all tenant databases

set -euo pipefail

# Configuration
BACKUP_DIR="${BACKUP_DIR:-/var/lib/billforge/backups}"
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
RETENTION_DAYS="${RETENTION_DAYS:-30}"
S3_BUCKET="${S3_BUCKET:-billforge-backups}"
DATABASE_HOST="${DATABASE_HOST:-localhost}"
DATABASE_PORT="${DATABASE_PORT:-5432}"
DATABASE_USER="${DATABASE_USER:-billforge}"

# Ensure backup directory exists
mkdir -p "$BACKUP_DIR"

log() {
    echo "[$(date +'%Y-%m-%d %H:%M:%S')] $1"
}

backup_control_plane() {
    log "Backing up control plane database..."
    local backup_file="$BACKUP_DIR/control_plane_${TIMESTAMP}.sql.gz"

    docker exec billforge-postgres-prod pg_dumpall -U "$DATABASE_USER" --database billforge_control | \
        gzip > "$backup_file"

    log "✓ Control plane backup: $backup_file"

    # Upload to S3
    if command -v aws &> /dev/null; then
        aws s3 cp "$backup_file" "s3://$S3_BUCKET/control_plane/" --storage-class STANDARD_IA
        log "✓ Uploaded to S3: s3://$S3_BUCKET/control_plane/"
    fi
}

backup_tenant() {
    local tenant_slug="$1"
    local db_name="billforge_tenant_${tenant_slug//-/_}"
    local backup_file="$BACKUP_DIR/tenant_${tenant_slug}_${TIMESTAMP}.sql.gz"

    log "Backing up tenant database: $tenant_slug"

    # Check if database exists
    if ! docker exec billforge-postgres-prod psql -U "$DATABASE_USER" -lqt | cut -d \| -f 1 | grep -qw "$db_name"; then
        log "⚠ Database $db_name not found, skipping"
        return 1
    fi

    # Create backup
    docker exec billforge-postgres-prod pg_dump -U "$DATABASE_USER" -d "$db_name" | \
        gzip > "$backup_file"

    log "✓ Tenant backup: $backup_file"

    # Upload to S3
    if command -v aws &> /dev/null; then
        aws s3 cp "$backup_file" "s3://$S3_BUCKET/tenants/${tenant_slug}/" --storage-class STANDARD_IA
        log "✓ Uploaded to S3: s3://$S3_BUCKET/tenants/${tenant_slug}/"
    fi

    # Backup document storage (MinIO)
    if docker ps | grep -q billforge-minio; then
        local docs_backup="$BACKUP_DIR/tenant_${tenant_slug}_docs_${TIMESTAMP}.tar.gz"
        docker exec billforge-minio mc mirror local/${tenant_slug} /tmp/backup_docs && \
        docker cp billforge-minio:/tmp/backup_docs - | gzip > "$docs_backup"

        if command -v aws &> /dev/null; then
            aws s3 cp "$docs_backup" "s3://$S3_BUCKET/tenants/${tenant_slug}/documents/" --storage-class STANDARD_IA
            log "✓ Uploaded documents to S3"
        fi
    fi
}

backup_all_tenants() {
    log "Backing up all tenant databases..."

    # Get list of tenant databases
    local databases=$(docker exec billforge-postgres-prod psql -U "$DATABASE_USER" -lqt | \
        cut -d \| -f 1 | \
        grep "billforge_tenant_" | \
        xargs)

    for db_name in $databases; do
        local tenant_slug=$(echo "$db_name" | sed 's/billforge_tenant_//' | sed 's/_/-/g')
        backup_tenant "$tenant_slug" || true
    done
}

cleanup_old_backups() {
    log "Cleaning up backups older than $RETENTION_DAYS days..."

    find "$BACKUP_DIR" -name "*.sql.gz" -mtime +$RETENTION_DAYS -delete
    find "$BACKUP_DIR" -name "*.tar.gz" -mtime +$RETENTION_DAYS -delete

    log "✓ Cleanup complete"
}

verify_backup() {
    local backup_file="$1"

    if [ ! -f "$backup_file" ]; then
        log "✗ Backup file not found: $backup_file"
        return 1
    fi

    # Check if file is valid gzip
    if ! gzip -t "$backup_file" 2>/dev/null; then
        log "✗ Backup file is corrupted: $backup_file"
        return 1
    fi

    # Check minimum size (should be at least 1KB)
    local size=$(stat -f%z "$backup_file" 2>/dev/null || stat -c%s "$backup_file")
    if [ "$size" -lt 1024 ]; then
        log "✗ Backup file too small: $backup_file ($size bytes)"
        return 1
    fi

    log "✓ Backup verified: $backup_file"
    return 0
}

send_notification() {
    local status="$1"
    local message="$2"

    # Send to Slack webhook if configured
    if [ -n "${SLACK_WEBHOOK_URL:-}" ]; then
        curl -X POST "$SLACK_WEBHOOK_URL" \
            -H 'Content-type: application/json' \
            -d "{\"text\":\"BillForge Backup: ${status}\", \"attachments\":[{\"text\":\"${message}\", \"color\":\"$([ "$status" = "SUCCESS" ] && echo "good" || echo "danger")\"}]}" \
            &>/dev/null || true
    fi

    # Send email if configured
    if [ -n "${NOTIFICATION_EMAIL:-}" ]; then
        echo "$message" | mail -s "BillForge Backup: ${status}" "$NOTIFICATION_EMAIL" || true
    fi
}

# Main execution
main() {
    log "========================================="
    log "BillForge Backup Script Started"
    log "========================================="

    local backup_status="SUCCESS"
    local backup_message=""

    # Backup control plane
    if ! backup_control_plane; then
        backup_status="FAILED"
        backup_message+="Control plane backup failed\n"
    fi

    # Backup tenant(s)
    if [ $# -eq 1 ]; then
        if ! backup_tenant "$1"; then
            backup_status="FAILED"
            backup_message+="Tenant $1 backup failed\n"
        fi
    else
        if ! backup_all_tenants; then
            backup_status="PARTIAL"
            backup_message+="Some tenant backups failed\n"
        fi
    fi

    # Cleanup old backups
    cleanup_old_backups

    # Verify recent backups
    local latest_backup=$(ls -t "$BACKUP_DIR"/*.sql.gz | head -1)
    if ! verify_backup "$latest_backup"; then
        backup_status="FAILED"
        backup_message+="Backup verification failed\n"
    fi

    # Send notification
    backup_message+="Backup completed at $(date)"
    send_notification "$backup_status" "$backup_message"

    log "========================================="
    log "BillForge Backup Script Completed: $backup_status"
    log "========================================="

    [ "$backup_status" = "SUCCESS" ] || exit 1
}

main "$@"
