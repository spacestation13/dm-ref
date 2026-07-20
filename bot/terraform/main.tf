terraform {
  required_providers {
    google = {
      source  = "hashicorp/google"
      version = "~> 6.0"
    }
  }
}

provider "google" {
  project = var.project_id
  region  = var.region
}

resource "google_project_service" "run" {
  service            = "run.googleapis.com"
  disable_on_destroy = false
}

resource "google_project_service" "artifactregistry" {
  service            = "artifactregistry.googleapis.com"
  disable_on_destroy = false
}

resource "google_project_service" "secretmanager" {
  service            = "secretmanager.googleapis.com"
  disable_on_destroy = false
}

resource "google_project_service" "firestore" {
  service            = "firestore.googleapis.com"
  disable_on_destroy = false
}

resource "google_firestore_database" "default" {
  name        = "(default)"
  location_id = var.region
  type        = "FIRESTORE_NATIVE"

  depends_on = [google_project_service.firestore]
}

resource "google_artifact_registry_repository" "bot" {
  repository_id = "dm-ref-bot"
  format        = "DOCKER"
  location      = var.region

  depends_on = [google_project_service.artifactregistry]
}

resource "google_secret_manager_secret" "discord_bot_token" {
  secret_id = "discord-bot-token"

  replication {
    auto {}
  }

  depends_on = [google_project_service.secretmanager]
}

resource "google_secret_manager_secret_version" "discord_bot_token" {
  secret      = google_secret_manager_secret.discord_bot_token.id
  secret_data = var.discord_bot_token
}

resource "google_secret_manager_secret" "discord_client_secret" {
  secret_id = "discord-client-secret"

  replication {
    auto {}
  }

  depends_on = [google_project_service.secretmanager]
}

resource "google_secret_manager_secret_version" "discord_client_secret" {
  secret      = google_secret_manager_secret.discord_client_secret.id
  secret_data = var.discord_client_secret
}

resource "google_service_account" "bot" {
  account_id   = "dm-ref-bot"
  display_name = "DM Ref Bot"
}

resource "google_cloud_run_v2_service" "bot" {
  name                = "dm-ref-bot"
  location            = var.region
  deletion_protection = false

  template {
    service_account = google_service_account.bot.email

    scaling {
      max_instance_count = 1
      min_instance_count = 0
    }

    containers {
      image = "${var.region}-docker.pkg.dev/${var.project_id}/dm-ref-bot/dm-ref-bot:latest"

      ports {
        container_port = 8080
      }

      env {
        name  = "DISCORD_PUBLIC_KEY"
        value = var.discord_public_key
      }

      env {
        name  = "DISCORD_APPLICATION_ID"
        value = var.discord_application_id
      }

      env {
        name = "DISCORD_BOT_TOKEN"
        value_source {
          secret_key_ref {
            secret  = google_secret_manager_secret.discord_bot_token.secret_id
            version = "latest"
          }
        }
      }

      env {
        name = "DISCORD_CLIENT_SECRET"
        value_source {
          secret_key_ref {
            secret  = google_secret_manager_secret.discord_client_secret.secret_id
            version = "latest"
          }
        }
      }

      resources {
        limits = {
          cpu    = "1"
          memory = "512Mi"
        }
      }
    }
  }

  depends_on = [google_project_service.run]
}

resource "google_secret_manager_secret_iam_member" "bot_secret_access" {
  secret_id = google_secret_manager_secret.discord_bot_token.secret_id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${google_service_account.bot.email}"
}

resource "google_secret_manager_secret_iam_member" "bot_client_secret_access" {
  secret_id = google_secret_manager_secret.discord_client_secret.secret_id
  role      = "roles/secretmanager.secretAccessor"
  member    = "serviceAccount:${google_service_account.bot.email}"
}

resource "google_project_iam_member" "bot_firestore_access" {
  project = var.project_id
  role    = "roles/datastore.user"
  member  = "serviceAccount:${google_service_account.bot.email}"
}

resource "google_cloud_run_v2_service_iam_member" "public_access" {
  name     = google_cloud_run_v2_service.bot.name
  location = var.region
  role     = "roles/run.invoker"
  member   = "allUsers"
}
