# Multi-Region Deployment Guide

## Overview

This guide covers deploying BillForge across multiple AWS regions for high availability and disaster recovery.

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                     PRIMARY REGION (us-east-1)               │
│                                                               │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │  API (3 pods) │  │ Worker (4)   │  │  PostgreSQL  │      │
│  │  + HPA       │  │  + HPA       │  │  (Primary)   │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
│                                              │                │
│  ┌──────────────┐  ┌──────────────┐          │                │
│  │  Redis       │  │  MinIO       │          │                │
│  │  (Cluster)   │  │  (Cluster)   │          │                │
│  └──────────────┘  └──────────────┘          │                │
│                                              │                │
└──────────────────────────────────────────────┼────────────────┘
                                               │
                                    Cross-Region Replication
                                               │
┌──────────────────────────────────────────────┼────────────────┐
│                     DR REGION (us-west-2)    │                │
│                                              │                │
│  ┌──────────────┐  ┌──────────────┐          ▼                │
│  │  API (1 pod) │  │ Worker (1)   │  ┌──────────────┐        │
│  │  Standby     │  │  Standby     │  │  PostgreSQL  │        │
│  └──────────────┘  └──────────────┘  │  (Replica)   │        │
│                                       └──────────────┘        │
│  ┌──────────────┐  ┌──────────────┐                           │
│  │  Redis       │  │  MinIO       │                           │
│  │  (Replica)   │  │  (Replica)   │                           │
│  └──────────────┘  └──────────────┘                           │
│                                                               │
└───────────────────────────────────────────────────────────────┘

                          Route 53
                    (Health-Based Routing)
                               │
                    ┌──────────┴──────────┐
                    ▼                     ▼
              us-east-1.elb         us-west-2.elb
             (Primary Active)      (DR Standby)
```

## Prerequisites

- AWS CLI configured with credentials
- kubectl configured for both clusters
- Helm 3.x installed
- Route 53 hosted zone for domain
- ACM certificates in both regions

## Setup Instructions

### 1. Create EKS Clusters

**Primary Region (us-east-1):**
```bash
# Create VPC
aws cloudformation create-stack \
  --stack-name billforge-vpc-primary \
  --template-url https://s3.us-west-2.amazonaws.com/amazon-eks/cloudformation/2020-10-29/amazon-eks-vpc-private-subnets.yaml \
  --region us-east-1

# Create EKS cluster
eksctl create cluster \
  --name billforge-primary \
  --region us-east-1 \
  --nodegroup-name workers \
  --node-type m5.xlarge \
  --nodes 5 \
  --nodes-min 3 \
  --nodes-max 10 \
  --managed \
  --asg-access

# Configure kubectl
aws eks update-kubeconfig --name billforge-primary --region us-east-1
```

**DR Region (us-west-2):**
```bash
# Repeat above commands with region=us-west-2
eksctl create cluster \
  --name billforge-dr \
  --region us-west-2 \
  --nodegroup-name workers \
  --node-type m5.xlarge \
  --nodes 3 \
  --nodes-min 1 \
  --nodes-max 5 \
  --managed \
  --asg-access

aws eks update-kubeconfig --name billforge-dr --region us-west-2
```

### 2. Install PostgreSQL with Cross-Region Replication

**Primary Region:**
```bash
kubectl config use-context billforge-primary

# Install PostgreSQL operator
helm repo add bitnami https://charts.bitnami.com/bitnami
helm install postgres-operator bitnami/postgresql-operator

# Create primary PostgreSQL cluster
cat <<EOF | kubectl apply -f -
apiVersion: postgresql.k8s.enterprisedb.io/v1
kind: Cluster
metadata:
  name: billforge-postgres-primary
spec:
  instances: 3
  primaryUpdateStrategy: unsupervised

  storage:
    storageClass: gp3
    size: 100Gi

  config:
    archiveMode: true
    archiveTimeout: 60

  backup:
    barmanObjectStore:
      destinationPath: s3://billforge-postgres-backups/primary
      s3Credentials:
        accessKeyId:
          name: aws-creds
          key: ACCESS_KEY_ID
        secretAccessKey:
          name: aws-creds
          key: ACCESS_SECRET_KEY

  replicas:
    - name: dr-replica
      enabled: true
      source: primary
EOF
```

**DR Region:**
```bash
kubectl config use-context billforge-dr

# Create replica PostgreSQL cluster
cat <<EOF | kubectl apply -f -
apiVersion: postgresql.k8s.enterprisedb.io/v1
kind: Cluster
metadata:
  name: billforge-postgres-dr
spec:
  instances: 2
  primaryUpdateStrategy: unsupervised

  storage:
    storageClass: gp3
    size: 100Gi

  replica:
    enabled: true
    source: billforge-postgres-primary

  externalClusters:
    - name: billforge-postgres-primary
      connectionParameters:
        host: billforge-postgres-primary.us-east-1.amazonaws.com
        port: 5432
        user: replication
        dbname: postgres
      sslKey:
        name: postgres-replication-ssl
        key: tls.key
      sslCert:
        name: postgres-replication-ssl
        key: tls.crt
      sslRootCert:
        name: postgres-replication-ssl
        key: ca.crt
EOF
```

### 3. Configure Redis Cross-Region Replication

**Primary Region:**
```bash
helm install redis bitnami/redis-cluster \
  --set cluster.nodes=6 \
  --set cluster.replicas=true \
  --set persistence.enabled=true \
  --set persistence.size=10Gi
```

**DR Region:**
```bash
# Redis cluster with replica mode
helm install redis bitnami/redis-cluster \
  --set cluster.nodes=3 \
  --set cluster.replicas=true \
  --set persistence.enabled=true
```

### 4. Deploy Application

**Both Regions:**
```bash
# Create namespace
kubectl apply -f k8s/namespace.yaml

# Deploy application
kubectl apply -f k8s/deployment.yaml

# In primary: scale up
kubectl scale deployment billforge-api --replicas=3 -n billforge
kubectl scale deployment billforge-worker --replicas=4 -n billforge

# In DR: scale down (standby mode)
kubectl scale deployment billforge-api --replicas=1 -n billforge
kubectl scale deployment billforge-worker --replicas=1 -n billforge
```

### 5. Configure Route 53 Health Checks

```bash
# Create health checks
aws route53 create-health-check \
  --caller-reference primary-api \
  --health-check-config '
{
  "IPAddress": "PRIMARY_ELB_IP",
  "Port": 80,
  "Type": "HTTP",
  "ResourcePath": "/health",
  "RequestInterval": 30,
  "FailureThreshold": 3
}
'

# Create latency-based routing policy
cat <<EOF > route53-policy.json
{
  "Comment": "BillForge Multi-Region Routing",
  "Changes": [
    {
      "Action": "CREATE",
      "ResourceRecordSet": {
        "Name": "api.billforge.io",
        "Type": "A",
        "AliasTarget": {
          "HostedZoneId": "PRIMARY_HOSTED_ZONE_ID",
          "DNSName": "PRIMARY_ELB_DNS_NAME",
          "EvaluateTargetHealth": true
        },
        "Region": "us-east-1",
        "SetIdentifier": "primary"
      }
    },
    {
      "Action": "CREATE",
      "ResourceRecordSet": {
        "Name": "api.billforge.io",
        "Type": "A",
        "AliasTarget": {
          "HostedZoneId": "DR_HOSTED_ZONE_ID",
          "DNSName": "DR_ELB_DNS_NAME",
          "EvaluateTargetHealth": true
        },
        "Region": "us-west-2",
        "SetIdentifier": "dr",
        "Failover": "SECONDARY"
      }
    }
  ]
}
EOF

aws route53 change-resource-record-sets \
  --hosted-zone-id YOUR_HOSTED_ZONE_ID \
  --change-batch file://route53-policy.json
```

### 6. Configure Database Backup Replication

```bash
# S3 cross-region replication
aws s3api put-bucket-replication \
  --bucket billforge-backups \
  --replication-configuration file://s3-replication.json

# s3-replication.json
{
  "Role": "arn:aws:iam::ACCOUNT_ID:role/s3-replication-role",
  "Rules": [
    {
      "Status": "Enabled",
      "Priority": 1,
      "DeleteMarkerReplication": { "Status": "Disabled" },
      "Filter": {},
      "Destination": {
        "Bucket": "arn:aws:s3:::billforge-backups-dr",
        "StorageClass": "STANDARD_IA"
      }
    }
  ]
}
```

## Failover Procedures

### Automatic Failover (RTO: 5-10 minutes)

1. Route 53 detects primary region failure (3 consecutive health check failures)
2. Traffic automatically routed to DR region
3. DR API pods scale up via HPA
4. PostgreSQL replica promoted to primary

### Manual Failover

```bash
# 1. Verify primary is down
aws route53 get-health-check-status --health-check-id PRIMARY_HC_ID

# 2. Scale up DR
kubectl config use-context billforge-dr
kubectl scale deployment billforge-api --replicas=3 -n billforge
kubectl scale deployment billforge-worker --replicas=4 -n billforge

# 3. Promote PostgreSQL replica
kubectl exec -it billforge-postgres-dr-1 -- patronictl promote

# 4. Update DNS to point to DR
aws route53 change-resource-record-sets \
  --hosted-zone-id YOUR_HOSTED_ZONE_ID \
  --change-batch file://failover-dns.json

# 5. Verify DR is serving traffic
curl https://api.billforge.io/health
```

### Failback Procedure

```bash
# 1. Restore primary region infrastructure
# 2. Sync data from DR to primary
aws s3 sync s3://billforge-backups-dr s3://billforge-backups

# 3. Re-establish PostgreSQL replication
kubectl exec -it billforge-postgres-primary-1 -- patronictl reinit

# 4. Scale up primary
kubectl config use-context billforge-primary
kubectl scale deployment billforge-api --replicas=3 -n billforge

# 5. Switch DNS back to primary
aws route53 change-resource-record-sets \
  --hosted-zone-id YOUR_HOSTED_ZONE_ID \
  --change-batch file://failback-dns.json

# 6. Scale down DR
kubectl config use-context billforge-dr
kubectl scale deployment billforge-api --replicas=1 -n billforge
```

## Monitoring

### Cross-Region Metrics

- Primary/DR latency
- Replication lag
- DNS failover events
- Cross-region traffic costs

### Alerts

```yaml
# prometheus/alerts.yml
- alert: CrossRegionReplicationLag
  expr: pg_replication_lag{region="dr"} > 60
  for: 10m
  labels:
    severity: critical
  annotations:
    summary: "PostgreSQL cross-region replication lag > 60s"

- alert: RegionFailoverTriggered
  expr: route53_health_check_status{region="primary"} == 0
  for: 2m
  labels:
    severity: critical
  annotations:
    summary: "Primary region health check failed - failover may occur"
```

## Cost Optimization

- **Primary region**: Full scale (3 API, 4 workers)
- **DR region**: Minimal scale (1 API, 1 worker) - scales up during failover
- **S3 storage**: Use STANDARD_IA for backups, GLACIER for archives
- **Reserved instances**: 1-year reserved for primary, on-demand for DR

## Testing

### Quarterly DR Drill

1. Schedule during low-traffic window
2. Announce maintenance window
3. Execute manual failover
4. Run smoke tests
5. Monitor for 1 hour
6. Execute failback
7. Document results

## Compliance

- **Data residency**: All data stays in configured regions
- **Encryption**: At-rest and in-transit encryption enabled
- **Audit logging**: CloudTrail enabled for cross-region activities
- **Backup retention**: 90 days primary, 30 days DR
