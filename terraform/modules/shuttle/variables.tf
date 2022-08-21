variable "api_container_port" {
  type        = number
  description = "Port API will be reachable at"
  default     = 8001
}

variable "api_fqdn" {
  type        = string
  description = "Fully qualified domain name where the api will be reachable at"
}

variable "db_fqdn" {
  type        = string
  description = "Fully qualified domain name where the shared databases will be reachable at"
}

variable "instance_type" {
  type        = string
  description = "EC2 instance type to provision"
  default     = "c6g.large"
}

variable "postgres_container_port" {
  type        = number
  description = "Port Postgres will be reachable at"
  default     = 5432
}

variable "mongodb_container_port" {
  type        = number
  description = "Port Mongodb will be reachable at"
  default     = 27017
}

variable "postgres_password" {
  type        = string
  description = "Root password for postgres instance"
}

variable "mongodb_password" {
  type        = string
  description = "Admin password for mongodb instance"
}

variable "proxy_container_port" {
  type        = number
  description = "Port reverse proxy will be reachable at"
  default     = 8000
}

variable "proxy_fqdn" {
  type        = string
  description = "The top level domain where deployed services can be reached at"
}

variable "shuttle_admin_secret" {
  type        = string
  description = "Secret for the shuttle admin user"
}
