region = "us-west-2"

vpc_cidr = "10.0.0.0/16"

public_subnets = ["subnet-12345678", "subnet-23456789"]
private_subnets = ["subnet-34567890", "subnet-45678901"]

container_port = 80
task_cpu = "512"
task_memory = "1GB"
container_image = "your-docker-image:latest"
environment_variables = {
  ENV_VAR_1 = "value1"
  ENV_VAR_2 = "value2"
}
desired_count = 2

db_engine = "mysql"
db_engine_version = "5.7"
db_instance_class = "db.t3.micro"
db_username = "admin"
db_password = "password"

environment = "dev"