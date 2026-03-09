provider "aws" {
  region = var.region
}

resource "aws_vpc" "main" {
  cidr_block       = var.vpc_cidr_block
  enable_dns_support   = true
  enable_dns_hostnames = true
}

resource "aws_subnet" "public" {
  count            = length(var.public_subnets)
  vpc_id             = aws_vpc.main.id
  cidr_block         = var.public_subnets[count.index]
  availability_zone  = element(var.availability_zones, count.index)
  map_public_ip_on_launch = true
}

resource "aws_subnet" "private" {
  count            = length(var.private_subnets)
  vpc_id             = aws_vpc.main.id
  cidr_block         = var.private_subnets[count.index]
  availability_zone  = element(var.availability_zones, count.index)
  map_public_ip_on_launch = false
}

resource "aws_internet_gateway" "main" {
  vpc_id = aws_vpc.main.id
}

resource "aws_route_table" "public" {
  vpc_id = aws_vpc.main.id

  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.main.id
  }

  associate_with_private_ips = false
}

resource "aws_route_table" "private" {
  vpc_id = aws_vpc.main.id

  associate_with_private_ips = true
}

resource "aws_route_table_association" "public" {
  count          = length(aws_subnet.public)
  subnet_id      = element(aws_subnet.public, count.index).id
  route_table_id = aws_route_table.public.id
}

resource "aws_security_group" "web_ui" {
  name        = "web_ui"
  description = "Allow HTTP and HTTPS from the internet"

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
}

resource "aws_security_group" "api_gateway" {
  name        = "api_gateway"
  description = "Allow HTTP and HTTPS from the internet"

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
}

resource "aws_security_group" "application" {
  name        = "application"
  description = "Allow all traffic within the VPC"

  ingress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["10.0.0.0/16"]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["10.0.0.0/16"]
  }
}

resource "aws_launch_template" "main" {
  name_prefix      = "OCRProvider-Instance"
  image_id         = var.ami_id
  instance_type    = var.instance_type
  key_name         = var.key_name
  security_groups  = [aws_security_group.application.id]
  user_data        = filebase64("${path.module}/user-data.sh")

  block_device_mappings {
    device_name = "/dev/xvda"
    ebs {
      volume_size = var.instance_volume_size
      volume_type = "gp2"
    }
  }

  iam_instance_profile {
    name = aws_iam_instance_profile.main.name
  }
}

resource "aws_autoscaling_group" "main" {
  launch_template {
    id      = aws_launch_template.main.id
    version = "$Latest"
  }

  min_size             = var.asg_min_size
  max_size             = var.asg_max_size
  desired_capacity     = var.asg_desired_capacity
  vpc_zone_identifier  = concat(aws_subnet.private[*].id, aws_subnet.public[*].id)
  health_check_type    = "ELB"
  target_group_arns    = [aws_lb_target_group.main.arn]

  tag {
    key                 = "Name"
    value               = "OCRProvider-AutoScalingGroup"
    propagate_at_launch = true
  }
}

resource "aws_lb" "main" {
  name               = "OCRProvider-ALB"
  subnets            = aws_subnet.public[*].id
  security_groups      = [aws_security_group.web_ui.id]
  internal           = false

  idle_timeout = 300

  listener {
    port     = 80
    protocol = "HTTP"

    default_action {
      type             = "forward"
      target_group_arn = aws_lb_target_group.main.arn
    }
  }

  listener {
    port     = 443
    protocol = "HTTPS"
    ssl_policy = "ELBSecurityPolicy-2016-08"

    certificate_arn = var.ssl_certificate_arn

    default_action {
      type             = "forward"
      target_group_arn = aws_lb_target_group.main.arn
    }
  }
}

resource "aws_lb_target_group" "main" {
  name     = "OCRProvider-TargetGroup"
  port     = 80
  protocol = "HTTP"
  vpc_id   = aws_vpc.main.id

  health_check {
    path                = "/"
    interval            = 30
    timeout             = 5
    healthy_threshold   = 2
    unhealthy_threshold = 2
  }
}

resource "aws_security_group_rule" "allow_alb_to_ec2" {
  security_group_id = aws_security_group.application.id

  ingress {
    from_port   = 80
    to_port     = 80
    protocol    = "tcp"
    cidr_blocks = [aws_lb.main.subnets[*].cidr_block]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }
}

resource "aws_iam_instance_profile" "main" {
  name = "OCRProvider-InstanceProfile"

  roles = [aws_iam_role.main.name]
}

resource "aws_iam_role" "main" {
  name = "OCRProvider-Role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Action = "sts:AssumeRole"
        Effect = "Allow"
        Principal = {
          Service = "ec2.amazonaws.com"
        }
      }
    ]
  })
}

resource "aws_iam_role_policy" "main" {
  name   = "OCRProvider-RolePolicy"
  role   = aws_iam_role.main.id
  policy = filebase64("${path.module}/lambda_policy.json")
}

resource "aws_lambda_function" "ocr_provider" {
  function_name = "OCRProvider-LambdaFunction"
  role          = aws_iam_role.lambda_role.arn
  handler       = "lambda_function.lambda_handler"
  runtime       = "python3.8"

  environment {
    variables = {
      ENV = var.lambda_environment_variables.env
    }
  }

  source_code_hash = filebase64sha256("${path.module}/lambda_function.py")

  depends_on = [aws_iam_role_policy.lambda_policy]
}

resource "aws_lambda_permission" "allow_alb_to_invoke_lambda" {
  statement_id  = "AllowALBInvoke"
  action        = "lambda:InvokeFunction"
  function_name = aws_lambda_function.ocr_provider.function_name
  principal     = "elasticloadbalancing.amazonaws.com"
  source_arn    = aws_lb.main.arn
}

resource "aws_alb_listener_rule" "ocr_provider_rule" {
  listener_arn = aws_lb.listener.http.id
  priority     = 100

  action {
    type             = "forward"
    target_group_arn = aws_lb_target_group.ocr_provider.arn
  }

  condition {
    field    = "path-pattern"
    values = ["/ocr/*"]
  }
}

resource "aws_cloudwatch_metric_alarm" "high_cpu_utilization" {
  alarm_name                = "HighCPUUtilization"
  comparison_operator         = "GreaterThanThreshold"
  evaluation_periods        = var.alarms.high_cpu_utilization.evaluation_periods
  metric_name               = "CPUUtilization"
  namespace                 = "AWS/EC2"
  period                    = var.alarms.high_cpu_utilization.period
  statistic                 = "Average"
  threshold                 = var.alarms.high_cpu_utilization.threshold
  dimensions                = {
    InstanceId = aws_autoscaling_group.main.launch_template.id
  }
  alarm_actions             = [aws_sns_topic.notification.arn]
  insufficient_data_actions = []
  ok_actions                  = []

  depends_on = [aws_iam_role_policy.cloudwatch_alarms_policy]
}

resource "aws_sns_topic" "notification" {
  name = "OCRProvider-Notification"
}

resource "aws_sns_subscription" "email" {
  topic_arn = aws_sns_topic.notification.arn
  protocol  = "email"
  endpoint  = var.notification_email
}

resource "aws_iam_role_policy" "cloudwatch_alarms_policy" {
  name   = "CloudWatchAlarmsPolicy"
  role   = aws_iam_role.main.id
  policy = filebase64("${path.module}/cloudwatch_alarms_policy.json")
}