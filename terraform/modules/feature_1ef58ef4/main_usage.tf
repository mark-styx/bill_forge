module "ocr_provider" {
  source = "./path/to/module"

  region                     = "us-west-2"
  vpc_cidr_block               = "10.0.0.0/16"
  public_subnets             = ["10.0.1.0/24", "10.0.2.0/24"]
  private_subnets            = ["10.0.3.0/24", "10.0.4.0/24"]
  availability_zones         = ["us-west-2a", "us-west-2b"]
  ami_id                     = "ami-0c55b159cbfafe1f0"
  key_name                   = "my-key-pair"
  instance_type              = "t2.micro"
  instance_volume_size       = 8
  asg_min_size               = 1
  asg_max_size               = 3
  asg_desired_capacity     = 2
  ssl_certificate_arn        = "arn:aws:acm:us-west-2::certificate/12345678-1234-1234-1234-1234567890ab"
  lambda_environment_variables = {
    env = "production"
  }
  notification_email         = "admin@example.com"
}