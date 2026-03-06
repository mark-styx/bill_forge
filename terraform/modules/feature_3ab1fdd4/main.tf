provider "aws" {
  region = var.region
}

# VPC and Subnets
resource "aws_vpc" "ocr_queue_vpc" {
  cidr_block           = var.vpc_cidr
  enable_dns_support   = true
  enable_dns_hostnames = true

  tags = {
    Name = "OCR-Queue-VPC"
  }
}

resource "aws_subnet" "public_subnets" {
  count          = length(var.public_subnet_cidrs)
  vpc_id         = aws_vpc.ocr_queue_vpc.id
  cidr_block     = var.public_subnet_cidrs[count.index]
  availability_zone = element(var.availability_zones, count.index)

  tags = {
    Name = "Public-Subnet-${count.index + 1}"
  }
}

resource "aws_subnet" "private_subnets" {
  count          = length(var.private_subnet_cidrs)
  vpc_id         = aws_vpc.ocr_queue_vpc.id
  cidr_block     = var.private_subnet_cidrs[count.index]
  availability_zone = element(var.availability_zones, count.index)

  tags = {
    Name = "Private-Subnet-${count.index + 1}"
  }
}

# Security Groups
resource "aws_security_group" "alb_sg" {
  vpc_id = aws_vpc.ocr_queue_vpc.id

  ingress {
    from_port   = 80
    to_port     = 80
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  ingress {
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "ALB-SG"
  }
}

resource "aws_security_group" "ecs_sg" {
  vpc_id = aws_vpc.ocr_queue_vpc.id

  ingress {
    from_port   = 80
    to_port     = 80
    protocol    = "tcp"
    source_security_group_id = aws_security_group.alb_sg.id
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "ECS-SG"
  }
}

# Application Load Balancer
resource "aws_lb" "ocr_queue_alb" {
  name               = "ocr-queue-alb"
  internal           = false
  load_balancer_type = "application"
  security_groups    = [aws_security_group.alb_sg.id]
  subnets            = aws_subnet.public_subnets[*].id

  tags = {
    Name = "OCR-Queue-ALB"
  }
}

# Target Group for ECS Service
resource "aws_lb_target_group" "ocr_queue_tg" {
  name     = "ocr-queue-tg"
  port     = var.container_port
  protocol = "HTTP"
  vpc_id   = aws_vpc.ocr_queue_vpc.id

  health_check {
    path                = "/health"
    interval            = 30
    timeout             = 10
    healthy_threshold   = 2
    unhealthy_threshold = 2
  }

  tags = {
    Name = "OCR-Queue-TG"
  }
}

# ECS Cluster
resource "aws_ecs_cluster" "ocr_queue_cluster" {
  name     = "ocr-queue-cluster"
  capacity_providers = ["FARGATE"]
  settings = [
    {
      name = "containerInsights"
      value = "enabled"
    }
  ]

  tags = {
    Name = "OCR-Queue-Cluster"
  }
}

# ECS Task Definition
resource "aws_ecs_task_definition" "ocr_queue_task_def" {
  family                = "ocr-queue-task"
  network_mode          = "awsvpc"
  requires_compatibilities = ["FARGATE"]
  cpu                   = var.task_cpu
  memory                = var.task_memory

  container_definitions = jsonencode([
    {
      name      = "ocr-queue-container"
      image     = var.container_image
      essential = true
      portMappings = [
        {
          containerPort = var.container_port
          hostPort      = var.container_port
        }
      ]
      environment = var.environment_variables
      logConfiguration = {
        logDriver = "awslogs"
        options = {
          "awslogs-group"   = "/ecs/ocr-queue-task"
          "awslogs-region"  = var.region
          "awslogs-stream-prefix" = "ecs"
        }
      }
    }
  ])

  execution_role_arn = aws_iam_role.ecs_task_execution_role.arn

  tags = {
    Name = "OCR-Queue-Task-Def"
  }
}

# ECS Service
resource "aws_ecs_service" "ocr_queue_service" {
  name            = "ocr-queue-service"
  cluster         = aws_ecs_cluster.ocr_queue_cluster.id
  task_definition = aws_ecs_task_definition.ocr_queue_task_def.arn
  launch_type     = "FARGATE"
  desired_count   = var.desired_count

  network_configuration {
    subnets          = aws_subnet.private_subnets[*].id
    security_groups  = [aws_security_group.ecs_sg.id]
    assign_public_ip = false
  }

  load_balancer {
    target_group_arn = aws_lb_target_group.ocr_queue_tg.arn
    container_name   = "ocr-queue-container"
    container_port   = var.container_port
  }

  scaling {
    minimum = 1
    maximum = 2
    unit    = "percent"

    metric {
      name              = "CPUUtilization"
      namespace         = "AWS/ECS"
      statistic         = "Average"
      period            = 300
      evaluation_periods = 2
      comparison_operator = "GreaterThanThreshold"
      threshold           = 50
    }
  }

  depends_on = [aws_ecs_cluster.ocr_queue_cluster]
}

# RDS Database
resource "aws_db_instance" "ocr_queue_rds" {
  identifier             = "ocr-queue-db"
  engine                 = var.db_engine
  engine_version         = var.db_engine_version
  instance_class         = var.db_instance_class
  username               = var.db_username
  password               = aws_secretsmanager_secret.ocr_queue_db_secret.secret_string
  parameter_group_name   = "default.mysql5.7"
  storage_type           = "gp2"
  allocated_storage      = 20
  multi_az               = true

  vpc_security_group_ids = [aws_security_group.rds_sg.id]
  subnet_ids             = aws_subnet.private_subnets[*].id

  tags = {
    Name = "OCR-Queue-RDS"
  }

  lifecycle {
    prevent_destroy = true
  }
}

resource "aws_db_instance_read_replica" "ocr_queue_rds_replica" {
  identifier             = "ocr-queue-db-replica"
  source_db_instance_identifier = aws_db_instance.ocr_queue_rds.identifier
  engine                 = var.db_engine
  engine_version         = var.db_engine_version
  instance_class         = var.db_instance_class
  parameter_group_name   = "default.mysql5.7"
  storage_type           = "gp2"
  allocated_storage      = 20

  vpc_security_group_ids = [aws_security_group.rds_sg.id]
  subnet_ids             = aws_subnet.private_subnets[*].id

  tags = {
    Name = "OCR-Queue-RDS-Replica"
  }
}

# S3 Bucket for Logs
resource "aws_s3_bucket" "ocr_queue_logs" {
  bucket = "ocr-queue-logs-${var.environment}"
  acl    = "private"

  versioning {
    enabled = true
  }

  lifecycle_rule {
    id      = "Expire Old Objects"
    enabled = true
    expiration {
      days = 365
    }
  }

  tags = {
    Name = "OCR-Queue-Logs-Bucket"
  }
}

# RDS Security Group
resource "aws_security_group" "rds_sg" {
  vpc_id = aws_vpc.ocr_queue_vpc.id

  ingress {
    from_port   = 3306
    to_port     = 3306
    protocol    = "tcp"
    source_security_group_id = aws_security_group.ecs_sg.id
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "RDS-SG"
  }
}

# IAM Role for ECS Task Execution
resource "aws_iam_role" "ecs_task_execution_role" {
  name = "ECS-Task-Execution-Role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Action = "sts:AssumeRole",
        Effect = "Allow",
        Principal = {
          Service = "ecs-tasks.amazonaws.com"
        }
      }
    ]
  })

  tags = {
    Name = "ECS-Task-Execution-Role"
  }
}

resource "aws_iam_role_policy" "ecs_task_execution_role_policy" {
  name = "ECS-Task-Execution-Role-Policy"
  role = aws_iam_role.ecs_task_execution_role.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Action = [
          "logs:CreateLogGroup",
          "logs:CreateLogStream",
          "logs:PutLogEvents"
        ],
        Effect = "Allow",
        Resource = "arn:aws:logs:*:*:/ecs/*"
      },
      {
        Action = [
          "ecr:GetAuthorizationToken",
          "ecr:GetDownloadUrlForLayer",
          "ecr:BatchGetImage",
          "ecr:InitiateLayerUpload",
          "ecr:UploadLayerPart",
          "ecr:CompleteLayerUpload",
          "ecr:PutImage"
        ],
        Effect = "Allow",
        Resource = "*"
      },
      {
        Action = [
          "ssm:GetParameters"
        ],
        Effect = "Allow",
        Resource = aws_ssm_parameter.ocr_queue_secrets.id
      }
    ]
  })
}

# Secrets Manager Secret for Database Credentials
resource "aws_secretsmanager_secret" "ocr_queue_db_secret" {
  name = "OCR-Queue-DB-Secret"

  secret_string = jsonencode({
    username = var.db_username
    password = var.db_password
  })

  tags = {
    Name = "OCR-Queue-DB-Secret"
  }
}

# SSM Parameter Store for Secrets
resource "aws_ssm_parameter" "ocr_queue_secrets" {
  name        = "/ocr-queue/secrets"
  type        = "String"
  value       = aws_secretsmanager_secret.ocr_queue_db_secret.secret_string
  description = "Database credentials and other secrets"

  tags = {
    Name = "OCR-Queue-Secrets"
  }
}

# Outputs
output "load_balancer_dns_name" {
  value = aws_lb.ocr_queue_alb.dns_name
}

output "ecs_service_arn" {
  value = aws_ecs_service.ocr_queue_service.arn
}

output "rds_db_endpoint" {
  value = aws_db_instance.ocr_queue_rds.endpoint
}