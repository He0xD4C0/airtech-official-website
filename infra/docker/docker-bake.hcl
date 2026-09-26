variable "AIRTEK_IMAGE_PREFIX" {
  default = "ghcr.io/he0xd4c0/airtekpower"
}

variable "AIRTEK_IMAGE_TAG" {
  default = "invalid"
}

variable "AIRTEK_IMAGE_REVISION" {
  default = "unknown"
}

group "default" {
  targets = ["public-web", "admin-web", "platform", "migrations", "gateway"]
}

target "base" {
  context = "."
  platforms = ["linux/amd64"]
  labels = {
    "org.opencontainers.image.source" = "https://github.com/He0xD4C0/airtech-official-website"
    "org.opencontainers.image.revision" = AIRTEK_IMAGE_REVISION
  }
}

target "public-web" {
  inherits = ["base"]
  dockerfile = "infra/docker/Dockerfile.web"
  tags = ["${AIRTEK_IMAGE_PREFIX}-public-web:${AIRTEK_IMAGE_TAG}"]
}

target "admin-web" {
  inherits = ["base"]
  dockerfile = "infra/docker/Dockerfile.admin"
  tags = ["${AIRTEK_IMAGE_PREFIX}-admin-web:${AIRTEK_IMAGE_TAG}"]
}

target "platform" {
  inherits = ["base"]
  dockerfile = "infra/docker/Dockerfile.platform"
  tags = ["${AIRTEK_IMAGE_PREFIX}-platform:${AIRTEK_IMAGE_TAG}"]
}

target "migrations" {
  inherits = ["base"]
  dockerfile = "infra/docker/Dockerfile.flyway"
  tags = ["${AIRTEK_IMAGE_PREFIX}-migrations:${AIRTEK_IMAGE_TAG}"]
}

target "gateway" {
  inherits = ["base"]
  dockerfile = "infra/docker/Dockerfile.gateway"
  tags = ["${AIRTEK_IMAGE_PREFIX}-gateway:${AIRTEK_IMAGE_TAG}"]
}
