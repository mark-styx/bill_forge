resource "aws_cloudwatch_log_group" "ecs_logs" {
  name              = "/aws/ecs/${module.ecs_cluster.cluster_name}"
  retention_in_days = 30
}

resource "aws_cloudwatch_log_group" "prometheus_logs" {
  name              = "/aws/prometheus"
  retention_in_days = 90
}

resource "aws_prometheus_grafana" "grafana" {
  vpc_id       = module.prometheus_grafana.vpc_id
  security_group_ids = [module.prometheus_grafana.security_group_ids]
}

resource "aws_alb_target_group" "prometheus" {
  name     = "prometheus-target-group"
  port     = 9090
  protocol = "HTTP"
  vpc_id   = module.vpc.id

  health_check {
    interval            = 30
    path                = "/api/v1/targets"
    timeout             = 5
    healthy_threshold   = 2
    unhealthy_threshold = 2
    matcher             = "2xx-3xx"
  }
}

resource "aws_alb_listener" "prometheus" {
  load_balancer_arn = aws_lb.prometheus.arn
  port              = 80
  protocol          = "HTTP"

  default_action {
    type             = "forward"
    target_group_arn = aws_alb_target_group.prometheus.arn
  }
}