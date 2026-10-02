variable "AIRTEK_IMAGE_PREFIX" {
  default = ""
}

variable "AIRTEK_IMAGE_TAG" {
  default = "invalid"
}

variable "AIRTEK_IMAGE_REVISION" {
  default = "unknown"
}

variable "AIRTEK_IMAGE_SOURCE" {
  default = ""
}

variable "AIRTEK_TARGET_PLATFORM" {
  default = "linux/amd64"
}

group "default" {
  targets = ["public-web", "admin-web", "platform", "migrations", "gateway"]
}

target "base" {
  context = "."
  platforms = [AIRTEK_TARGET_PLATFORM]
  labels = merge(
    { "org.opencontainers.image.revision" = AIRTEK_IMAGE_REVISION },
    AIRTEK_IMAGE_SOURCE == "" ? {} : { "org.opencontainers.image.source" = AIRTEK_IMAGE_SOURCE },
  )
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
