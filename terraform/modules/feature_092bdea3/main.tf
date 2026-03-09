provider "aws" {
  region = var.region
}

module "ecs_cluster" {
  source = "./modules/ecs_cluster"

  cluster_name = var.cluster_name
  vpc_id       = var.vpc_id
}

module "lambda_function" {
  source = "./modules/lambda_function"

  function_name    = var.lambda_function_name
  runtime          = var.lambda_runtime
  handler          = var.lambda_handler
  code_s3_bucket   = var.code_s3_bucket
  code_s3_key      = var.code_s3_key
  environment_variables = {
    "ENV_VAR" = "value"
  }
}

module "prometheus_grafana" {
  source = "./modules/prometheus_grafana"

  vpc_id       = var.vpc_id
  security_group_ids = [var.prometheus_security_group_id, var.grafana_security_group_id]
}

resource "aws_cloudwatch_log_group" "lambda_logs" {
  name              = "/aws/lambda/${module.lambda_function.function_name}"
  retention_in_days = 30
}