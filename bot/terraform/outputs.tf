output "service_url" {
  # Set this as the Interactions Endpoint URL in the Discord developer portal
  value = google_cloud_run_v2_service.bot.uri
}

output "artifact_registry_repo" {
  value = "${var.region}-docker.pkg.dev/${var.project_id}/${google_artifact_registry_repository.bot.repository_id}"
}
