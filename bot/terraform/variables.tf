variable "project_id" {
  type = string
}

variable "region" {
  type    = string
  default = "us-central1"
}

variable "discord_public_key" {
  type = string
}

variable "discord_bot_token" {
  type      = string
  sensitive = true
}

variable "discord_application_id" {
  type = string
}
