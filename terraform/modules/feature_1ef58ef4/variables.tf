variable "region" {
  description = "The AWS region to deploy resources in."
  type        = string
}

variable "vpc_cidr_block" {
  description = "CIDR block for the VPC."
  type        = string
}

variable "public_subnets" {
  description = "List of CIDR blocks for public subnets."
  type        = list(string)
}

variable "private_subnets" {
  description = "List of CIDR blocks for private subnets."
  type        = list(string)
}

variable "availability_zones" {
  description = "List of availability zones."
  type        = list(string)
}

variable "ami_id" {
  description = "AMI ID to use for EC2 instances."
  type        = string
}

variable "key_name" {
  description = "Name of the SSH key pair to associate with instances."
  type        = string
}

variable "instance_type" {
  description = "Instance type for EC2 instances."
  type        = string
}

variable "instance_volume_size" {
  description = "Size of the root volume for EC2 instances."
  type        = number
}

variable "asg_min_size" {
  description = "Minimum size of the auto-scaling group."
  type        = number
}

variable "asg_max_size" {
  description = "Maximum size of the auto-scaling group."
  type        = number
}

variable "asg_desired_capacity" {
  description = "Desired capacity of the auto-scaling group."
  type        = number
}

variable "ssl_certificate_arn" {
  description = "ARN of the SSL certificate for HTTPS listener."
  type        = string
}

variable "lambda_environment_variables" {
  description = "Environment variables for the Lambda function."
  type        = object({
    env = string
  })
}

variable "notification_email" {
  description = "Email address to receive notifications."
  type        = string
}