variable "region" {
  description = "AWS region"
  type        = string
}

variable "vpc_cidr" {
  description = "VPC CIDR block"
  type        = string
}

variable "public_subnets" {
  description = "List of public subnets"
  type        = list(string)
}

variable "private_subnets" {
  description = "List of private subnets"
  type        = list(string)
}

variable "container_port" {
  description = "Container port"
  type        = number
}

variable "task_cpu" {
  description = "ECS task CPU"
  type        = string
}

variable "task_memory" {
  description = "ECS task memory"
  type        = string
}

variable "container_image" {
  description = "Docker container image"
  type        = string
}

variable "environment_variables" {
  description = "Environment variables for the container"
  type        = map(string)
}

variable "desired_count" {
  description = "Desired number of tasks in the ECS service"
  type        = number
}

variable "db_engine" {
  description = "RDS database engine"
  type        = string
}

variable "db_engine_version" {
  description = "RDS database engine version"
  type        = string
}

variable "db_instance_class" {
  description = "RDS instance class"
  type        = string
}

variable "db_username" {
  description = "Database username"
  type        = string
}

variable "db_password" {
  description = "Database password"
  type        = string
}

variable "environment" {
  description = "Environment (e.g., dev, prod)"
  type        = string
}