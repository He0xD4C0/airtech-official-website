#!/bin/bash
set -euo pipefail

if [ "$(id -u)" -ne 0 ]; then
  echo "Run this script as root on the dedicated Debian 12 Jenkins VM." >&2
  exit 2
fi

repo_root=$(cd "$(dirname "$0")/../.." && pwd)
export DEBIAN_FRONTEND=noninteractive
install -d -m 0755 /etc/apt/keyrings
printf '%s\n' 'Acquire::ForceIPv4 "true";' 'Acquire::Retries "5";' \
  >/etc/apt/apt.conf.d/99airtek-network
rm -f \
  /etc/apt/sources.list.d/adoptium.list \
  /etc/apt/sources.list.d/jenkins.list \
  /etc/apt/sources.list.d/nodesource.list \
  /etc/apt/sources.list.d/trivy.list
apt-get update
apt-get install -y \
  adduser build-essential ca-certificates curl fontconfig git gnupg jq lsb-base \
  net-tools nftables openssh-server openssl pkg-config qemu-guest-agent \
  sysvinit-utils unzip xz-utils

curl -4 -fsSL https://download.docker.com/linux/debian/gpg \
  | gpg --dearmor --yes -o /etc/apt/keyrings/docker.gpg
printf '%s\n' \
  'deb [arch=amd64 signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/debian bookworm stable' \
  >/etc/apt/sources.list.d/docker.list

apt-get update
apt-get install -y \
  docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

download_dir=/var/cache/airtek/downloads
install -d -m 0755 "$download_dir"
download_verified() {
  local url=$1 path=$2 sha256=$3
  if ! printf '%s  %s\n' "$sha256" "$path" | sha256sum --check --status 2>/dev/null; then
    if curl -4 -fL --retry 8 --retry-all-errors --continue-at - "$url" -o "$path" \
      && printf '%s  %s\n' "$sha256" "$path" | sha256sum --check --status; then
      return
    fi
    rm -f "$path"
    curl -4 -fL --retry 8 --retry-all-errors "$url" -o "$path"
    printf '%s  %s\n' "$sha256" "$path" | sha256sum --check --status
  fi
}

# Digests are pinned from the corresponding official release metadata. Mirrors
# are transport-only and can be overridden without weakening verification.
java_archive=OpenJDK21U-jdk_x64_linux_hotspot_21.0.12.1_1.tar.gz
download_verified \
  "${AIRTEK_ADOPTIUM_MIRROR:-https://mirrors.tuna.tsinghua.edu.cn/Adoptium}/21/jdk/x64/linux/$java_archive" \
  "$download_dir/$java_archive" \
  ce79869e1307ed8ee1e2baa86a412b1eb5b75d10a01006d788a6f968bcfaee94
java_dir=/opt/jdk-21.0.12.1+1
if [ ! -x "$java_dir/bin/java" ]; then
  tar -xzf "$download_dir/$java_archive" -C /opt
fi
ln -sfn "$java_dir" /opt/temurin-21
update-alternatives --install /usr/bin/java java /opt/temurin-21/bin/java 2121
update-alternatives --install /usr/bin/javac javac /opt/temurin-21/bin/javac 2121
update-alternatives --install /usr/bin/jar jar /opt/temurin-21/bin/jar 2121

node_archive=node-v22.23.3-linux-x64.tar.xz
download_verified \
  "${AIRTEK_NODE_MIRROR:-https://mirrors.nju.edu.cn/nodejs-release}/v22.23.3/$node_archive" \
  "$download_dir/$node_archive" \
  df450af89261115ef9f9e3830c3eeb2cc9213b63c720b1af623cb5dcbe2e02de
install -d -m 0755 /usr/local/lib/nodejs
if [ ! -x /usr/local/lib/nodejs/node-v22.23.3-linux-x64/bin/node ]; then
  tar -xJf "$download_dir/$node_archive" -C /usr/local/lib/nodejs
fi
for command in node npm npx corepack; do
  ln -sfn "/usr/local/lib/nodejs/node-v22.23.3-linux-x64/bin/$command" \
    "/usr/local/bin/$command"
done

jenkins_deb=jenkins_2.568.3_all.deb
download_verified \
  "${AIRTEK_JENKINS_MIRROR:-https://mirrors.tuna.tsinghua.edu.cn/jenkins/debian-stable}/$jenkins_deb" \
  "$download_dir/$jenkins_deb" \
  05d00283415f902f85602eda67913cb874d7493119d109fe47122e287d24497c
apt-get install -y "$download_dir/$jenkins_deb"
systemctl stop jenkins

github_proxy=${AIRTEK_GITHUB_PROXY:-https://ghproxy.net/https://github.com}
trivy_archive=trivy_0.74.0_Linux-64bit.tar.gz
download_verified \
  "$github_proxy/aquasecurity/trivy/releases/download/v0.74.0/$trivy_archive" \
  "$download_dir/$trivy_archive" \
  2ae6fe3ee734b7fdf11335663e18c75ea12dccc76062f09f164a3b0f8be4371a
tar -xzf "$download_dir/$trivy_archive" -C /usr/local/bin trivy
chmod 0755 /usr/local/bin/trivy

export COREPACK_NPM_REGISTRY=${AIRTEK_NPM_REGISTRY:-https://registry.npmmirror.com}
npm config set registry "$COREPACK_NPM_REGISTRY"
corepack enable
corepack install --global pnpm@11.19.0
corepack pnpm dlx playwright@1.55.0 install-deps chromium

if ! id jenkins-agent >/dev/null 2>&1; then
  useradd --create-home --home-dir /var/lib/jenkins-agent --shell /bin/bash jenkins-agent
fi
usermod -aG docker jenkins-agent
gpasswd -d jenkins docker >/dev/null 2>&1 || true
install -d -o jenkins-agent -g jenkins-agent -m 0700 /var/lib/jenkins-agent/.ssh
install -d -o jenkins-agent -g jenkins-agent -m 0755 /var/cache/airtek/{cargo,pnpm,playwright,buildkit,trivy}

rustup_init=$download_dir/rustup-init
download_verified \
  "${AIRTEK_RUSTUP_MIRROR:-https://rsproxy.cn/rustup}/dist/x86_64-unknown-linux-gnu/rustup-init" \
  "$rustup_init" \
  dda7234360b7f578ca8b0ddcb80145646fa61a67c1720a5abc7051b35c9fcb71
chmod 0755 "$rustup_init"
rust_env='RUSTUP_DIST_SERVER=https://rsproxy.cn RUSTUP_UPDATE_ROOT=https://rsproxy.cn/rustup'
su -s /bin/bash jenkins-agent -c \
  "$rust_env '$rustup_init' -y --profile minimal --default-toolchain 1.98.0"
su -s /bin/bash jenkins-agent -c \
  "$rust_env /var/lib/jenkins-agent/.cargo/bin/rustup toolchain install 1.88.0 --profile minimal"
su -s /bin/bash jenkins-agent -c \
  "$rust_env /var/lib/jenkins-agent/.cargo/bin/rustup component add rustfmt clippy --toolchain 1.98.0"

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
plugin_manager=$download_dir/jenkins-plugin-manager-${plugin_manager_version}.jar
download_verified \
  "$github_proxy/jenkinsci/plugin-installation-manager-tool/releases/download/${plugin_manager_version}/jenkins-plugin-manager-${plugin_manager_version}.jar" \
  "$plugin_manager" \
  aa720c79c658cacc54ae1156252e6eafb770cdc0c8b4f57d51d043e42e956c89
JENKINS_UC_DOWNLOAD=${AIRTEK_JENKINS_MIRROR:-https://mirrors.tuna.tsinghua.edu.cn/jenkins} \
  java -jar "$plugin_manager" \
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
passwd -l deploy >/dev/null

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
After=nftables.service docker.service
Requires=nftables.service docker.service

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/airtek-docker-firewall
RemainAfterExit=yes

[Install]
WantedBy=multi-user.target
EOF
install -d -m 0755 /etc/systemd/system/docker.service.d
install -m 0644 /dev/stdin /etc/systemd/system/docker.service.d/airtek.conf <<'EOF'
[Unit]
After=nftables.service
Requires=nftables.service
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
systemctl enable nftables docker qemu-guest-agent airtek-docker-firewall
systemctl restart nftables
systemctl restart docker
systemctl restart airtek-docker-firewall
systemctl start qemu-guest-agent
systemctl enable --now jenkins
systemctl --no-pager --full status jenkins | sed -n '1,20p'
printf 'Jenkins URL: http://%s:8080/\n' "$jenkins_ip"
printf 'Initial admin password is stored at %s (root-readable).\n' "$admin_password_file"
