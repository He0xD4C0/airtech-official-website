# AIRTEKPOWER Jenkins host

This directory manages the native Jenkins controller and its independent local
SSH agent. The controller has zero executors and is not in the `docker` group;
only `jenkins-agent` can execute builds and access Docker.

`provision-debian.sh` is intended for a fresh Debian 12 VM. It installs Java 21,
Jenkins LTS, Docker Engine/Buildx/Compose, Node 22, pnpm 11.19, Rust 1.88 and
1.98, Playwright Chromium dependencies and Trivy. It also installs the pinned
plugins and JCasC configuration, generates controller and local-agent secrets,
and applies the LAN firewall policy. Secrets remain root/Jenkins-readable on the
VM and never enter this repository.

On the PVE node, `create-pve-vm.sh /root/deploy.pub` creates VMID 107 only when
that ID is unused. It downloads the official Debian 12 Generic Cloud image,
verifies its published SHA-512 checksum, imports a 200 GB disk to `ZTnvme`,
configures 12 vCPU/24 GiB fixed RAM, VirtIO networking, QEMU Guest Agent,
cloud-init DHCP and host autostart, then boots the VM. It refuses to overwrite
an existing VM.

After provisioning, add these Jenkins Credentials through the UI:

- `airtek-github-read`: username/password credential containing a dedicated
  read-only GitHub token for polling this private repository.
- `airtek-ghcr-push`: username/password credential containing a dedicated GHCR
  package-write token.
- `airtek-prod-ssh`: SSH private key for the future restricted deployment user.
- `airtek-prod-known-hosts`: secret file containing the target host key.

Leave `CD_ENABLED=false` until the target database, network, TLS/DNS,
`/etc/airtek/production.env`, GHCR read-only login and restricted `deploy` sudo
policy are ready. Set `AIRTEK_DEPLOY_HOST` and then change `CD_ENABLED` through
the managed JCasC environment before enabling deployment.

The controller listens on all VM interfaces at port 8080. The host firewall
allows that port only from RFC1918 networks, permits SSH only from
`10.10.14.0/24`, and applies a `DOCKER-USER` policy so test containers cannot
accidentally expose host ports beyond the Jenkins VM.
