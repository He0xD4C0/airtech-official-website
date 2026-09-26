#!/bin/bash
set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
  echo "Run this script as root on the dedicated Debian 12 Jenkins VM." >&2
  exit 2
fi

repo_root=$(cd "$(dirname "$0")/../.." && pwd)
export DEBIAN_FRONTEND=noninteractive
install -d -m 0755 /etc/apt/keyrings
apt-get update
apt-get install -y ca-certificates curl fontconfig git gnupg jq nftables openssh-server openssl unzip xz-utils

curl -fsSL https://packages.adoptium.net/artifactory/api/gpg/key/public \
  | gpg --dearmor --yes -o /etc/apt/keyrings/adoptium.gpg
printf '%s\n' 'deb [signed-by=/etc/apt/keyrings/adoptium.gpg] https://packages.adoptium.net/artifactory/deb bookworm main' \
  >/etc/apt/sources.list.d/adoptium.list

curl -fsSL https://pkg.jenkins.io/debian-stable/jenkins.io-2026.key \
  -o /etc/apt/keyrings/jenkins-keyring.asc
printf '%s\n' 'deb [signed-by=/etc/apt/keyrings/jenkins-keyring.asc] https://pkg.jenkins.io/debian-stable binary/' \
  >/etc/apt/sources.list.d/jenkins.list

curl -fsSL https://download.docker.com/linux/debian/gpg \
  | gpg --dearmor --yes -o /etc/apt/keyrings/docker.gpg
printf '%s\n' \
  'deb [arch=amd64 signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian bookworm stable' \
  >/etc/apt/sources.list.d/docker.list

curl -fsSL https://deb.nodesource.com/gpgkey/nodesource-repo.gpg.key \
  | gpg --dearmor --yes -o /etc/apt/keyrings/nodesource.gpg
printf '%s\n' \
  'deb [arch=amd64 signed-by=/etc/apt/keyrings/nodesource.gpg] https://deb.nodesource.com/node_22.x nodistro main' \
  >/etc/apt/sources.list.d/nodesource.list

curl -fsSL https://aquasecurity.github.io/trivy-repo/deb/public.key \
  | gpg --dearmor --yes -o /etc/apt/keyrings/trivy.gpg
printf '%s\n' \
  'deb [signed-by=/etc/apt/keyrings/trivy.gpg] https://aquasecurity.github.io/trivy-repo/deb generic main' \
  >/etc/apt/sources.list.d/trivy.list

apt-get update
apt-get install -y \
  temurin-21-jdk jenkins nodejs qemu-guest-agent trivy \
  docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
systemctl stop jenkins

corepack enable
corepack prepare pnpm@11.19.0 --activate
corepack pnpm dlx playwright@1.55.0 install-deps chromium

if ! id jenkins-agent >/dev/null 2>&1; then
  useradd --create-home --home-dir /var/lib/jenkins-agent --shell /bin/bash jenkins-agent
fi
usermod -aG docker jenkins-agent
gpasswd -d jenkins docker >/dev/null 2>&1 || true
install -d -o jenkins-agent -g jenkins-agent -m 0700 /var/lib/jenkins-agent/.ssh
install -d -o jenkins-agent -g jenkins-agent -m 0755 /var/cache/airtek/{cargo,pnpm,playwright,buildkit,trivy}

su -s /bin/bash jenkins-agent -c \
  'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain 1.98.0'
su -s /bin/bash jenkins-agent -c \
  '/var/lib/jenkins-agent/.cargo/bin/rustup toolchain install 1.88.0 --profile minimal'
su -s /bin/bash jenkins-agent -c \
  '/var/lib/jenkins-agent/.cargo/bin/rustup component add rustfmt clippy --toolchain 1.98.0'

install -d -o jenkins -g jenkins -m 0700 /var/lib/jenkins/secrets /var/lib/jenkins/.ssh
if [ ! -f /var/lib/jenkins/secrets/airtek-agent-key ]; then
  ssh-keygen -q -t ed25519 -N '' -C airtek-local-jenkins-agent \
    -f /var/lib/jenkins/secrets/airtek-agent-key
fi
cat /var/lib/jenkins/secrets/airtek-agent-key.pub \
  >/var/lib/jenkins-agent/.ssh/authorized_keys
chown jenkins-agent:jenkins-agent /var/lib/jenkins-agent/.ssh/authorized_keys
chmod 0600 /var/lib/jenkins-agent/.ssh/authorized_keys
ssh-keyscan -H 127.0.0.1 >/var/lib/jenkins/.ssh/known_hosts 2>/dev/null
chown -R jenkins:jenkins /var/lib/jenkins/secrets /var/lib/jenkins/.ssh
chmod 0600 /var/lib/jenkins/secrets/airtek-agent-key /var/lib/jenkins/.ssh/known_hosts

install -o jenkins -g jenkins -m 0600 "$repo_root/infra/jenkins/jenkins.yaml" \
  /var/lib/jenkins/jenkins.yaml
plugin_manager_version=2.14.0
curl -fsSL \
  "https://github.com/jenkinsci/plugin-installation-manager-tool/releases/download/${plugin_manager_version}/jenkins-plugin-manager-${plugin_manager_version}.jar" \
  -o /usr/local/lib/jenkins-plugin-manager.jar
java -jar /usr/local/lib/jenkins-plugin-manager.jar \
  --war /usr/share/java/jenkins.war \
  --plugin-file "$repo_root/infra/jenkins/plugins.txt" \
  --plugin-download-directory /var/lib/jenkins/plugins
chown -R jenkins:jenkins /var/lib/jenkins/plugins

admin_password_file=/var/lib/jenkins/secrets/airtek-admin-password
if [ ! -s "$admin_password_file" ]; then
  openssl rand -base64 36 >"$admin_password_file"
fi
chown jenkins:jenkins "$admin_password_file"
chmod 0600 "$admin_password_file"
jenkins_ip=$(hostname -I | awk '{print $1}')
admin_password=$(tr -d '\n' <"$admin_password_file")
install -d -m 0755 /etc/systemd/system/jenkins.service.d
{
  printf '%s\n' '[Service]'
  printf 'Environment="CASC_JENKINS_CONFIG=/var/lib/jenkins/jenkins.yaml"\n'
  printf 'Environment="JAVA_OPTS=-Djenkins.install.runSetupWizard=false"\n'
  printf 'Environment="JENKINS_PORT=8080"\n'
  printf 'Environment="AIRTEK_JENKINS_URL=http://%s:8080/"\n' "$jenkins_ip"
  printf 'Environment="AIRTEK_JENKINS_ADMIN_PASSWORD=%s"\n' "$admin_password"
} >/etc/systemd/system/jenkins.service.d/airtek.conf
chmod 0600 /etc/systemd/system/jenkins.service.d/airtek.conf

install -d -m 0755 /etc/ssh/sshd_config.d
printf '%s\n' 'PasswordAuthentication no' 'PermitRootLogin prohibit-password' \
  >/etc/ssh/sshd_config.d/20-airtek-hardening.conf
sshd -t
systemctl reload ssh
passwd -l root >/dev/null

install -m 0755 /dev/stdin /usr/local/sbin/airtek-docker-firewall <<'EOF'
#!/bin/sh
set -eu
external_interface=$(ip route show default | awk '{ print $5; exit }')
iptables -N DOCKER-USER 2>/dev/null || true
iptables -F DOCKER-USER
iptables -A DOCKER-USER -m conntrack --ctstate RELATED,ESTABLISHED -j ACCEPT
iptables -A DOCKER-USER -i lo -j ACCEPT
iptables -A DOCKER-USER -i "$external_interface" -j DROP
iptables -A DOCKER-USER -j RETURN
EOF
install -m 0644 /dev/stdin /etc/systemd/system/airtek-docker-firewall.service <<'EOF'
[Unit]
Description=AIRTEK Docker ingress boundary
After=docker.service
Requires=docker.service

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/airtek-docker-firewall
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
EOF
install -m 0644 /dev/stdin /etc/nftables.conf <<'EOF'
#!/usr/sbin/nft -f
flush ruleset
table inet filter {
  chain input {
    type filter hook input priority filter; policy drop;
    ct state established,related accept
    iifname "lo" accept
    ip protocol icmp accept
    ip6 nexthdr ipv6-icmp accept
    udp sport 67 udp dport 68 accept
    ip saddr 10.10.14.0/24 tcp dport 22 accept
    ip saddr { 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16 } tcp dport 8080 accept
  }
  chain forward { type filter hook forward priority filter; policy accept; }
  chain output { type filter hook output priority filter; policy accept; }
}
EOF

systemctl daemon-reload
systemctl enable --now docker nftables qemu-guest-agent airtek-docker-firewall
systemctl enable --now jenkins
systemctl --no-pager --full status jenkins | sed -n '1,20p'
printf 'Jenkins URL: http://%s:8080/\n' "$jenkins_ip"
printf 'Initial admin password is stored at %s (root-readable).\n' "$admin_password_file"
